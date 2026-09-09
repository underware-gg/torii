//! Process memory reporting.
//!
//! Publishes allocator statistics as gauges and logs a summary line periodically, so memory
//! growth can be attributed without attaching a profiler:
//!
//! - `torii_memory_jemalloc_allocated_bytes`: bytes the application currently holds. Grows only
//!   if something is retained. This is the leak signal.
//! - `torii_memory_jemalloc_active_bytes`: pages backing those allocations.
//! - `torii_memory_jemalloc_resident_bytes`: pages jemalloc keeps mapped in RAM, including
//!   freed-but-not-yet-returned ones. This, not `allocated`, is what the OS reports as RSS.
//! - `torii_memory_jemalloc_mapped_bytes`, `torii_memory_jemalloc_retained_bytes`,
//!   `torii_memory_jemalloc_metadata_bytes`: address space mapped, address space kept after
//!   being unmapped from use, and allocator bookkeeping.
//! - `torii_memory_resident_bytes`: RSS read from the kernel (Linux only).
//!
//! A gap between `resident` and `allocated` that stays flat is allocator retention. A climbing
//! `allocated` is a leak.
//!
//! The gauges exported by `dojo-metrics` under `jemalloc_*` accumulate across scrapes and cannot
//! be used for this; these are the ones to graph.

use std::time::Duration;

use metrics::{describe_gauge, gauge};
use tokio::time::MissedTickBehavior;
use tracing::{debug, info};

pub(crate) const LOG_TARGET: &str = "torii::runner::memory";

/// How often gauges are refreshed.
pub const REPORT_INTERVAL: Duration = Duration::from_secs(15);

/// Ticks between summary lines at `info`, so one every five minutes at the default interval.
const LOG_EVERY_TICKS: u32 = 20;

/// Allocator statistics for one sample, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JemallocStats {
    pub allocated: u64,
    pub active: u64,
    pub resident: u64,
    pub mapped: u64,
    pub retained: u64,
    pub metadata: u64,
}

#[cfg(all(feature = "jemalloc", unix))]
pub fn jemalloc_stats() -> Option<JemallocStats> {
    use jemalloc_ctl::{epoch, stats};

    // Statistics are cached until the epoch advances.
    epoch::advance().ok()?;
    Some(JemallocStats {
        allocated: stats::allocated::read().ok()? as u64,
        active: stats::active::read().ok()? as u64,
        resident: stats::resident::read().ok()? as u64,
        mapped: stats::mapped::read().ok()? as u64,
        retained: stats::retained::read().ok()? as u64,
        metadata: stats::metadata::read().ok()? as u64,
    })
}

#[cfg(not(all(feature = "jemalloc", unix)))]
pub fn jemalloc_stats() -> Option<JemallocStats> {
    None
}

/// Resident set size as reported by the kernel, when the platform exposes it cheaply.
#[cfg(target_os = "linux")]
pub fn resident_set_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: sysconf has no preconditions and only reads process configuration.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let page_size = if page_size > 0 {
        page_size as u64
    } else {
        4096
    };
    Some(resident_pages * page_size)
}

#[cfg(not(target_os = "linux"))]
pub fn resident_set_bytes() -> Option<u64> {
    None
}

fn describe() {
    describe_gauge!(
        "torii_memory_jemalloc_allocated_bytes",
        "Bytes currently allocated by the application (jemalloc stats.allocated)."
    );
    describe_gauge!(
        "torii_memory_jemalloc_active_bytes",
        "Bytes in pages backing application allocations (jemalloc stats.active)."
    );
    describe_gauge!(
        "torii_memory_jemalloc_resident_bytes",
        "Bytes in pages jemalloc keeps mapped in physical memory (jemalloc stats.resident)."
    );
    describe_gauge!(
        "torii_memory_jemalloc_mapped_bytes",
        "Bytes of address space mapped by jemalloc (jemalloc stats.mapped)."
    );
    describe_gauge!(
        "torii_memory_jemalloc_retained_bytes",
        "Bytes of address space retained by jemalloc after being unmapped from use (jemalloc stats.retained)."
    );
    describe_gauge!(
        "torii_memory_jemalloc_metadata_bytes",
        "Bytes of jemalloc bookkeeping (jemalloc stats.metadata)."
    );
    describe_gauge!(
        "torii_memory_resident_bytes",
        "Resident set size reported by the kernel."
    );
}

fn publish(stats: Option<JemallocStats>, rss: Option<u64>) {
    if let Some(stats) = stats {
        gauge!("torii_memory_jemalloc_allocated_bytes").set(stats.allocated as f64);
        gauge!("torii_memory_jemalloc_active_bytes").set(stats.active as f64);
        gauge!("torii_memory_jemalloc_resident_bytes").set(stats.resident as f64);
        gauge!("torii_memory_jemalloc_mapped_bytes").set(stats.mapped as f64);
        gauge!("torii_memory_jemalloc_retained_bytes").set(stats.retained as f64);
        gauge!("torii_memory_jemalloc_metadata_bytes").set(stats.metadata as f64);
    }
    if let Some(rss) = rss {
        gauge!("torii_memory_resident_bytes").set(rss as f64);
    }
}

fn mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

/// Refresh memory gauges every [`REPORT_INTERVAL`] and log a summary every few minutes.
pub async fn run() {
    describe();

    if jemalloc_stats().is_none() {
        info!(target: LOG_TARGET, "Allocator statistics unavailable on this build; only kernel RSS will be reported where supported.");
    }

    let mut ticker = tokio::time::interval(REPORT_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut tick: u32 = 0;

    loop {
        ticker.tick().await;
        let stats = jemalloc_stats();
        let rss = resident_set_bytes();
        publish(stats, rss);

        let summary_due = tick.is_multiple_of(LOG_EVERY_TICKS);
        tick = tick.wrapping_add(1);

        match stats {
            Some(stats) if summary_due => info!(
                target: LOG_TARGET,
                allocated_mib = mib(stats.allocated),
                active_mib = mib(stats.active),
                resident_mib = mib(stats.resident),
                mapped_mib = mib(stats.mapped),
                retained_mib = mib(stats.retained),
                metadata_mib = mib(stats.metadata),
                rss_mib = rss.map(mib),
                "Memory usage."
            ),
            Some(stats) => debug!(
                target: LOG_TARGET,
                allocated_mib = mib(stats.allocated),
                resident_mib = mib(stats.resident),
                rss_mib = rss.map(mib),
                "Memory usage."
            ),
            None if summary_due => {
                if let Some(rss) = rss {
                    info!(target: LOG_TARGET, rss_mib = mib(rss), "Memory usage.");
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mib_rounds_down() {
        assert_eq!(mib(0), 0);
        assert_eq!(mib(1024 * 1024 - 1), 0);
        assert_eq!(mib(3 * 1024 * 1024 + 5), 3);
    }

    #[cfg(all(feature = "jemalloc", unix))]
    #[test]
    fn jemalloc_stats_are_readable_and_consistent() {
        let stats = jemalloc_stats().expect("jemalloc stats should be readable");
        assert!(stats.allocated > 0);
        assert!(stats.active >= stats.allocated);
        assert!(stats.resident >= stats.active);
        assert!(stats.mapped >= stats.active);
    }
}
