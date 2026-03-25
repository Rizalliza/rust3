use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub struct PaperTradeRecord {
    pub timestamp_unix_ms: u128,
    pub mint: String,
    pub simulation_passed: bool,
    pub priority_fee_lamports: u64,
    pub slippage_cost_lamports: u64,
    pub estimated_net_lamports: i64,
}

pub fn append_record_csv(path: &str, record: &PaperTradeRecord) -> anyhow::Result<()> {
    let file_exists = Path::new(path).exists();
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    if !file_exists {
        writeln!(
            file,
            "timestamp_unix_ms,mint,simulation_passed,priority_fee_lamports,slippage_cost_lamports,estimated_net_lamports"
        )?;
    }

    writeln!(
        file,
        "{},{},{},{},{},{}",
        record.timestamp_unix_ms,
        record.mint,
        record.simulation_passed,
        record.priority_fee_lamports,
        record.slippage_cost_lamports,
        record.estimated_net_lamports
    )?;

    Ok(())
}
