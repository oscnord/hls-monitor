use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Configuration for an HLS monitor instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Time in ms before a manifest is considered stale (default: 6000).
    pub stale_limit: Duration,
    /// Polling interval between manifest fetches (default: stale_limit / 2).
    pub poll_interval: Duration,
    /// Maximum number of errors to retain per stream (ring buffer capacity).
    pub error_limit: usize,
    /// HTTP request timeout for manifest fetches.
    pub request_timeout: Duration,
    /// Maximum number of retries for failed manifest fetches.
    pub max_retries: u32,
    /// Base backoff duration for retries (doubled each attempt).
    pub retry_backoff: Duration,
    /// Whether to enable SCTE-35/CUE marker validation.
    pub scte35_enabled: bool,
    /// Maximum number of events to retain per stream (ring buffer capacity).
    pub event_limit: usize,
    pub target_duration_tolerance: f64,
    pub mseq_gap_threshold: u64,
    pub variant_sync_drift_threshold: u64,
    pub variant_failure_threshold: u32,
    pub segment_duration_anomaly_ratio: f64,
    pub max_concurrent_fetches: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        let stale_limit = Duration::from_millis(6000);
        Self {
            stale_limit,
            poll_interval: stale_limit / 2,
            error_limit: 100,
            request_timeout: Duration::from_secs(10),
            max_retries: 3,
            retry_backoff: Duration::from_millis(100),
            scte35_enabled: false,
            event_limit: 200,
            target_duration_tolerance: 0.5,
            mseq_gap_threshold: 5,
            variant_sync_drift_threshold: 3,
            variant_failure_threshold: 3,
            segment_duration_anomaly_ratio: 0.5,
            max_concurrent_fetches: 4,
        }
    }
}

impl MonitorConfig {
    pub fn with_stale_limit(mut self, ms: u64) -> Self {
        let old_default_poll = self.stale_limit / 2;
        self.stale_limit = Duration::from_millis(ms);
        if self.poll_interval == old_default_poll {
            self.poll_interval = self.stale_limit / 2;
        }
        if self.poll_interval > self.stale_limit {
            self.poll_interval = self.stale_limit / 2;
        }
        self
    }

    pub fn with_poll_interval(mut self, ms: u64) -> Self {
        self.poll_interval = Duration::from_millis(ms);
        self
    }

    pub fn with_error_limit(mut self, limit: usize) -> Self {
        self.error_limit = limit;
        self
    }

    pub fn with_scte35(mut self, enabled: bool) -> Self {
        self.scte35_enabled = enabled;
        self
    }

    pub fn with_event_limit(mut self, limit: usize) -> Self {
        self.event_limit = limit;
        self
    }

    pub fn with_target_duration_tolerance(mut self, tolerance: f64) -> Self {
        self.target_duration_tolerance = tolerance;
        self
    }

    pub fn with_mseq_gap_threshold(mut self, threshold: u64) -> Self {
        self.mseq_gap_threshold = threshold;
        self
    }

    pub fn with_variant_sync_drift_threshold(mut self, threshold: u64) -> Self {
        self.variant_sync_drift_threshold = threshold;
        self
    }

    pub fn with_variant_failure_threshold(mut self, threshold: u32) -> Self {
        self.variant_failure_threshold = threshold;
        self
    }

    pub fn with_segment_duration_anomaly_ratio(mut self, ratio: f64) -> Self {
        self.segment_duration_anomaly_ratio = ratio;
        self
    }

    pub fn with_max_concurrent_fetches(mut self, max: usize) -> Self {
        self.max_concurrent_fetches = max.max(1);
        self
    }
}
