use crate::config::Config;
use crate::metrics;
use crate::refresh::initialize_pool_data;
use crate::signer::{LocalKeypairSigner, TransactionSigner};
use crate::transaction::build_and_send_transaction;
use anyhow::Context;
use solana_client::rpc_client::RpcClient;
use solana_sdk::address_lookup_table::AddressLookupTableAccount;
use solana_sdk::hash::Hash;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::{
    address_lookup_table::state::AddressLookupTable, compute_budget::ComputeBudgetInstruction,
};
use spl_associated_token_account::get_associated_token_address_with_program_id;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

pub async fn run_bot(config_path: &str) -> anyhow::Result<()> {
    let config = Config::load(config_path)?;
    info!("Configuration loaded successfully");

    let rpc_client = Arc::new(RpcClient::new(config.rpc.url.clone()));

    let sending_rpc_clients = if let Some(spam_config) = &config.spam {
        if spam_config.enabled {
            let mut seen = HashSet::new();
            let unique_urls = spam_config
                .sending_rpc_urls
                .iter()
                .filter(|url| seen.insert((*url).clone()))
                .cloned()
                .collect::<Vec<_>>();

            if unique_urls.len() < spam_config.sending_rpc_urls.len() {
                warn!(
                    "Duplicate sending RPC URLs detected ({} configured, {} unique). Duplicates can amplify 429 rate-limit failures.",
                    spam_config.sending_rpc_urls.len(),
                    unique_urls.len()
                );
            }

            unique_urls
                .iter()
                .map(|url| Arc::new(RpcClient::new(url.clone())))
                .collect::<Vec<_>>()
        } else {
            vec![rpc_client.clone()]
        }
    } else {
        vec![rpc_client.clone()]
    };

    let wallet_kp =
        load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    let wallet_signer = Arc::new(LocalKeypairSigner::new(
        Keypair::from_bytes(&wallet_kp.to_bytes()).context("Failed to clone wallet keypair")?,
    ));
    info!("Wallet loaded: {}", wallet_signer.pubkey());

    let initial_blockhash = rpc_client.get_latest_blockhash()?;
    let cached_blockhash = Arc::new(Mutex::new(initial_blockhash));

    let refresh_interval = Duration::from_secs(10);
    let blockhash_client = rpc_client.clone();
    let blockhash_cache = cached_blockhash.clone();
    let should_serialize_submissions = config
        .execution
        .as_ref()
        .and_then(|e| e.serialize_submissions)
        .unwrap_or(true);
    let submission_lock = Arc::new(Mutex::new(()));
    tokio::spawn(async move {
        blockhash_refresher(blockhash_client, blockhash_cache, refresh_interval).await;
    });
    tokio::spawn(async move {
        metrics::metrics_reporter(Duration::from_secs(30)).await;
    });

    for mint_config in &config.routing.mint_config_list {
        // Get the mint account info to check owner
        let mint_owner = rpc_client
            .get_account(&Pubkey::from_str(&mint_config.mint).unwrap())
            .unwrap()
            .owner;
        let wallet_token_account = get_associated_token_address_with_program_id(
            &wallet_kp.pubkey(),
            &Pubkey::from_str(&mint_config.mint).unwrap(),
            &mint_owner,
        );

        println!("   Token mint: {}", mint_config.mint);
        println!("   Wallet token ATA: {}", wallet_token_account);
        // Check if the PWEASE token account exists and create it if it doesn't
        println!("\n   Checking if token account exists...");
        loop {
            match rpc_client.get_account(&wallet_token_account) {
                Ok(_) => {
                    println!("   token account exists!");
                    break;
                }
                Err(_) => {
                    println!("   token account does not exist. Creating it...");

                    // Create the instruction to create the associated token account
                    let create_ata_ix =
                            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                                &wallet_kp.pubkey(), // Funding account
                                &wallet_kp.pubkey(), // Wallet account
                                &Pubkey::from_str(&mint_config.mint).unwrap(),   // Token mint
                                &spl_token::ID,      // Token program
                            );

                    // Get a recent blockhash
                    let blockhash = rpc_client.get_latest_blockhash()?;

                    let compute_unit_price_ix =
                        ComputeBudgetInstruction::set_compute_unit_price(1_000_000);
                    let compute_unit_limit_ix =
                        ComputeBudgetInstruction::set_compute_unit_limit(60_000);

                    // Create the transaction
                    let create_ata_tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
                        &[compute_unit_price_ix, compute_unit_limit_ix, create_ata_ix],
                        Some(&wallet_kp.pubkey()),
                        &[&wallet_kp],
                        blockhash,
                    );

                    // Send the transaction
                    match rpc_client.send_and_confirm_transaction(&create_ata_tx) {
                        Ok(sig) => {
                            println!("   token account created successfully! Signature: {}", sig);
                        }
                        Err(e) => {
                            println!("   Failed to create token account: {:?}", e);
                            return Err(anyhow::anyhow!("Failed to create token account"));
                        }
                    }
                }
            }
        }
    }

    for mint_config in &config.routing.mint_config_list {
        info!("Processing mint: {}", mint_config.mint);

        let pool_data = initialize_pool_data(
            &mint_config.mint,
            &wallet_signer.pubkey().to_string(),
            mint_config.raydium_pool_list.as_ref(),
            mint_config.raydium_cp_pool_list.as_ref(),
            mint_config.pump_pool_list.as_ref(),
            mint_config.meteora_dlmm_pool_list.as_ref(),
            mint_config.whirlpool_pool_list.as_ref(),
            mint_config.raydium_clmm_pool_list.as_ref(),
            mint_config.meteora_damm_pool_list.as_ref(),
            mint_config.solfi_pool_list.as_ref(),
            mint_config.meteora_damm_v2_pool_list.as_ref(),
            mint_config.vertigo_pool_list.as_ref(),
            rpc_client.clone(),
        )
        .await?;

        let mint_pool_data = Arc::new(Mutex::new(pool_data));

        // TODO: Add logic to periodically refresh pool data

        let config_clone = config.clone();
        let mint_config_clone = mint_config.clone();
        let sending_rpc_clients_clone = sending_rpc_clients.clone();
        let cached_blockhash_clone = cached_blockhash.clone();
        let submission_lock_clone = submission_lock.clone();
        let wallet_signer_clone = wallet_signer.clone();
        let mut lookup_table_accounts = mint_config_clone.lookup_table_accounts.unwrap_or_default();
        lookup_table_accounts.push("4sKLJ1Qoudh8PJyqBeuKocYdsZvxTcRShUt9aKqwhgvC".to_string());

        let mut lookup_table_accounts_list = vec![];

        for lookup_table_account in lookup_table_accounts {
            match Pubkey::from_str(&lookup_table_account) {
                Ok(pubkey) => {
                    match rpc_client.get_account(&pubkey) {
                        Ok(account) => {
                            match AddressLookupTable::deserialize(&account.data) {
                                Ok(lookup_table) => {
                                    let lookup_table_account = AddressLookupTableAccount {
                                        key: pubkey,
                                        addresses: lookup_table.addresses.into_owned(),
                                    };
                                    lookup_table_accounts_list.push(lookup_table_account);
                                    info!("   Successfully loaded lookup table: {}", pubkey);
                                }
                                Err(e) => {
                                    error!(
                                        "   Failed to deserialize lookup table {}: {}",
                                        pubkey, e
                                    );
                                    continue; // Skip this lookup table but continue processing others
                                }
                            }
                        }
                        Err(e) => {
                            error!("   Failed to fetch lookup table account {}: {}", pubkey, e);
                            continue; // Skip this lookup table but continue processing others
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "   Invalid lookup table pubkey string {}: {}",
                        lookup_table_account, e
                    );
                    continue; // Skip this lookup table but continue processing others
                }
            }
        }
        if lookup_table_accounts_list.is_empty() {
            warn!("   Warning: No valid lookup tables were loaded");
        } else {
            info!(
                "   Loaded {} lookup tables successfully",
                lookup_table_accounts_list.len()
            );
        }

        tokio::spawn(async move {
            let process_delay = Duration::from_millis(mint_config_clone.process_delay);
            let mut last_submission_blockhash = Hash::default();
            let mut submitted_once = false;
            let mut rate_limit_streak = 0u32;
            let startup_jitter_ms = config_clone
                .spam
                .as_ref()
                .and_then(|s| s.worker_startup_jitter_ms)
                .unwrap_or(500);
            let rate_limit_cooldown_base_ms = config_clone
                .spam
                .as_ref()
                .and_then(|s| s.rate_limit_cooldown_base_ms)
                .unwrap_or(250);
            let rate_limit_cooldown_max_ms = config_clone
                .spam
                .as_ref()
                .and_then(|s| s.rate_limit_cooldown_max_ms)
                .unwrap_or(8000);

            // Prevent all mint workers from submitting in lock-step, which can trigger burst 429s.
            if startup_jitter_ms > 0 {
                tokio::time::sleep(Duration::from_millis(
                    rand::random::<u64>() % startup_jitter_ms,
                ))
                .await;
            }

            loop {
                let latest_blockhash = {
                    let guard = cached_blockhash_clone.lock().await;
                    *guard
                };

                if submitted_once && latest_blockhash == last_submission_blockhash {
                    tokio::time::sleep(process_delay).await;
                    continue;
                }

                let guard = mint_pool_data.lock().await;

                let result = if should_serialize_submissions {
                    let _submit_guard = submission_lock_clone.lock().await;
                    build_and_send_transaction(
                        wallet_signer_clone.as_ref(),
                        &config_clone,
                        &*guard, // Dereference the guard here
                        &sending_rpc_clients_clone,
                        latest_blockhash,
                        &lookup_table_accounts_list,
                    )
                    .await
                } else {
                    build_and_send_transaction(
                        wallet_signer_clone.as_ref(),
                        &config_clone,
                        &*guard, // Dereference the guard here
                        &sending_rpc_clients_clone,
                        latest_blockhash,
                        &lookup_table_accounts_list,
                    )
                    .await
                };

                match result {
                    Ok(signatures) => {
                        info!(
                            "Transactions sent successfully for mint {}",
                            mint_config_clone.mint
                        );
                        if signatures.is_empty() {
                            info!(
                                "Paper-trading mode active: no transaction broadcast for mint {}",
                                mint_config_clone.mint
                            );
                        } else {
                            for signature in signatures {
                                info!("  Signature: {}", signature);
                            }
                        }
                        submitted_once = true;
                        last_submission_blockhash = latest_blockhash;
                        rate_limit_streak = 0;
                    }
                    Err(e) => {
                        error!(
                            "Error sending transaction for mint {}: {}",
                            mint_config_clone.mint, e
                        );

                        if crate::transaction::is_rate_limit_error(&e.to_string()) {
                            rate_limit_streak = rate_limit_streak.saturating_add(1);
                            let cooldown_ms = rate_limit_cooldown_base_ms
                                .saturating_mul(2u64.saturating_pow(rate_limit_streak.min(4)))
                                .min(rate_limit_cooldown_max_ms);
                            warn!(
                                "Mint {} hit RPC 429 rate limits (streak {}). Cooling down for {}ms",
                                mint_config_clone.mint, rate_limit_streak, cooldown_ms
                            );
                            metrics::inc_cooldown_events();
                            tokio::time::sleep(Duration::from_millis(cooldown_ms)).await;
                        } else {
                            rate_limit_streak = 0;
                        }
                    }
                }

                tokio::time::sleep(process_delay).await;
            }
        });
    }

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn blockhash_refresher(
    rpc_client: Arc<RpcClient>,
    cached_blockhash: Arc<Mutex<Hash>>,
    refresh_interval: Duration,
) {
    loop {
        match rpc_client.get_latest_blockhash() {
            Ok(blockhash) => {
                let mut guard = cached_blockhash.lock().await;
                *guard = blockhash;
                info!("Blockhash refreshed: {}", blockhash);
            }
            Err(e) => {
                error!("Failed to refresh blockhash: {:?}", e);
            }
        }
        tokio::time::sleep(refresh_interval).await;
    }
}

fn load_keypair(private_key: &str) -> anyhow::Result<Keypair> {
    if let Ok(keypair) = bs58::decode(private_key)
        .into_vec()
        .map_err(|e| anyhow::anyhow!("Failed to decode base58: {}", e))
        .and_then(|bytes| {
            Keypair::from_bytes(&bytes).map_err(|e| anyhow::anyhow!("Invalid keypair bytes: {}", e))
        })
    {
        return Ok(keypair);
    }

    if let Ok(keypair) = solana_sdk::signature::read_keypair_file(private_key) {
        return Ok(keypair);
    }

    anyhow::bail!("Failed to load keypair from: {}", private_key)
}
