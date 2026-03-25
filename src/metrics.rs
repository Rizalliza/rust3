use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tracing::info;

pub struct Metrics {
    tx_attempted: AtomicU64,
    tx_sent_success: AtomicU64,
    tx_send_failed: AtomicU64,
    tx_sim_ok: AtomicU64,
    tx_sim_failed: AtomicU64,
    rpc_429: AtomicU64,
    rpc_send_fail_non_429: AtomicU64,
    cooldown_events: AtomicU64,
}

impl Metrics {
    const fn new() -> Self {
        Self {
            tx_attempted: AtomicU64::new(0),
            tx_sent_success: AtomicU64::new(0),
            tx_send_failed: AtomicU64::new(0),
            tx_sim_ok: AtomicU64::new(0),
            tx_sim_failed: AtomicU64::new(0),
            rpc_429: AtomicU64::new(0),
            rpc_send_fail_non_429: AtomicU64::new(0),
            cooldown_events: AtomicU64::new(0),
        }
    }

    fn get(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }
}

fn metrics_instance() -> &'static Metrics {
    static METRICS: OnceLock<Metrics> = OnceLock::new();
    METRICS.get_or_init(Metrics::new)
}

pub fn inc_tx_attempted() {
    metrics_instance()
        .tx_attempted
        .fetch_add(1, Ordering::Relaxed);
}

pub fn inc_tx_sent_success() {
    metrics_instance()
        .tx_sent_success
        .fetch_add(1, Ordering::Relaxed);
}

pub fn inc_tx_send_failed() {
    metrics_instance()
        .tx_send_failed
        .fetch_add(1, Ordering::Relaxed);
}

pub fn inc_tx_sim_ok() {
    metrics_instance().tx_sim_ok.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_tx_sim_failed() {
    metrics_instance()
        .tx_sim_failed
        .fetch_add(1, Ordering::Relaxed);
}

pub fn inc_rpc_429() {
    metrics_instance().rpc_429.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_rpc_send_fail_non_429() {
    metrics_instance()
        .rpc_send_fail_non_429
        .fetch_add(1, Ordering::Relaxed);
}

pub fn inc_cooldown_events() {
    metrics_instance()
        .cooldown_events
        .fetch_add(1, Ordering::Relaxed);
}

pub async fn metrics_reporter(report_interval: Duration) {
    loop {
        info!(
            "metrics tx_attempted={} tx_sent_success={} tx_send_failed={} tx_sim_ok={} tx_sim_failed={} rpc_429={} rpc_send_fail_non_429={} cooldown_events={}",
            Metrics::get(&metrics_instance().tx_attempted),
            Metrics::get(&metrics_instance().tx_sent_success),
            Metrics::get(&metrics_instance().tx_send_failed),
            Metrics::get(&metrics_instance().tx_sim_ok),
            Metrics::get(&metrics_instance().tx_sim_failed),
            Metrics::get(&metrics_instance().rpc_429),
            Metrics::get(&metrics_instance().rpc_send_fail_non_429),
            Metrics::get(&metrics_instance().cooldown_events),
        );
        tokio::time::sleep(report_interval).await;
    }
}
