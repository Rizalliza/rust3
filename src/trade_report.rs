use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;

#[derive(Debug, Clone, Default)]
pub struct BalanceSnapshot {
    pub lamports: u64,
    pub token_amount: u64,
}

impl BalanceSnapshot {
    pub fn new(lamports: u64, token_amount: u64) -> Self {
        Self {
            lamports,
            token_amount,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TradeExecutionReport {
    pub signature: Option<Signature>,
    pub confirmed: bool,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub mint: Pubkey,
    pub wallet: Pubkey,
    pub pre_trade: Option<BalanceSnapshot>,
    pub post_trade: Option<BalanceSnapshot>,
    pub estimated_profit_lamports: Option<i128>,
    pub realized_profit_lamports: Option<i128>,
}

impl TradeExecutionReport {
    pub fn skipped(mint: Pubkey, wallet: Pubkey, reason: impl Into<String>) -> Self {
        Self {
            signature: None,
            confirmed: false,
            skipped: true,
            skip_reason: Some(reason.into()),
            mint,
            wallet,
            pre_trade: None,
            post_trade: None,
            estimated_profit_lamports: None,
            realized_profit_lamports: None,
        }
    }

    pub fn submitted(
        mint: Pubkey,
        wallet: Pubkey,
        signature: Signature,
        estimated_profit_lamports: Option<i128>,
        pre_trade: Option<BalanceSnapshot>,
    ) -> Self {
        Self {
            signature: Some(signature),
            confirmed: false,
            skipped: false,
            skip_reason: None,
            mint,
            wallet,
            pre_trade,
            post_trade: None,
            estimated_profit_lamports,
            realized_profit_lamports: None,
        }
    }

    pub fn mark_confirmed(&mut self, post_trade: Option<BalanceSnapshot>) {
        self.confirmed = true;
        self.post_trade = post_trade;
        self.realized_profit_lamports = self
            .pre_trade
            .as_ref()
            .zip(self.post_trade.as_ref())
            .map(|(pre, post)| post.lamports as i128 - pre.lamports as i128);
    }

    pub fn mark_failed(&mut self, reason: impl Into<String>) {
        self.confirmed = false;
        self.skip_reason = Some(reason.into());
    }

    pub fn profit_delta_lamports(pre_lamports: u64, post_lamports: u64) -> i128 {
        post_lamports as i128 - pre_lamports as i128
    }

    pub fn estimated_is_profitable(&self, min_profit_lamports: i128) -> bool {
        self.estimated_profit_lamports
            .map(|profit| profit >= min_profit_lamports)
            .unwrap_or(false)
    }
}