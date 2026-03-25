# Onchain Program



The onchain program takes in mints and pools, and will calculate the most optimal arbitrage trade between them and execute the trades.

If you want maximum flexibility, you can write your own bot and just call the onchain program from there. This allow you to do whatever customization you want but still utilize the onchain program to get maximum arb opportunity.

Demo bot: [https://github.com/Cetipoo/solana-onchain-arbitrage-bot](https://github.com/Cetipoo/solana-onchain-arbitrage-bot)

**NOTE**: It's recommended to include this address lookup table as it has all the fixed keys needed: 4sKLJ1Qoudh8PJyqBeuKocYdsZvxTcRShUt9aKqwhgvC

**NOTE**: The bot should also append/load this fixed LUT at runtime in addition to any configured `lookup_table_accounts`; without it, v0 message compilation can fail with `AddressLoaderError` when fixed account keys are missing.

**NOTE**: It's recommended to use at least 250\_000 compute unit limit when only amm pools/dlmm, and at least 450\_000 compute unit limit with other pools. Ideally you may want 400\_000 - 600\_000 to be able to hit the big arbs and not being limited by compute unit. Using merge mint will need a bit more CU than just one mint.

Here's a simplified Rust example for how to interact with the program. You can include multiple X mints with their according pools here.

You can also check [https://github.com/Cetipoo/solana-onchain-arbitrage-bot](https://github.com/Cetipoo/solana-onchain-arbitrage-bot) to see how to extract all the keys for each pool type.

Current supported DEX [#current-supported-dex]

* Raydium
* Raydium CPMM
* Raydium CLMM
* Pumpfun Swap
* Meteora DLMM
* Meteora Dynamic Pool
* Meteora DAMM V2
* Orca whirlpool
* Humidifi
* Vertigo
* Heaven Dex
* Pancake Swap
* Byreal
* Futarchy

Example code to generate instruction [#example-code-to-generate-instruction]

```rust
fn derive_pump_pool_v2(base_mint: &Pubkey, pump_program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"pool-v2", base_mint.as_ref()], pump_program).0
}

pub fn generate_onchain_swap_multiple_mints_instruction(
    wallet: &Pubkey,
    base_mint: &Pubkey,
    wallet_base_account: &Pubkey,
    token_program: &Pubkey,
    system_program: &Pubkey,
    associated_token_program: &Pubkey,
    token_pools: &[(
        Pubkey, // X mint
        Pubkey, // Token program ID
        Pubkey, // Wallet X account
        Vec<(
            Pubkey, // Raydium program ID
            Pubkey, // V9: Base mint
            Pubkey, // Raydium authority
            // Raydium pools
            Pubkey, // Raydium AMM account
            Pubkey, // Raydium token X vault
            Pubkey, // Raydium base vault
        )>,
        Vec<(
            Pubkey, // Raydium CP program ID
            Pubkey, // V9: Base mint
            Pubkey, // Raydium CP authority
            // Raydium CP pools
            Pubkey, // Raydium CP pool
            Pubkey, // Raydium CP amm config
            Pubkey, // Raydium CP token X vault
            Pubkey, // Raydium CP base vault
            Pubkey, // Raydium CP observation
        )>,
        Vec<(
            Pubkey, // Pump program ID
            Pubkey, // V9: Base mint
            Pubkey, // Pump global config
            Pubkey, // Pump authority
            Pubkey, // Pump fee wallet
            // Pump pools
            Pubkey, // Pump pool
            Pubkey, // Pump token X account
            Pubkey, // Pump base account
            Pubkey, // Pump fee token wallet
            Pubkey, // Pump coin creator vault
            Pubkey, // Pump coin creator vault authority
            bool,   // Pump is cashback coin
        )>,
        Vec<(
            Pubkey,         // DLMM program ID
            Pubkey,         // V9: Base mint
            Pubkey,         // DLMM event authority
            Option<Pubkey>, // DLMM memo program v2 (Only needed for Token 2022)
            // DLMM pairs
            Pubkey,           // DLMM pair account
            Pubkey,           // DLMM token X vault
            Pubkey,           // DLMM base vault
            Pubkey,           // DLMM oracle
            Vec<AccountMeta>, // Bin array accounts
        )>,
        Vec<(
            Pubkey,         // Whirlpool program ID
            Pubkey,         // V9: Base mint
            Pubkey,         // Whirlpool memo program v2
            // Whirlpool pools
            Pubkey,           // Whirlpool pool
            Pubkey,           // Whirlpool oracle
            Pubkey,           // Whirlpool token X vault
            Pubkey,           // Whirlpool token base vault
            Vec<AccountMeta>, // Whirlpool tick array accounts
        )>,
        Vec<(
            Pubkey,         // Raydium CLMM program ID
            Pubkey,         // V9: Base mint
            Option<Pubkey>, // Raydium CLMM memo program v2 (Only needed for Token 2022)
            // Raydium CLMM pools
            Pubkey,           // Raydium CLMM pool
            Pubkey,           // Raydium CLMM amm config
            Pubkey,           // Raydium CLMM observation state
            Pubkey,           // Raydium CLMM bitmap extension
            Pubkey,           // Raydium CLMM token X vault
            Pubkey,           // Raydium CLMM token base vault
            Vec<AccountMeta>, // Raydium CLMM tick array accounts
        )>,
        Vec<(
            Pubkey, // Meteora program ID
            Pubkey, // V9: Base mint
            Pubkey, // Meteora vault program ID
            // Meteora pools
            Pubkey, // Meteora pool
            Pubkey, // Meteora pool token X vault
            Pubkey, // Meteora pool token base vault
            Pubkey, // Meteora pool token X token vault
            Pubkey, // Meteora pool token base token vault
            Pubkey, // Meteora pool token X lp mint
            Pubkey, // Meteora pool token base lp mint
            Pubkey, // Meteora pool token X pool lp
            Pubkey, // Meteora pool token base pool lp
            Pubkey, // Meteora pool admin token fee X
            Pubkey, // Meteora pool admin token fee base
        )>,
        Vec<(
            Pubkey, // Solfi program ID
            Pubkey, // V9: Base mint
            Pubkey, // Sysvar instructions
            // Solfi pools
            Pubkey, // Solfi pool
            Pubkey, // Solfi token X vault
            Pubkey, // Solfi token base vault
        )>,
        Vec<(
            Pubkey, // DAMM V2 program ID
            Pubkey, // V9: Base mint
            Pubkey, // Event authority
            Pubkey, // Pool authority
            // DAMM V2 pools
            Pubkey, // DAMM V2 pool
            Pubkey, // DAMM V2 token X vault
            Pubkey, // DAMM V2 token base vault
        )>,
        Vec<(
            Pubkey, // Vertigo program ID
            Pubkey, // V9: Base mint
            // Vertigo pools
            Pubkey, // Vertigo pool
            Pubkey, // Vertigo pool owner
            Pubkey, // Vertigo token X vault
            Pubkey, // Vertigo token base vault
        )>,
        Vec<(
            Pubkey, // Heaven program ID
            Pubkey, // V9: Base mint
            // Heaven pools
            Pubkey, // Heaven pool
            Pubkey, // Heaven protocol config
            Pubkey, // Heaven token X vault
            Pubkey, // Heaven token base vault
        )>,
    )],
    program_id: &Pubkey,
    minimum_profit: u64,
    compute_unit_limit: u32,
    no_failure_mode: bool,
    use_flashloan: bool,
) -> Instruction {
    let fee_collector = if use_flashloan {
        Pubkey::from_str("6AGB9kqgSp2mQXwYpdrV4QVV8urvCaDS35U1wsLssy6H").unwrap()
    } else {
        let fee_accounts = [
            Pubkey::from_str("GPpkDpzCDmYJY5qNhYmM14c7rct1zmkjWc2CjR5g7RZ1").unwrap(),
            Pubkey::from_str("J6c7noBHvWju4mMA3wXt3igbBSp2m9ATbA6cjMtAUged").unwrap(),
            Pubkey::from_str("BjsfwxDu7GX7RRW6oSRTpMkASdXAgCcHnXEcatqSfuuY").unwrap(),
        ];
        fee_accounts[rand::random::<usize>() % fee_accounts.len()]
    };
    let mut accounts = vec![
        AccountMeta::new(*wallet, true),
        AccountMeta::new_readonly(*base_mint, false),
        AccountMeta::new(fee_collector, false),
        AccountMeta::new(*wallet_base_account, false),
        AccountMeta::new_readonly(*token_program, false),
        AccountMeta::new_readonly(*system_program, false),
        AccountMeta::new_readonly(*associated_token_program, false),
    ];

    // Add flashloan accounts if enabled
    if use_flashloan {
        accounts.push(AccountMeta::new_readonly(
            Pubkey::from_str("5LFpzqgsxrSfhKwbaFiAEJ2kbc9QyimjKueswsyU4T3o").unwrap(),
            false,
        ));
        let token_pda = derive_vault_token_account(
            &Pubkey::from_str("MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz").unwrap(),
            base_mint,
        );
        accounts.push(AccountMeta::new(token_pda.0, false));
    }

    // Detect mixed mode by checking if any pool has a USDC base mint
    let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
    let mut has_usdc_base = false;

    // Check all pools to see if any have USDC as base mint
    for (
        _,
        _,
        _,
        raydium_pools,
        raydium_cp_pools,
        pump_pools,
        dlmm_pairs,
        whirlpool_pools,
        raydium_clmm_pools,
        meteora_pools,
        solfi_pools,
        dammv2_pools,
        vertigo_pools,
    ) in token_pools.iter()
    {
        // Check Raydium pools
        for (_, pool_base_mint, _, _, _, _) in raydium_pools.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        // Check Raydium CP pools
        for (_, pool_base_mint, _, _, _, _, _, _) in raydium_cp_pools.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        // Check Pump pools
        for (_, pool_base_mint, _, _, _, _, _, _, _, _, _, _) in pump_pools.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        // Check DLMM pairs
        for (_, pool_base_mint, _, _, _, _, _, _, _) in dlmm_pairs.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        // Check Whirlpool pools
        for (_, pool_base_mint, _, _, _, _, _, _) in whirlpool_pools.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        // Check Raydium CLMM pools
        for (_, pool_base_mint, _, _, _, _, _, _, _, _) in raydium_clmm_pools.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        // Check Meteora pools
        for (_, pool_base_mint, _, _, _, _, _, _, _, _, _, _, _, _) in meteora_pools.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        // Check Solfi pools
        for (_, pool_base_mint, _, _, _, _) in solfi_pools.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        // Check DAMM V2 pools
        for (_, pool_base_mint, _, _, _, _, _) in dammv2_pools.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        // Check Vertigo pools
        for (_, pool_base_mint, _, _, _, _) in vertigo_pools.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        // Check Heaven pools
        for (_, pool_base_mint, _, _, _, _) in heaven_pools.iter() {
            if *pool_base_mint == usdc_mint {
                has_usdc_base = true;
                break;
            }
        }

        if has_usdc_base {
            break;
        }
    }

    // If mixed mode is detected, add the required accounts
    if has_usdc_base {
        // Add mixed mode accounts
        let wallet_usdc_account = get_associated_token_address(wallet, &usdc_mint);
        let raydium_program_id =
            Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8").unwrap();
        let raydium_authority =
            Pubkey::from_str("5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1").unwrap();
        let sysvar_instructions = solana_program::sysvar::instructions::id();

        // Raydium SOL/USDC pool accounts (mainnet)
        let raydium_sol_usdc_pool =
            Pubkey::from_str("58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2").unwrap();
        let raydium_usdc_vault =
            Pubkey::from_str("HLmqeL62xR1QoZ1HKKbXRrdN1p3phKpxRMb2VVopvBBz").unwrap();
        let raydium_sol_vault =
            Pubkey::from_str("DQyrAcCrDXQ7NeoqGgDCZwBvWDcYmFCjSb9JtteuvPpz").unwrap();

        accounts.push(AccountMeta::new_readonly(usdc_mint, false));
        accounts.push(AccountMeta::new(wallet_usdc_account, false));
        accounts.push(AccountMeta::new_readonly(raydium_program_id, false));
        accounts.push(AccountMeta::new_readonly(raydium_authority, false));
        accounts.push(AccountMeta::new_readonly(sysvar_instructions, false));
        accounts.push(AccountMeta::new(raydium_sol_usdc_pool, false));
        accounts.push(AccountMeta::new(raydium_usdc_vault, false));
        accounts.push(AccountMeta::new(raydium_sol_vault, false));
    }

    // Add token mint accounts and their pools
    for (
        x_mint,
        token_program_id,
        wallet_x_account,
        raydium_pools,
        raydium_cp_pools,
        pump_pools,
        dlmm_pairs,
        whirlpool_pools,
        raydium_clmm_pools,
        meteora_pools,
        solfi_pools,
        dammv2_pools,
        vertigo_pools,
    ) in token_pools.iter()
    {
        // Add mint info
        accounts.push(AccountMeta::new_readonly(*x_mint, false));
        accounts.push(AccountMeta::new_readonly(*token_program_id, false));
        accounts.push(AccountMeta::new(*wallet_x_account, false));

        // Add Raydium pools
        for (program_id, pool_base_mint, authority, pool, x_vault, base_vault) in
            raydium_pools.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            accounts.push(AccountMeta::new_readonly(*authority, false));
            accounts.push(AccountMeta::new(*pool, false));
            accounts.push(AccountMeta::new(*x_vault, false));
            accounts.push(AccountMeta::new(*base_vault, false));
        }

        // Add Raydium CP pools
        for (
            program_id,
            pool_base_mint,
            authority,
            pool,
            amm_config,
            x_vault,
            base_vault,
            observation,
        ) in raydium_cp_pools.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            accounts.push(AccountMeta::new_readonly(*authority, false));
            accounts.push(AccountMeta::new(*pool, false));
            accounts.push(AccountMeta::new_readonly(*amm_config, false));
            accounts.push(AccountMeta::new(*x_vault, false));
            accounts.push(AccountMeta::new(*base_vault, false));
            accounts.push(AccountMeta::new(*observation, false));
        }

        // Add Pump pools
        for (
            program_id,
            pool_base_mint,
            global_config,
            authority,
            fee_wallet,
            pool,
            x_account,
            base_account,
            fee_token_wallet,
            coin_creator_vault,
            coin_creator_vault_authority,
            is_cashback_coin,
        ) in pump_pools.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            accounts.push(AccountMeta::new_readonly(*global_config, false));
            accounts.push(AccountMeta::new_readonly(*authority, false));
            accounts.push(AccountMeta::new(*fee_wallet, false));
            accounts.push(AccountMeta::new(*pool, false));
            accounts.push(AccountMeta::new(*x_account, false));
            accounts.push(AccountMeta::new(*base_account, false));
            accounts.push(AccountMeta::new(*fee_token_wallet, false));
            accounts.push(AccountMeta::new(*coin_creator_vault, false));
            accounts.push(AccountMeta::new_readonly(
                *coin_creator_vault_authority,
                false,
            ));
            let pump_program_id = Pubkey::from_str("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA").unwrap();
            let (global_volume_accumulator, _) = Pubkey::find_program_address(
                &[b"global_volume_accumulator"],
                &pump_program_id,
            );
            let (user_volume_accumulator, _) = Pubkey::find_program_address(
                &[b"user_volume_accumulator", wallet.as_ref()],
                &pump_program_id,
            );
            accounts.push(AccountMeta::new_readonly(global_volume_accumulator, false));
            accounts.push(AccountMeta::new(user_volume_accumulator, false));
            // Add fee_config and fee_program accounts for PumpSwap
            let fee_config =
                Pubkey::from_str("5PHirr8joyTMp9JMm6nW7hNDVyEYdkzDqazxPD7RaTjx").unwrap();
            let pump_fee_program =
                Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ").unwrap();
            accounts.push(AccountMeta::new_readonly(fee_config, false));
            accounts.push(AccountMeta::new_readonly(pump_fee_program, false));

            // Cashback-tail accounts for Pump AMM v2:
            // user_volume_accumulator WSOL ATA (mutable) + user_volume_accumulator (mutable).
            if *is_cashback_coin {
                let user_volume_accumulator_wsol_ata = get_associated_token_address(
                    &user_volume_accumulator,
                    &Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap(),
                );
                accounts.push(AccountMeta::new(user_volume_accumulator_wsol_ata, false));
                accounts.push(AccountMeta::new(user_volume_accumulator, false));
            }

            // Pump AMM v2 PDA is derived from the pool's base mint
            // (the X mint in our token_pools entry).
            let pool_v2 = derive_pump_pool_v2(x_mint, program_id);
            accounts.push(AccountMeta::new_readonly(pool_v2, false));
        }

        // Add DLMM pairs
        for (
            program_id,
            pool_base_mint,
            event_authority,
            memo_program,
            pair,
            x_vault,
            base_vault,
            oracle,
            bin_arrays,
        ) in dlmm_pairs.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            accounts.push(AccountMeta::new_readonly(*event_authority, false));
            if memo_program.is_some() {
                accounts.push(AccountMeta::new_readonly(memo_program.unwrap(), false));
            }
            accounts.push(AccountMeta::new(*pair, false));
            accounts.push(AccountMeta::new(*x_vault, false));
            accounts.push(AccountMeta::new(*base_vault, false));
            accounts.push(AccountMeta::new(*oracle, false));
            accounts.extend(bin_arrays.clone());
        }

        // Add Whirlpool pools
        for (
            program_id,
            pool_base_mint,
            memo_program,
            pool,
            oracle,
            x_vault,
            base_vault,
            tick_arrays,
        ) in whirlpool_pools.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            accounts.push(AccountMeta::new_readonly(MEMO_PROGRAM.clone(), false));
            accounts.push(AccountMeta::new(*pool, false));
            accounts.push(AccountMeta::new(*oracle, false)); // Oracle NEEDS to be writable for Whirlpool
            accounts.push(AccountMeta::new(*x_vault, false));
            accounts.push(AccountMeta::new(*base_vault, false));
            accounts.extend(tick_arrays.clone());
        }

        // Add Raydium CLMM pools
        for (
            program_id,
            pool_base_mint,
            memo_program,
            pool,
            amm_config,
            observation_state,
            bitmap_extension,
            x_vault,
            base_vault,
            tick_arrays,
        ) in raydium_clmm_pools.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            if memo_program.is_some() {
                accounts.push(AccountMeta::new_readonly(memo_program.unwrap(), false));
            }
            accounts.push(AccountMeta::new(*pool, false));
            accounts.push(AccountMeta::new_readonly(*amm_config, false));
            accounts.push(AccountMeta::new(*observation_state, false));
            accounts.push(AccountMeta::new(*bitmap_extension, false));
            accounts.push(AccountMeta::new(*x_vault, false));
            accounts.push(AccountMeta::new(*base_vault, false));
            accounts.extend(tick_arrays.clone());
        }

        // Add Meteora pools
        for (
            program_id,
            pool_base_mint,
            vault_program_id,
            pool,
            x_vault,
            base_vault,
            x_token_vault,
            base_token_vault,
            x_lp_mint,
            base_lp_mint,
            x_pool_lp,
            base_pool_lp,
            x_admin_fee,
            base_admin_fee,
        ) in meteora_pools.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            accounts.push(AccountMeta::new_readonly(*vault_program_id, false));
            accounts.push(AccountMeta::new(*pool, false));
            accounts.push(AccountMeta::new(*x_vault, false));
            accounts.push(AccountMeta::new(*base_vault, false));
            accounts.push(AccountMeta::new(*x_token_vault, false));
            accounts.push(AccountMeta::new(*base_token_vault, false));
            accounts.push(AccountMeta::new(*x_lp_mint, false));
            accounts.push(AccountMeta::new(*base_lp_mint, false));
            accounts.push(AccountMeta::new(*x_pool_lp, false));
            accounts.push(AccountMeta::new(*base_pool_lp, false));
            accounts.push(AccountMeta::new(*x_admin_fee, false));
            accounts.push(AccountMeta::new(*base_admin_fee, false));
        }

        // Add Solfi pools
        for (program_id, pool_base_mint, sysvar_instructions, pool, x_vault, base_vault) in
            solfi_pools.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            accounts.push(AccountMeta::new_readonly(*sysvar_instructions, false));
            accounts.push(AccountMeta::new(*pool, false));
            accounts.push(AccountMeta::new(*x_vault, false));
            accounts.push(AccountMeta::new(*base_vault, false));
        }

        // Add DAMM V2 pools
        for (
            program_id,
            pool_base_mint,
            event_authority,
            pool_authority,
            pool,
            x_vault,
            base_vault,
        ) in dammv2_pools.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            accounts.push(AccountMeta::new_readonly(*event_authority, false));
            accounts.push(AccountMeta::new_readonly(*pool_authority, false));
            accounts.push(AccountMeta::new(*pool, false));
            accounts.push(AccountMeta::new(*x_vault, false));
            accounts.push(AccountMeta::new(*base_vault, false));
            accounts.push(AccountMeta::new_readonly(
                Pubkey::from_str("Sysvar1nstructions1111111111111111111111111").unwrap(),
                false,
            ));
        }

        // Add Vertigo pools
        for (program_id, pool_base_mint, pool, pool_owner, x_vault, base_vault) in
            vertigo_pools.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            accounts.push(AccountMeta::new(*pool, false));
            accounts.push(AccountMeta::new_readonly(*pool_owner, false));
            accounts.push(AccountMeta::new(*x_vault, false));
            accounts.push(AccountMeta::new(*base_vault, false));
        }

        // Add Heaven pools
        for (program_id, pool_base_mint, pool, protocol_config, x_vault, base_vault) in
            heaven_pools.iter()
        {
            accounts.push(AccountMeta::new_readonly(*program_id, false));
            accounts.push(AccountMeta::new_readonly(*pool_base_mint, false)); // V9: Add base mint
            accounts.push(AccountMeta::new(*pool, false));
            accounts.push(AccountMeta::new(*protocol_config, false)); // Protocol config is writable for Heaven
            // Add fixed Heaven accounts
            accounts.push(AccountMeta::new_readonly(solana_program::sysvar::instructions::ID, false)); // Instructions sysvar
            accounts.push(AccountMeta::new_readonly(
                Pubkey::from_str("HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny").unwrap(),
                false,
            )); // Heaven protocol account 1
            accounts.push(AccountMeta::new_readonly(
                Pubkey::from_str("CH31Xns5z3M1cTAbKW34jcxPPciazARpijcHj9rxtemt").unwrap(),
                false,
            )); // Heaven protocol account 2
            accounts.push(AccountMeta::new(*x_vault, false));
            accounts.push(AccountMeta::new(*base_vault, false));
        }
    }

    // Create instruction data
    let mut data = vec![28u8];
    data.extend_from_slice(&minimum_profit.to_le_bytes());
    data.extend_from_slice(&compute_unit_limit.to_le_bytes());
    data.extend_from_slice(if no_failure_mode { &[1] } else { &[0] });
    data.extend_from_slice(&0u16.to_le_bytes()); // reserved
    data.extend_from_slice(if use_flashloan { &[1] } else { &[0] });

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}


pub fn derive_vault_token_account(program_id: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault_token_account", mint.as_ref()], program_id)
}
```

Additional fee account [#additional-fee-account]

The onchain program supports charging additional fees on top of the bot fee itself. This allows you to build customized bot and charge your users yourself.

To do so you need to change two things when constructing the instruction.

1. Add your additional fee account. Note this need to be the same token mint as base mint. aka if base mint is WSOL your additional fee account also need to be WSOL account.
2. Change the additional\_fee\_bp arg. The number is in base point. For example if you set 500, it means you will charge 5% of the profit yourself, onchain program will take 15%, and user will get 80% of the whole profit.

```rust
    let additional_fee_collector = Pubkey::from_str("XXX").unwrap();
    let mut accounts = vec![
        // Fixed accounts (0-6)
        AccountMeta::new_readonly(*wallet, true), // 0. Wallet (signer)
        AccountMeta::new_readonly(*base_mint, false), // 1. SOL mint
        AccountMeta::new(fee_collector, false),   // 2. Fee collector
        AccountMeta::new(additional_fee_collector, false), // 2.5 Additional fee account
        AccountMeta::new(*wallet_base_account, false), // 3. Wallet SOL account
        AccountMeta::new_readonly(*token_program, false), // 4. Token program
        AccountMeta::new_readonly(*system_program, false), // 5. System program
        AccountMeta::new_readonly(*associated_token_program, false), // 6. Associated Token program
    ];
...
...
    // Create instruction data
    let mut data = vec![28u8];
    data.extend_from_slice(&minimum_profit.to_le_bytes());
    data.extend_from_slice(&compute_unit_limit.to_le_bytes());
    data.extend_from_slice(if no_failure_mode { &[1] } else { &[0] });
    // Example of 5% additional fee
    let additional_fee_bp = 500u16;
    data.extend_from_slice(&additional_fee_bp.to_le_bytes());
    data.extend_from_slice(if use_flashloan { &[1] } else { &[0] });
```

Using fixed trade size [#using-fixed-trade-size]

This is advanced feature and only available when you call the program yourself. SMB bot does not set this value.

```rust
    // Create instruction data
    let mut data = vec![28u8];
    data.extend_from_slice(&minimum_profit.to_le_bytes());
    data.extend_from_slice(&compute_unit_limit.to_le_bytes());
    data.extend_from_slice(if no_failure_mode { &[1] } else { &[0] });
    data.extend_from_slice(&0u16.to_le_bytes()); // reserved
    data.extend_from_slice(if use_flashloan { &[1] } else { &[0] });

    let trade_size: u64 = fixed_trade_size; // 0 => disable override, >0 => fixed size
    data.extend_from_slice(&trade_size.to_le_bytes());
```
