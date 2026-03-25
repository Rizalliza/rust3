# Reverse Engineering Notes for the Executor Instruction

## What is required to reverse engineer this project

To reverse engineer the executor instruction safely, the project needs these elements collected and compared:

1. `src/transaction.rs`
   - This is where the live instruction is built.
   - The `create_swap_instruction(...)` function defines:
     - program id
     - account list order
     - instruction data layout
     - compute unit settings
     - flashloan flag
   - This is the main file to inspect first.

2. Known-good reference files
   - `transaction2.rs`
   - `2-bot.rs`
   - `2-ata.rs`
   - These look like captured or experimental versions that can reveal:
     - account ordering changes
     - ATA creation behavior
     - logging patterns
     - instruction serialization differences

3. Program ABI clues
   - The executor program id:
     - `MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz`
   - Known log:
     - `Program log: SolanaMevBot.com`
   - The instruction discriminator byte currently used:
     - `28u8`
   - The instruction payload fields currently assembled:
     - `minimum_profit: u64`
     - `compute_unit_limit: u64`
     - `no_failure_mode: bool as u8`
     - reserved `u16`
     - `use_flashloan: bool as u8`

4. Account expectations
   - The executor instruction appears to require a strict account order.
   - The code currently includes accounts for:
     - wallet signer
     - SOL mint
     - fee collector
     - wallet SOL ATA
     - token program
     - system program
     - associated token program
     - optional flashloan vault accounts
     - mint-specific ATA and pool accounts for all supported DEXes
   - If one account is misordered or missing, the program can fail with:
     - `InvalidAccountData`
     - `InvalidInstructionData`

5. ATA prerequisites
   - `src/bot.rs` shows that the bot ensures base ATAs exist before processing:
     - WSOL
     - USDC
     - USD1
   - These are important because the executor uses wallet token accounts as part of swap execution.

6. Lookup table prerequisites
   - The project loads LUTs at runtime, including:
     - `4sKLJ1Qoudh8PJyqBeuKocYdsZvxTcRShUt9aKqwhgvC`
   - Missing or mismatched LUT data can break transaction compilation or simulation.

## Current instruction shape captured from the project

### Program id
`MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz`

### Instruction data layout
```
[0]    u8   = 28
[1..9] u64  = minimum_profit
[9..17] u64 = compute_unit_limit
[17]   u8   = no_failure_mode
[18..20] u16 = reserved, must be 0
[20]   u8   = use_flashloan
```

### Important note
The current simulation failure is:
- `InstructionError(2, InvalidAccountData)`

That suggests the byte layout may be correct, but one or more accounts still do not match the exact ABI expectations of the executor program.

## Files and code to keep in sync

- `src/transaction.rs`
  - builds the executor instruction
  - prints the account list for debugging
- `src/bot.rs`
  - loads LUTs
  - ensures ATAs exist
  - controls simulation and submission flow
- `src/pools.rs`
  - supplies mint and pool account data
- `src/dex/*`
  - each DEX-specific module defines pool account parsing and ordering

## Suggested next debugging step

Add a full account dump before instruction submission and compare it against the known-good account sequence from the reference files or the upstream program expectations.

A good debug output should include:
- account index
- pubkey
- signer flag
- writable flag
- derived token program / ATA relationships
- DEX-specific pool account clusters

## Summary

The project is already partially reverse engineered:
- the executor program id is known
- the instruction payload layout is known
- the runtime account list is assembled for all supported DEXes
- ATA and LUT prerequisites are already handled

What is still needed is exact account-order verification against the on-chain ABI to eliminate `InvalidAccountData`.

==================

use crate:: ata:: ensure_base_atas_exist;
use crate:: config:: Config;
use crate:: pool_refreshers:: PoolDataRefresher;
use crate:: refresh:: initialize_pools_from_markets;
use crate:: transaction:: build_and_send_transaction;
use anyhow:: Context;
use solana_client:: rpc_client:: RpcClient;
use solana_sdk:: address_lookup_table:: state:: AddressLookupTable;
use solana_sdk:: address_lookup_table:: AddressLookupTableAccount;
use solana_sdk:: hash:: Hash;
use solana_sdk:: pubkey:: Pubkey;
use solana_sdk:: signature:: Keypair;
use solana_sdk:: signer:: Signer;
use std:: str:: FromStr;
use std:: sync:: Arc;
use std:: time:: { Duration, Instant };
use tokio:: sync:: Mutex;
use tracing:: { error, info, warn };

pub async fn run_bot(config_path: & str) -> anyhow:: Result < () > {
    let config = Config:: load(config_path)?;
    info!("Configuration loaded successfully");

    let rpc_client = Arc:: new(RpcClient::new(config.rpc.url.clone()));

let sending_rpc_clients = if let Some(spam_config) = & config.spam {
    if spam_config.enabled {
        spam_config
            .sending_rpc_urls
            .iter()
            .map(| url | Arc:: new (RpcClient:: new (url.clone())))
                .collect::<Vec<_>>()
        } else {
            vec![rpc_client.clone()]
        }
    } else {
        vec![rpc_client.clone()]
    };

    let wallet_kp =
        load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    info!("Wallet loaded: {}", wallet_kp.pubkey());

    let initial_blockhash = rpc_client.get_latest_blockhash()?;
    let cached_blockhash = Arc::new(Mutex::new(initial_blockhash));

    let refresh_interval = Duration::from_secs(10);
    let blockhash_client = rpc_client.clone();
    let blockhash_cache = cached_blockhash.clone();
    tokio::spawn(async move {
        blockhash_refresher(blockhash_client, blockhash_cache, refresh_interval).await;
    });

    // Initialize pools from markets config (auto-detect DEX types and group by mint)
    let mint_pool_data_map = initialize_pools_from_markets(
        &config.routing.markets,
        &wallet_kp.pubkey(),
        rpc_client.clone(),
    )
    .await?;

    info!("Initialized {} mints from markets config", mint_pool_data_map.len());

    // Ensure base token ATAs (WSOL, USDC, USD1) exist
    // Route token ATAs are NOT created here - the on-chain program creates them as needed
    ensure_base_atas_exist(&rpc_client, &wallet_kp)?;

    // Load lookup tables (global config)
    let mut lookup_table_addresses = config.routing.markets.lookup_table_accounts.clone().unwrap_or_default();
    lookup_table_addresses.push("4sKLJ1Qoudh8PJyqBeuKocYdsZvxTcRShUt9aKqwhgvC".to_string());

    let mut lookup_table_accounts_list = vec![];
    for lookup_table_account in &lookup_table_addresses {
        match Pubkey::from_str(lookup_table_account) {
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
                                error!("   Failed to deserialize lookup table {}: {}", pubkey, e);
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        error!("   Failed to fetch lookup table account {}: {}", pubkey, e);
                        continue;
                    }
                }
            }
            Err(e) => {
                error!("   Invalid lookup table pubkey string {}: {}", lookup_table_account, e);
                continue;
            }
        }
    }

    if lookup_table_accounts_list.is_empty() {
        warn!("   Warning: No valid lookup tables were loaded");
    } else {
        info!("   Loaded {} lookup tables successfully", lookup_table_accounts_list.len());
    }

    let lookup_table_accounts_list = Arc::new(lookup_table_accounts_list);
    let process_delay = Duration::from_millis(config.routing.markets.process_delay);
    let pool_refresh_interval = Duration::from_secs(5);

    // Spawn processing task for each mint
    for (mint, pool_data) in mint_pool_data_map {
        info!("Starting processing for mint: {}", mint);

        let mint_pool_data = Arc::new(Mutex::new(pool_data));
        let config_clone = config.clone();
        let sending_rpc_clients_clone = sending_rpc_clients.clone();
        let cached_blockhash_clone = cached_blockhash.clone();
        let wallet_bytes = wallet_kp.to_bytes();
        let wallet_kp_clone = Keypair::from_bytes(&wallet_bytes).unwrap();
        let lookup_tables = lookup_table_accounts_list.clone();
        let mint_str = mint.to_string();
        let rpc_client_clone = rpc_client.clone();

        tokio::spawn(async move {
            // Pool refresher for CLMM pools (DLMM, Whirlpool, Raydium CLMM, PancakeSwap, Byreal)
            let pool_refresher = PoolDataRefresher::new();
            let mut last_pool_refresh = Instant::now()
                .checked_sub(pool_refresh_interval)
                .unwrap_or_else(Instant::now);

            loop {
                // Check if pool refresh is needed (every 5 seconds)
                let now = Instant::now();
                if now.duration_since(last_pool_refresh) >= pool_refresh_interval {
                    let mut guard = mint_pool_data.lock().await;
                    match pool_refresher.refresh_all_pools(&mut guard, &rpc_client_clone, false) {
                        Ok(_) => {
                            last_pool_refresh = now;
                            info!("Pool data refreshed for mint {}", mint_str);
                        }
                        Err(e) => {
                            error!("Failed to refresh pool data for mint {}: {}", mint_str, e);
                        }
                    }
                    drop(guard);
                }

                let latest_blockhash = {
                    let guard = cached_blockhash_clone.lock().await;
                    *guard
                };

                let guard = mint_pool_data.lock().await;

                match build_and_send_transaction(
                    &wallet_kp_clone,
                    &config_clone,
                    &*guard,
                    &sending_rpc_clients_clone,
                    latest_blockhash,
                    &lookup_tables,
                )
                .await
                {
                    Ok(signatures) => {
                        info!("Transactions sent successfully for mint {}", mint_str);
                        for signature in signatures {
                            info!("  Signature: {}", signature);
                        }
                    }
                    Err(e) => {
                        error!("Error sending transaction for mint {}: {}", mint_str, e);
                    }
                }

                drop(guard);
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

================================

use crate::constants::{sol_mint, usdc_mint, usd1_mint};
use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account_idempotent,
};
use tracing::info;

/// Ensures a single ATA exists, creating it if necessary
fn ensure_ata_exists(
    rpc_client: &RpcClient,
    wallet_kp: &Keypair,
    mint: &Pubkey,
    mint_name: &str,
) -> Result<Pubkey> {
    let wallet = wallet_kp.pubkey();
    let ata = get_associated_token_address(&wallet, mint);

    info!("Checking {} ATA: {}", mint_name, ata);

    match rpc_client.get_account(&ata) {
        Ok(_) => {
            info!("{} ATA already exists", mint_name);
            Ok(ata)
        }
        Err(_) => {
            info!("{} ATA does not exist, creating...", mint_name);

            let create_ata_ix = create_associated_token_account_idempotent(
                &wallet,
                &wallet,
                mint,
                &spl_token::id(),
            );

            let blockhash = rpc_client
                .get_latest_blockhash()
                .context("Failed to get blockhash for ATA creation")?;

            let compute_unit_price_ix = ComputeBudgetInstruction::set_compute_unit_price(1_000_000);
            let compute_unit_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(60_000);

            let tx = Transaction::new_signed_with_payer(
                &[compute_unit_price_ix, compute_unit_limit_ix, create_ata_ix],
                Some(&wallet),
                &[wallet_kp],
                blockhash,
            );

            let sig = rpc_client
                .send_and_confirm_transaction(&tx)
                .context(format!("Failed to create {} ATA", mint_name))?;

            info!("{} ATA created successfully. Signature: {}", mint_name, sig);
            Ok(ata)
        }
    }
}

/// Ensures all base token ATAs (WSOL, USDC, USD1) exist.
/// This should be called during bot initialization before processing pools.
pub fn ensure_base_atas_exist(rpc_client: &RpcClient, wallet_kp: &Keypair) -> Result<()> {
    info!("Verifying base token ATAs...");

    let wsol_ata = ensure_ata_exists(rpc_client, wallet_kp, &sol_mint(), "WSOL")?;
    let usdc_ata = ensure_ata_exists(rpc_client, wallet_kp, &usdc_mint(), "USDC")?;
    let usd1_ata = ensure_ata_exists(rpc_client, wallet_kp, &usd1_mint(), "USD1")?;

    info!("All base token ATAs verified/created successfully");
    info!("  WSOL ATA: {}", wsol_ata);
    info!("  USDC ATA: {}", usdc_ata);
    info!("  USD1 ATA: {}", usd1_ata);

    Ok(())
}
