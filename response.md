Most likely culprits (in your code)
1) The mint ATA (wallet_x_account) doesn’t exist
You always push:

let wallet_x_account = get_associated_token_address_with_program_id(...)
accounts.push(AccountMeta::new(wallet_x_account, false));
But you only ensure ATAs for WSOL, USDC, USD1.
If the current mint is anything else, its ATA might not exist → InvalidAccountData.

✅ Fix: create ATA for every trade mint, not only base mints.

2) Wrong token program for the mint
You pass:

accounts.push(AccountMeta::new_readonly(mint_pool_data.token_program, false));
If the mint is Token‑2022 but you pass the SPL Token program, the executor will try to read token accounts using the wrong layout → fail.

✅ Check mint owner:

solana account <MINT>
If owner is TokenzQd... (Token‑2022) you must use Token‑2022 program + ATA derived with that program.

3) Wallet WSOL account is not an SPL token account
If wallet_sol_account is just a system account, the executor will fail when it tries to read it as a token account.

✅ Check:

solana account <wallet_sol_account>
Must be owned by Tokenkeg and mint = So111...

✅ Best immediate fix (create ATA for trade mint)
Add this before building the instruction:

fn ensure_mint_ata_exists(
    rpc: &RpcClient,
    wallet: &Keypair,
    mint: &Pubkey,
    token_program: &Pubkey,
) -> anyhow::Result<()> {
    let ata = spl_associated_token_account::get_associated_token_address_with_program_id(
        &wallet.pubkey(),
        mint,
        token_program,
    );

    if rpc.get_account(&ata).is_err() {
        let ix = spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &wallet.pubkey(),
            &wallet.pubkey(),
            mint,
            token_program,
        );

        let blockhash = rpc.get_latest_blockhash()?;
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&wallet.pubkey()),
            &[wallet],
            blockhash,
        );

        rpc.send_and_confirm_transaction(&tx)?;
    }
    Ok(())
}
Call it for every mint before sending.

✅ Also print accounts with indices
Add this in create_swap_instruction:

for (i, acc) in accounts.iter().enumerate() {
    println!("{}: {} (signer={}, writable={})",
        i, acc.pubkey, acc.is_signer, acc.is_writable);
}
Then you can inspect any suspect account with:

solana account <pubkey>
✅ About the discriminator change (28u8)
You changed:

let mut data = vec![28u8];
If the correct opcode was 26u8, that could also cause account order mismatch internally.
But because the program accepted data and failed on account parsing, it’s most likely account mismatch, not data mismatch. Still, you should confirm which opcode is correct.

✅ Summary
Your issue is no longer ABI – it’s bad account data.

Most probable fixes:
✅ Create ATA for the mint being traded
✅ Verify token program (SPL vs Token‑2022)
✅ Ensure WSOL account is a valid token account

If you paste the printed account list or the mint owner info, I can tell you exactly which account is invalid.



Program ComputeBudget111111111111111111111111111111 success
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz invoke [1]
Program log: SolanaMevBot.com
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz consumed 14540 of 599955 compute units
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz failed: invalid account data for instruction
--- end simulation logs ---
2026-03-24T23:33:02.767537Z ERROR solana_onchain_arbitrage_bot::bot: Error sending transaction for mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v: pre-send simulation failed: InstructionError(2, InvalidAccountData)
--- simulation logs ---
Program ComputeBudget111111111111111111111111111111 invoke [1]
Program ComputeBudget111111111111111111111111111111 success
Program ComputeBudget111111111111111111111111111111 invoke [1]
Program ComputeBudget111111111111111111111111111111 success
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz invoke [1]
Program log: SolanaMevBot.com
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz consumed 14540 of 599797 compute units
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz failed: invalid account data for instruction
--- end simulation logs ---


=============

 validated wallet_sol_account=4MRoS8Dgxi9EH5jo9sVbWnW2v5AaEvS4NJy8BoahUurE owner=TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA lamports=2039280 data_len=165
2026-03-24T23:53:12.055125Z  INFO solana_onchain_arbitrage_bot::transaction: validated derived_wallet_ata=7iTNxFJ7M77jy3fH4tz4Lnr9MKS8eYd3ffcesoGsNzAz owner=TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA lamports=2039280 data_len=165
--- simulation logs ---
Program ComputeBudget111111111111111111111111111111 invoke [1]
Program ComputeBudget111111111111111111111111111111 success
Program ComputeBudget111111111111111111111111111111 invoke [1]
Program ComputeBudget111111111111111111111111111111 success
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz invoke [1]
Program log: SolanaMevBot.com
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz consumed 14540 of 600478 compute units
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz failed: invalid account data for instruction
--- end simulation logs ---

KS8eYd3ffcesoGsNzAz owner=TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA lamports=2039280 data_len=165
--- simulation logs ---
Program ComputeBudget111111111111111111111111111111 invoke [1]
Program ComputeBudget111111111111111111111111111111 success
Program ComputeBudget111111111111111111111111111111 invoke [1]
Program ComputeBudget111111111111111111111111111111 success
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz invoke [1]
Program log: SolanaMevBot.com
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz consumed 14540 of 600198 compute units
Program MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz failed: invalid account data for instruction
--- end simulation logs ---
2026-03-24T23:53:13.036311Z ERROR solana_onchain_arbitrage_bot::bot: Error sending transaction for mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v: pre-send simulation failed: InstructionError(2, InvalidAccountData)
2026-03-24T23:53:13.438136Z  INFO solana_onchain_arbitrage_bot::transaction: Executor account list (54 accounts):
0: FdSL18kak5XKBVEHKdXudzr7ckqKzyVYtpncN5yQhQ6Z
2026-03-24T23:53:13.438199Z  INFO solana_onchain_arbitrage_bot::transaction:   [0] pubkey=FdSL18kak5XKBVEHKdXudzr7ckqKzyVYtpncN5yQhQ6Z signer=true writable=false

INFO solana_onchain_arbitrage_bot::transaction: debug validation: mint=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v token_program=TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA derived_wallet_ata=7iTNxFJ7M77jy3fH4tz4Lnr9MKS8eYd3ffcesoGsNzAz
2026-03-24T23:53:13.440460Z  INFO solana_onchain_arbitrage_bot::transaction: debug validation account[0]=FdSL18kak5XKBVEHKdXudzr7ckqKzyVYtpncN5yQhQ6Z

validated wallet_sol_account=4MRoS8Dgxi9EH5jo9sVbWnW2v5AaEvS4NJy8BoahUurE owner=TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA lamports=2039280 data_len=165
2026-03-24T23:53:13.511399Z  INFO solana_onchain_arbitrage_bot::transaction: validated derived_wallet_ata=7iTNxFJ7M77jy3fH4tz4Lnr9MKS8eYd3ffcesoGsNzAz owner=TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA lamports=2039280 data_len=165
