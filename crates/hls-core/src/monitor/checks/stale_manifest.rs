use std::time::Duration;

use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct StaleManifestCheck {
    pub stale_limit: Duration,
}

impl StaleManifestCheck {
    pub fn new(stale_limit: Duration) -> Self {
        Self { stale_limit }
    }
}

impl Check for StaleManifestCheck {
    fn name(&self) -> &'static str {
        "StaleManifest"
    }

    fn check(
        &self,
        _prev: &VariantState,
        _curr: &PlaylistSnapshot,
        _ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        vec![]
    }
}

pub fn check_stale(
    time_since_change_ms: u128,
    stale_limit: Duration,
    stream_url: &str,
    stream_id: &str,
) -> Option<MonitorError> {
    let limit_ms = stale_limit.as_millis();
    if time_since_change_ms > limit_ms {
        Some(MonitorError::new(
            ErrorType::StaleManifest,
            "ALL",
            "ALL",
            format!("Expected: {}ms. Got: {}ms", limit_ms, time_since_change_ms),
            stream_url,
            stream_id,
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_stale_manifest() {
        let err = check_stale(
            7000,
            Duration::from_millis(6000),
            "http://example.com/",
            "s1",
        );
        assert!(err.is_some());
        let e = err.unwrap();
        assert_eq!(e.error_type, ErrorType::StaleManifest);
        assert!(e.details.contains("Expected: 6000ms. Got: 7000ms"));
    }

    #[test]
    fn no_error_within_limit() {
        let err = check_stale(
            5000,
            Duration::from_millis(6000),
            "http://example.com/",
            "s1",
        );
        assert!(err.is_none());
    }

    #[test]
    fn no_error_at_exact_limit() {
        let err = check_stale(
            6000,
            Duration::from_millis(6000),
            "http://example.com/",
            "s1",
        );
        assert!(err.is_none());
    }
}
