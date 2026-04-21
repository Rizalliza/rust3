use crate::config::Config;
use crate::dex::raydium::{raydium_authority, raydium_cp_authority};
use crate::dex::solfi::constants::solfi_program_id;
use crate::dex::vertigo::constants::vertigo_program_id;
use crate::metrics;
use crate::paper_trading::{append_record_csv, PaperTradeRecord};
use crate::pools::MintPoolData;
use crate::signer::TransactionSigner;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSimulateTransactionConfig;
use solana_program::instruction::Instruction;
use solana_sdk::address_lookup_table::AddressLookupTableAccount;
use solana_sdk::commitment_config::CommitmentLevel;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::hash::Hash;
use solana_sdk::message::v0::Message;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;
use solana_sdk::{instruction::Instruction as SolanaInstruction, program_pack::Pack, signature::Signer, transaction::Transaction};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::constants::sol_mint;
use crate::dex::meteora::constants::{
    damm_program_id, damm_v2_event_authority, damm_v2_pool_authority, damm_v2_program_id,
    dlmm_event_authority, dlmm_program_id, vault_program_id,
};
use crate::dex::pump::constants::{pump_fee_wallet, pump_program_id};
use crate::dex::raydium::constants::{
    raydium_clmm_program_id, raydium_cp_program_id, raydium_program_id,
};
use crate::dex::whirlpool::constants::whirlpool_program_id;
use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;
use solana_program::system_program;
use spl_token::state::Account as TokenAccount;
use spl_associated_token_account::ID as associated_token_program_id;
use spl_token::ID as token_program_id;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

#[derive(Default, Clone, Copy)]
struct RpcHealthStats {
    successes: u64,
    failures: u64,
    rate_limits: u64,
}

impl RpcHealthStats {
    fn score(&self) -> i64 {
        (self.successes as i64 * 3) - (self.failures as i64) - (self.rate_limits as i64 * 2)
    }
}

fn rpc_health_registry() -> &'static Mutex<std::collections::HashMap<usize, RpcHealthStats>> {
    static REGISTRY: OnceLock<Mutex<std::collections::HashMap<usize, RpcHealthStats>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn update_rpc_health(index: usize, success: bool, rate_limited: bool) {
    if let Ok(mut registry) = rpc_health_registry().lock() {
        let entry = registry.entry(index).or_default();
        if success {
            entry.successes = entry.successes.saturating_add(1);
        } else if rate_limited {
            entry.rate_limits = entry.rate_limits.saturating_add(1);
        } else {
            entry.failures = entry.failures.saturating_add(1);
        }
    }
}

fn prioritized_rpc_indices(client_count: usize) -> Vec<usize> {
    let mut indices = (0..client_count).collect::<Vec<_>>();

    if let Ok(registry) = rpc_health_registry().lock() {
        indices.sort_by(|a, b| {
            let score_a = registry.get(a).copied().unwrap_or_default().score();
            let score_b = registry.get(b).copied().unwrap_or_default().score();
            score_b.cmp(&score_a).then_with(|| a.cmp(b))
        });
    }

    indices
}

pub async fn build_and_send_transaction(
    signer: &dyn TransactionSigner,
    config: &Config,
    mint_pool_data: &MintPoolData,
    rpc_clients: &[Arc<RpcClient>],
    blockhash: Hash,
    address_lookup_table_accounts: &[AddressLookupTableAccount],
) -> anyhow::Result<Vec<Signature>> {
    metrics::inc_tx_attempted();
    let enable_flashloan = config.flashloan.as_ref().map_or(false, |k| k.enabled);
    let compute_unit_limit = config.bot.compute_unit_limit.max(250_000).min(600_000);
    let mut instructions = vec![];
    // Add a random number here to make each transaction unique
    let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(
        compute_unit_limit + rand::random::<u32>() % 1000,
    );
    instructions.push(compute_budget_ix);

    let compute_unit_price = config.spam.as_ref().map_or(1000, |s| s.compute_unit_price);
    let compute_budget_price_ix =
        ComputeBudgetInstruction::set_compute_unit_price(compute_unit_price);
    instructions.push(compute_budget_price_ix);

    let swap_ix = create_swap_instruction(
        signer.pubkey(),
        mint_pool_data,
        compute_unit_limit as u64,
        enable_flashloan,
    )?;

    let mut all_instructions = instructions.clone();

    debug!("Adding swap instruction");
    all_instructions.push(swap_ix);

    let message = Message::try_compile(
        &signer.pubkey(),
        &all_instructions,
        address_lookup_table_accounts,
        blockhash,
    )?;

    let tx = signer.sign_versioned_message(solana_sdk::message::VersionedMessage::V0(message))?;

    let require_simulation_success = config
        .execution
        .as_ref()
        .and_then(|e| e.require_simulation_success)
        .unwrap_or(true);
    let paper_trading_enabled = config
        .paper_trading
        .as_ref()
        .map(|p| p.enabled)
        .unwrap_or(false);

    if require_simulation_success {
        let simulation_client = rpc_clients
            .first()
            .ok_or_else(|| anyhow::anyhow!("no RPC clients configured for simulation"))?;
        if let Err(err) = ensure_simulation_passes(simulation_client, &tx) {
            metrics::inc_tx_sim_failed();
            metrics::inc_tx_send_failed();
            return Err(err);
        }
        metrics::inc_tx_sim_ok();
    }

    if paper_trading_enabled {
        let compute_unit_price = config.spam.as_ref().map_or(1000, |s| s.compute_unit_price);
        let priority_fee_lamports =
            (compute_unit_limit as u64).saturating_mul(compute_unit_price) / 1_000_000;
        let assumed_notional_lamports = config
            .paper_trading
            .as_ref()
            .and_then(|p| p.assumed_notional_lamports)
            .unwrap_or(1_000_000);
        let assumed_slippage_bps = config
            .paper_trading
            .as_ref()
            .and_then(|p| p.assumed_slippage_bps)
            .unwrap_or(50);
        let slippage_cost_lamports =
            assumed_notional_lamports.saturating_mul(assumed_slippage_bps) / 10_000;
        let estimated_net_lamports =
            -((priority_fee_lamports.saturating_add(slippage_cost_lamports)) as i64);
        let journal_path = config
            .paper_trading
            .as_ref()
            .and_then(|p| p.journal_path.clone())
            .unwrap_or_else(|| "paper_trades.csv".to_string());

        let record = PaperTradeRecord {
            timestamp_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default(),
            mint: mint_pool_data.mint.to_string(),
            simulation_passed: true,
            priority_fee_lamports,
            slippage_cost_lamports,
            estimated_net_lamports,
        };

        append_record_csv(&journal_path, &record)?;
        return Ok(Vec::new());
    }
    validate_wallet_accounts(
        rpc_clients.first().ok_or_else(|| anyhow::anyhow!("no RPC clients configured"))?,
        signer.pubkey(),
        mint_pool_data,
    )?;
    log_all_account_metadata(
        rpc_clients.first().ok_or_else(|| anyhow::anyhow!("no RPC clients configured"))?,
        signer.pubkey(),
        mint_pool_data,
    )?;

    let require_simulation_success = config
        .execution
        .as_ref()
        .and_then(|e| e.require_simulation_success)
        .unwrap_or(true);
    let paper_trading_enabled = config
        .paper_trading
        .as_ref()
        .map(|p| p.enabled)
        .unwrap_or(false);

    if require_simulation_success {
        let simulation_client = rpc_clients
            .first()
            .ok_or_else(|| anyhow::anyhow!("no RPC clients configured for simulation"))?;
        if let Err(err) = ensure_simulation_passes(simulation_client, &tx) {
            metrics::inc_tx_sim_failed();
            metrics::inc_tx_send_failed();
            return Err(err);
        }
        metrics::inc_tx_sim_ok();
    }

    if paper_trading_enabled {
        let compute_unit_price = config.spam.as_ref().map_or(1000, |s| s.compute_unit_price);
        let priority_fee_lamports =
            (compute_unit_limit as u64).saturating_mul(compute_unit_price) / 1_000_000;
        let assumed_notional_lamports = config
            .paper_trading
            .as_ref()
            .and_then(|p| p.assumed_notional_lamports)
            .unwrap_or(1_000_000);
        let assumed_slippage_bps = config
            .paper_trading
            .as_ref()
            .and_then(|p| p.assumed_slippage_bps)
            .unwrap_or(50);
        let slippage_cost_lamports =
            assumed_notional_lamports.saturating_mul(assumed_slippage_bps) / 10_000;
        let estimated_net_lamports =
            -((priority_fee_lamports.saturating_add(slippage_cost_lamports)) as i64);
        let journal_path = config
            .paper_trading
            .as_ref()
            .and_then(|p| p.journal_path.clone())
            .unwrap_or_else(|| "paper_trades.csv".to_string());

        let record = PaperTradeRecord {
            timestamp_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default(),
            mint: mint_pool_data.mint.to_string(),
            simulation_passed: true,
            priority_fee_lamports,
            slippage_cost_lamports,
            estimated_net_lamports,
        };

        append_record_csv(&journal_path, &record)?;
        return Ok(Vec::new());
    }

    let max_retries = config
        .spam
        .as_ref()
        .and_then(|s| s.max_retries)
        .unwrap_or(3);

    let mut signatures = Vec::new();
    let mut successful_send = false;

    for i in prioritized_rpc_indices(rpc_clients.len()) {
        let client = &rpc_clients[i];
        if successful_send {
            break;
        }

        debug!("Sending transaction through RPC client {}", i);

        let signature = match send_transaction_with_retries(client, &tx, max_retries).await {
            Ok(sig) => sig,
            Err(e) => {
                let e_str = e.to_string();
                if is_rate_limit_error(&e_str) {
                    update_rpc_health(i, false, true);
                    metrics::inc_rpc_429();
                    warn!(
                        "RPC client {} rate-limited transaction submission (429 Too Many Requests): {}",
                        i, e
                    );
                } else {
                    update_rpc_health(i, false, false);
                    metrics::inc_rpc_send_fail_non_429();
                    error!("Failed to send transaction through RPC client {}: {}", i, e);
                }
                continue;
            }
        };

        info!(
            "Transaction sent successfully through RPC client {}: {}",
            i, signature
        );
        signatures.push(signature);
        metrics::inc_tx_sent_success();
        update_rpc_health(i, true, false);
        successful_send = true;
    }

    if signatures.is_empty() {
        metrics::inc_tx_send_failed();
        anyhow::bail!("failed to send transaction via all configured RPC clients");
    }

    Ok(signatures)
}

fn ensure_simulation_passes(client: &RpcClient, tx: &VersionedTransaction) -> anyhow::Result<()> {
    let simulation = client.simulate_transaction_with_config(
        tx,
        RpcSimulateTransactionConfig {
            sig_verify: false,
            replace_recent_blockhash: true,
            ..Default::default()
        },
    )?;

    if let Some(err) = simulation.value.err {
    if let Some(logs) = simulation.value.logs.as_ref() {
        println!("--- simulation logs ---");
        for l in logs {
            println!("{}", l);
        }
        println!("--- end simulation logs ---");
    }

    if let Some(err) = simulation.value.err {
        if let Some(accounts) = simulation.value.accounts.as_ref() {
            println!("--- simulation account states ---");
            for (i, account) in accounts.iter().enumerate() {
                match account {
                    Some(account) => {
                        println!(
                            "account[{}]: lamports={} owner={} executable={} rent_epoch={}",
                            i,
                            account.lamports,
                            account.owner,
                            account.executable,
                            account.rent_epoch
                        );
                    }
                    None => {
                        println!("account[{}]: <none>", i);
                    }
                }
            }
            println!("--- end simulation account states ---");
        }

        anyhow::bail!("pre-send simulation failed: {:?}", err);
    }

    Ok(())
}

async fn send_transaction_with_retries(
    client: &RpcClient,
    tx: &VersionedTransaction,
    max_retries: u64,
) -> anyhow::Result<Signature> {
    let mut attempt = 0u64;
    let mut backoff = Duration::from_millis(250);

    loop {
        match client.send_transaction_with_config(
            tx,
            solana_client::rpc_config::RpcSendTransactionConfig {
                skip_preflight: true,
                max_retries: Some(max_retries as usize),
                preflight_commitment: Some(CommitmentLevel::Confirmed),
                ..Default::default()
            },
        ) {
            Ok(signature) => return Ok(signature),
            Err(err) => {
                attempt += 1;
                let err_str = err.to_string();

                if !is_rate_limit_error(&err_str) || attempt > max_retries {
                    return Err(err.into());
                }

                warn!(
                    "RPC send attempt {} rate-limited with 429; backing off for {:?}",
                    attempt, backoff
                );
                sleep(backoff).await;
                backoff = backoff.saturating_mul(2);
            }
        }
    }
}

pub(crate) fn is_rate_limit_error(err: &str) -> bool {
    err.contains("429")
        || err.contains("Too Many Requests")
        || err.contains("rate limit")
        || err.contains("Rate limit")
}

/// Helper function to derive the vault token account PDA address for a given mint
pub fn derive_vault_token_account(program_id: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault_token_account", mint.as_ref()], program_id)
}

// See https://docs.solanamevbot.com/home/onchain-bot/onchain-program for more information
fn create_swap_instruction(
    wallet: Pubkey,
    mint_pool_data: &MintPoolData,
    compute_unit_limit: u64,
    use_flashloan: bool,
) -> anyhow::Result<Instruction> {
    debug!("Creating swap instruction for all DEX types");

    let executor_program_id =
        Pubkey::from_str("MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz").unwrap();
    let fee_collector = Pubkey::from_str("6AGB9kqgSp2mQXwYpdrV4QVV8urvCaDS35U1wsLssy6H").unwrap();

    let pump_global_config =
        Pubkey::from_str("ADyA8hdefvWN2dbGGWFotbzWxrAvLW83WG6QCVXvJKqw").unwrap();
    let pump_authority = Pubkey::from_str("GS4CU59F31iL7aR2Q8zVS8DRrcRnXX1yjQ66TqNVQnaR").unwrap();
    let sysvar_instructions =
        Pubkey::from_str("Sysvar1nstructions1111111111111111111111111").unwrap();

    let sol_mint_pubkey = sol_mint();
    let wallet_sol_account = mint_pool_data.wallet_wsol_account;
    let wallet_x_account =
        spl_associated_token_account::get_associated_token_address_with_program_id(
            &wallet,
            &mint_pool_data.mint,
            &mint_pool_data.token_program,
        );

    let mut accounts = vec![
        AccountMeta::new_readonly(wallet, true), // 0. Wallet (signer)
        AccountMeta::new_readonly(sol_mint_pubkey, false), // 1. SOL mint
        AccountMeta::new(fee_collector, false), // 2. Fee collector
        AccountMeta::new(wallet_sol_account, false), // 3. Wallet SOL account
        AccountMeta::new_readonly(token_program_id, false), // 4. Token program
        AccountMeta::new_readonly(system_program::ID, false), // 5. System program
        AccountMeta::new_readonly(associated_token_program_id, false), // 6. Associated Token program
    ];

    let base_mint = sol_mint_pubkey;

    if use_flashloan {
        accounts.push(AccountMeta::new_readonly(
            Pubkey::from_str("5LFpzqgsxrSfhKwbaFiAEJ2kbc9QyimjKueswsyU4T3o").unwrap(),
            false,
        ));
        let token_pda = derive_vault_token_account(
            &Pubkey::from_str("MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz").unwrap(),
            &base_mint,
        );
        accounts.push(AccountMeta::new(token_pda.0, false));
    }

    accounts.push(AccountMeta::new_readonly(mint_pool_data.mint, false));
    accounts.push(AccountMeta::new_readonly(
        mint_pool_data.token_program,
        false,
    )); // Token program (SPL Token or Token 2022)
    accounts.push(AccountMeta::new(wallet_x_account, false));
    debug!(
        "Wallet mint ATA for {} is {} (token program: {})",
        mint_pool_data.mint,
        wallet_x_account,
        mint_pool_data.token_program
    );

    for pool in &mint_pool_data.raydium_pools {
        accounts.push(AccountMeta::new_readonly(raydium_program_id(), false));
        accounts.push(AccountMeta::new_readonly(raydium_authority(), false)); // Raydium authority
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new(pool.token_vault, false));
        accounts.push(AccountMeta::new(pool.sol_vault, false));
    }

    for pool in &mint_pool_data.raydium_cp_pools {
        accounts.push(AccountMeta::new_readonly(raydium_cp_program_id(), false));
        accounts.push(AccountMeta::new_readonly(raydium_cp_authority(), false)); // Raydium CP authority
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new_readonly(pool.amm_config, false));
        accounts.push(AccountMeta::new(pool.token_vault, false));
        accounts.push(AccountMeta::new(pool.sol_vault, false));
        accounts.push(AccountMeta::new(pool.observation, false));
    }

    for pool in &mint_pool_data.pump_pools {
        accounts.push(AccountMeta::new_readonly(pump_program_id(), false));
        accounts.push(AccountMeta::new_readonly(pump_global_config, false));
        accounts.push(AccountMeta::new_readonly(pump_authority, false));
        accounts.push(AccountMeta::new_readonly(pump_fee_wallet(), false));
        accounts.push(AccountMeta::new_readonly(pool.pool, false));
        accounts.push(AccountMeta::new(pool.token_vault, false));
        accounts.push(AccountMeta::new(pool.sol_vault, false));
        accounts.push(AccountMeta::new(pool.fee_token_wallet, false));
        accounts.push(AccountMeta::new(pool.coin_creator_vault_ata, false));
        accounts.push(AccountMeta::new_readonly(
            pool.coin_creator_vault_authority,
            false,
        ));
    }

    for pair in &mint_pool_data.dlmm_pairs {
        accounts.push(AccountMeta::new_readonly(dlmm_program_id(), false));
        accounts.push(AccountMeta::new(dlmm_event_authority(), false)); // DLMM event authority
        if let Some(memo_program) = pair.memo_program {
            accounts.push(AccountMeta::new_readonly(memo_program, false)); // Token 2022 memo program
        }
        accounts.push(AccountMeta::new(pair.pair, false));
        accounts.push(AccountMeta::new(pair.token_vault, false));
        accounts.push(AccountMeta::new(pair.sol_vault, false));
        accounts.push(AccountMeta::new(pair.oracle, false));
        for bin_array in &pair.bin_arrays {
            accounts.push(AccountMeta::new(*bin_array, false));
        }
    }

    for pool in &mint_pool_data.whirlpool_pools {
        accounts.push(AccountMeta::new_readonly(whirlpool_program_id(), false));
        if let Some(memo_program) = pool.memo_program {
            accounts.push(AccountMeta::new_readonly(memo_program, false)); // Token 2022 memo program
        }
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new(pool.oracle, false));
        accounts.push(AccountMeta::new(pool.x_vault, false));
        accounts.push(AccountMeta::new(pool.y_vault, false));
        for tick_array in &pool.tick_arrays {
            accounts.push(AccountMeta::new(*tick_array, false));
        }
    }

    for pool in &mint_pool_data.raydium_clmm_pools {
        accounts.push(AccountMeta::new_readonly(raydium_clmm_program_id(), false));
        if let Some(memo_program) = pool.memo_program {
            accounts.push(AccountMeta::new_readonly(memo_program, false)); // Token 2022 memo program
        }
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new_readonly(pool.amm_config, false));
        accounts.push(AccountMeta::new(pool.observation_state, false));
        accounts.push(AccountMeta::new(pool.bitmap_extension, false));
        accounts.push(AccountMeta::new(pool.x_vault, false));
        accounts.push(AccountMeta::new(pool.y_vault, false));
        for tick_array in &pool.tick_arrays {
            accounts.push(AccountMeta::new(*tick_array, false));
        }
    }

    for pool in &mint_pool_data.meteora_damm_pools {
        accounts.push(AccountMeta::new_readonly(damm_program_id(), false));
        accounts.push(AccountMeta::new_readonly(vault_program_id(), false));
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new(pool.token_x_vault, false));
        accounts.push(AccountMeta::new(pool.token_sol_vault, false));
        accounts.push(AccountMeta::new(pool.token_x_token_vault, false));
        accounts.push(AccountMeta::new(pool.token_sol_token_vault, false));
        accounts.push(AccountMeta::new(pool.token_x_lp_mint, false));
        accounts.push(AccountMeta::new(pool.token_sol_lp_mint, false));
        accounts.push(AccountMeta::new(pool.token_x_pool_lp, false));
        accounts.push(AccountMeta::new(pool.token_sol_pool_lp, false));
        accounts.push(AccountMeta::new(pool.admin_token_fee_x, false));
        accounts.push(AccountMeta::new(pool.admin_token_fee_sol, false));
    }

    for pool in &mint_pool_data.meteora_damm_v2_pools {
        accounts.push(AccountMeta::new_readonly(damm_v2_program_id(), false));
        accounts.push(AccountMeta::new_readonly(damm_v2_event_authority(), false));
        accounts.push(AccountMeta::new_readonly(damm_v2_pool_authority(), false));
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new(pool.token_x_vault, false));
        accounts.push(AccountMeta::new(pool.token_sol_vault, false));
    }

    for pool in &mint_pool_data.solfi_pools {
        accounts.push(AccountMeta::new_readonly(solfi_program_id(), false));
        accounts.push(AccountMeta::new_readonly(sysvar_instructions, false));
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new(pool.token_x_vault, false));
        accounts.push(AccountMeta::new(pool.token_sol_vault, false));
    }

    for pool in &mint_pool_data.vertigo_pools {
        accounts.push(AccountMeta::new_readonly(vertigo_program_id(), false));
        accounts.push(AccountMeta::new(pool.pool, false));
        accounts.push(AccountMeta::new_readonly(pool.pool_owner, false));
        accounts.push(AccountMeta::new(pool.token_x_vault, false));
        accounts.push(AccountMeta::new(pool.token_sol_vault, false));
    }

    info!("Executor account list ({} accounts):", accounts.len());
    for (i, acc) in accounts.iter().enumerate() {
        println!("{}: {}", i, acc.pubkey);
        info!(
            "  [{}] pubkey={} signer={} writable={}",
            i,
            acc.pubkey,
            acc.is_signer,
            acc.is_writable
        );
    }

    if let Err(err) =
        debug_validate_accounts(&accounts, &mint_pool_data.mint, &mint_pool_data.token_program)
    {
        warn!("account validation warning: {}", err);
    }

    let mut data = vec![28u8];

    let minimum_profit: u64 = 0;
    // When true, the bot will not fail the transaction even when it can't find a profitable arbitrage. It will just do nothing and succeed.
    let no_failure_mode = false;

    data.extend_from_slice(&minimum_profit.to_le_bytes());
    data.extend_from_slice(&compute_unit_limit.to_le_bytes());
    data.extend_from_slice(if no_failure_mode { &[1] } else { &[0] });
    data.extend_from_slice(&0u16.to_le_bytes()); // Keep this 0.
    data.extend_from_slice(if use_flashloan { &[1] } else { &[0] });

    Ok(Instruction {
        program_id: executor_program_id,
        accounts,
        data,
    })
}

fn debug_validate_accounts(
    accounts: &[AccountMeta],
    mint: &Pubkey,
    token_program: &Pubkey,
) -> anyhow::Result<()> {
    let wallet_x_account = spl_associated_token_account::get_associated_token_address_with_program_id(
        &accounts[0].pubkey,
        mint,
        token_program,
    );
    info!(
        "debug validation: mint={} token_program={} derived_wallet_ata={}",
        mint,
        token_program,
        wallet_x_account
    );
    for (i, acc) in accounts.iter().enumerate() {
        info!("debug validation account[{}]={}", i, acc.pubkey);
    }
    Ok(())
}

fn validate_wallet_accounts(
    rpc: &RpcClient,
    wallet: Pubkey,
    mint_pool_data: &MintPoolData,
) -> anyhow::Result<()> {
    let wsol = mint_pool_data.wallet_wsol_account;
    let mint = mint_pool_data.mint;
    let token_program = mint_pool_data.token_program;
    let ata = spl_associated_token_account::get_associated_token_address_with_program_id(
        &wallet,
        &mint,
        &token_program,
    );

    for (label, pubkey, expected_mint) in [
        ("wallet_sol_account", wsol, sol_mint()),
        ("derived_wallet_ata", ata, mint),
    ] {
        let account = rpc
            .get_account(&pubkey)
            .map_err(|e| anyhow::anyhow!("{} {} missing: {}", label, pubkey, e))?;

        info!(
            "validated {}={} owner={} lamports={} data_len={}",
            label,
            pubkey,
            account.owner,
            account.lamports,
            account.data.len()
        );

        if account.owner != token_program {
            anyhow::bail!(
                "{} {} has wrong owner {} (expected token program {})",
                label,
                pubkey,
                account.owner,
                token_program
            );
        }

        let token_account = TokenAccount::unpack(&account.data)
            .map_err(|e| anyhow::anyhow!("{} {} is not a valid token account: {}", label, pubkey, e))?;

        if token_account.mint != expected_mint {
            anyhow::bail!(
                "{} {} has wrong mint {} (expected {})",
                label,
                pubkey,
                token_account.mint,
                expected_mint
            );
        }
    }

    Ok(())
}

fn log_all_account_metadata(
    rpc: &RpcClient,
    wallet: Pubkey,
    mint_pool_data: &MintPoolData,
) -> anyhow::Result<()> {
    let mut accounts = vec![
        ("wallet", wallet),
        ("wallet_sol_account", mint_pool_data.wallet_wsol_account),
        (
            "derived_wallet_ata",
            spl_associated_token_account::get_associated_token_address_with_program_id(
                &wallet,
                &mint_pool_data.mint,
                &mint_pool_data.token_program,
            ),
        ),
        ("mint", mint_pool_data.mint),
        ("token_program", mint_pool_data.token_program),
    ];

    for pool in &mint_pool_data.raydium_pools {
        accounts.push(("raydium_pool", pool.pool));
        accounts.push(("raydium_token_vault", pool.token_vault));
        accounts.push(("raydium_sol_vault", pool.sol_vault));
    }

    for pool in &mint_pool_data.raydium_cp_pools {
        accounts.push(("raydium_cp_pool", pool.pool));
        accounts.push(("raydium_cp_amm_config", pool.amm_config));
        accounts.push(("raydium_cp_token_vault", pool.token_vault));
        accounts.push(("raydium_cp_sol_vault", pool.sol_vault));
        accounts.push(("raydium_cp_observation", pool.observation));
    }

    for pool in &mint_pool_data.pump_pools {
        accounts.push(("pump_pool", pool.pool));
        accounts.push(("pump_token_vault", pool.token_vault));
        accounts.push(("pump_sol_vault", pool.sol_vault));
        accounts.push(("pump_fee_token_wallet", pool.fee_token_wallet));
        accounts.push(("pump_coin_creator_vault_ata", pool.coin_creator_vault_ata));
        accounts.push(("pump_coin_creator_vault_authority", pool.coin_creator_vault_authority));
    }

    for pair in &mint_pool_data.dlmm_pairs {
        accounts.push(("dlmm_pair", pair.pair));
        accounts.push(("dlmm_token_vault", pair.token_vault));
        accounts.push(("dlmm_sol_vault", pair.sol_vault));
        accounts.push(("dlmm_oracle", pair.oracle));
        for bin in &pair.bin_arrays {
            accounts.push(("dlmm_bin_array", *bin));
        }
    }

    for pool in &mint_pool_data.whirlpool_pools {
        accounts.push(("whirlpool_pool", pool.pool));
        accounts.push(("whirlpool_oracle", pool.oracle));
        accounts.push(("whirlpool_x_vault", pool.x_vault));
        accounts.push(("whirlpool_y_vault", pool.y_vault));
        for tick in &pool.tick_arrays {
            accounts.push(("whirlpool_tick_array", *tick));
        }
    }

    for pool in &mint_pool_data.raydium_clmm_pools {
        accounts.push(("raydium_clmm_pool", pool.pool));
        accounts.push(("raydium_clmm_amm_config", pool.amm_config));
        accounts.push(("raydium_clmm_observation_state", pool.observation_state));
        accounts.push(("raydium_clmm_bitmap_extension", pool.bitmap_extension));
        accounts.push(("raydium_clmm_x_vault", pool.x_vault));
        accounts.push(("raydium_clmm_y_vault", pool.y_vault));
        for tick in &pool.tick_arrays {
            accounts.push(("raydium_clmm_tick_array", *tick));
        }
    }

    for pool in &mint_pool_data.meteora_damm_pools {
        accounts.push(("meteora_damm_pool", pool.pool));
        accounts.push(("meteora_damm_token_x_vault", pool.token_x_vault));
        accounts.push(("meteora_damm_token_sol_vault", pool.token_sol_vault));
        accounts.push(("meteora_damm_token_x_token_vault", pool.token_x_token_vault));
        accounts.push(("meteora_damm_token_sol_token_vault", pool.token_sol_token_vault));
        accounts.push(("meteora_damm_token_x_lp_mint", pool.token_x_lp_mint));
        accounts.push(("meteora_damm_token_sol_lp_mint", pool.token_sol_lp_mint));
        accounts.push(("meteora_damm_token_x_pool_lp", pool.token_x_pool_lp));
        accounts.push(("meteora_damm_token_sol_pool_lp", pool.token_sol_pool_lp));
        accounts.push(("meteora_damm_admin_token_fee_x", pool.admin_token_fee_x));
        accounts.push(("meteora_damm_admin_token_fee_sol", pool.admin_token_fee_sol));
    }

    for pool in &mint_pool_data.meteora_damm_v2_pools {
        accounts.push(("meteora_damm_v2_pool", pool.pool));
        accounts.push(("meteora_damm_v2_token_x_vault", pool.token_x_vault));
        accounts.push(("meteora_damm_v2_token_sol_vault", pool.token_sol_vault));
    }

    for pool in &mint_pool_data.solfi_pools {
        accounts.push(("solfi_pool", pool.pool));
        accounts.push(("solfi_token_x_vault", pool.token_x_vault));
        accounts.push(("solfi_token_sol_vault", pool.token_sol_vault));
    }

    for pool in &mint_pool_data.vertigo_pools {
        accounts.push(("vertigo_pool", pool.pool));
        accounts.push(("vertigo_pool_owner", pool.pool_owner));
        accounts.push(("vertigo_token_x_vault", pool.token_x_vault));
        accounts.push(("vertigo_token_sol_vault", pool.token_sol_vault));
    }

    println!("--- account ownership diagnostics ---");
    for (label, pubkey) in accounts {
        match rpc.get_account(&pubkey) {
            Ok(account) => {
                println!(
                    "[{}] {} owner={} data_len={} executable={}",
                    label,
                    pubkey,
                    account.owner,
                    account.data.len(),
                    account.executable
                );
            }
            Err(_) => {
                println!("[{}] {} MISSING", label, pubkey);
            }
        }
    }
    println!("--- end account ownership diagnostics ---");

    Ok(())
}
