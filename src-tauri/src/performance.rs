use serde::Serialize;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};
use std::time::Instant;

static GLOBAL_MONITOR: OnceLock<PerformanceMonitor> = OnceLock::new();

pub fn global() -> &'static PerformanceMonitor {
    GLOBAL_MONITOR.get_or_init(PerformanceMonitor::new)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSnapshot {
    uptime_ms: u64,
    active_operations: u64,
    peak_operations: u64,
    completed_operations: u64,
    total_operation_time_ms: u64,
    downloaded_bytes: u64,
    reused_bytes: u64,
    cache_hits: u64,
    cache_misses: u64,
    verification_failures: u64,
    cache_hit_rate_basis_points: u32,
}

pub struct PerformanceMonitor {
    started_at: Instant,
    active_operations: AtomicU64,
    peak_operations: AtomicU64,
    completed_operations: AtomicU64,
    total_operation_nanos: AtomicU64,
    downloaded_bytes: AtomicU64,
    reused_bytes: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    verification_failures: AtomicU64,
}

impl PerformanceMonitor {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            active_operations: AtomicU64::new(0),
            peak_operations: AtomicU64::new(0),
            completed_operations: AtomicU64::new(0),
            total_operation_nanos: AtomicU64::new(0),
            downloaded_bytes: AtomicU64::new(0),
            reused_bytes: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            verification_failures: AtomicU64::new(0),
        }
    }

    pub fn track(&'static self) -> OperationGuard {
        let active = self.active_operations.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak_operations.fetch_max(active, Ordering::Relaxed);
        OperationGuard {
            monitor: self,
            started_at: Instant::now(),
        }
    }

    pub fn record_downloaded_bytes(&self, bytes: u64) {
        self.downloaded_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_reused_bytes(&self, bytes: u64) {
        self.reused_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_verification_failure(&self) {
        self.verification_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> PerformanceSnapshot {
        let cache_hits = self.cache_hits.load(Ordering::Relaxed);
        let cache_misses = self.cache_misses.load(Ordering::Relaxed);
        let cache_requests = cache_hits.saturating_add(cache_misses);
        let cache_hit_rate_basis_points = if cache_requests == 0 {
            0
        } else {
            ((cache_hits.saturating_mul(10_000)) / cache_requests).min(10_000) as u32
        };

        PerformanceSnapshot {
            uptime_ms: self.started_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
            active_operations: self.active_operations.load(Ordering::Relaxed),
            peak_operations: self.peak_operations.load(Ordering::Relaxed),
            completed_operations: self.completed_operations.load(Ordering::Relaxed),
            total_operation_time_ms: self.total_operation_nanos.load(Ordering::Relaxed) / 1_000_000,
            downloaded_bytes: self.downloaded_bytes.load(Ordering::Relaxed),
            reused_bytes: self.reused_bytes.load(Ordering::Relaxed),
            cache_hits,
            cache_misses,
            verification_failures: self.verification_failures.load(Ordering::Relaxed),
            cache_hit_rate_basis_points,
        }
    }
}

pub struct OperationGuard {
    monitor: &'static PerformanceMonitor,
    started_at: Instant,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let elapsed_nanos = self.started_at.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        self.monitor
            .total_operation_nanos
            .fetch_add(elapsed_nanos, Ordering::Relaxed);
        self.monitor
            .completed_operations
            .fetch_add(1, Ordering::Relaxed);
        self.monitor
            .active_operations
            .fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::PerformanceMonitor;

    #[test]
    fn tracks_operation_lifecycle_and_peak_concurrency() {
        let monitor = Box::leak(Box::new(PerformanceMonitor::new()));
        {
            let first = monitor.track();
            assert_eq!(monitor.snapshot().active_operations, 1);
            let second = monitor.track();
            assert_eq!(monitor.snapshot().active_operations, 2);
            assert_eq!(monitor.snapshot().peak_operations, 2);
            drop(second);
            assert_eq!(monitor.snapshot().active_operations, 1);
            drop(first);
        }
        let snapshot = monitor.snapshot();
        assert_eq!(snapshot.active_operations, 0);
        assert_eq!(snapshot.completed_operations, 2);
    }

    #[test]
    fn records_cache_download_reuse_and_verification_counters() {
        let monitor = Box::leak(Box::new(PerformanceMonitor::new()));
        monitor.record_cache_hit();
        monitor.record_cache_hit();
        monitor.record_cache_miss();
        monitor.record_downloaded_bytes(4096);
        monitor.record_reused_bytes(2048);
        monitor.record_verification_failure();

        let snapshot = monitor.snapshot();
        assert_eq!(snapshot.cache_hits, 2);
        assert_eq!(snapshot.cache_misses, 1);
        assert_eq!(snapshot.cache_hit_rate_basis_points, 6666);
        assert_eq!(snapshot.downloaded_bytes, 4096);
        assert_eq!(snapshot.reused_bytes, 2048);
        assert_eq!(snapshot.verification_failures, 1);
    }
}
