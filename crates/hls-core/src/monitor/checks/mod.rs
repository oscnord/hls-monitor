pub mod media_sequence;
pub mod playlist_size;
pub mod playlist_content;
pub mod segment_continuity;
pub mod discontinuity;
pub mod stale_manifest;
pub mod scte35;
pub mod target_duration;
pub mod gap;
pub mod mseq_gap;
pub mod playlist_type;
pub mod segment_duration_anomaly;
pub mod version;

use super::error::MonitorError;
use super::state::{CheckContext, PlaylistSnapshot, VariantState};

/// Trait for a composable HLS validation check.
///
/// Each check receives the previous variant state and the freshly-fetched
/// playlist snapshot, and returns zero or more errors found.
pub trait Check: Send + Sync {
    /// Human-readable name of this check.
    fn name(&self) -> &'static str;

    /// Run the check and return any errors detected.
    fn check(
        &self,
        prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError>;
}

/// Build the default set of checks based on configuration.
pub fn default_checks(config: &crate::config::MonitorConfig) -> Vec<Box<dyn Check>> {
    let mut checks: Vec<Box<dyn Check>> = vec![
        Box::new(media_sequence::MediaSequenceCheck),
        Box::new(playlist_size::PlaylistSizeCheck),
        Box::new(playlist_content::PlaylistContentCheck),
        Box::new(segment_continuity::SegmentContinuityCheck),
        Box::new(discontinuity::DiscontinuityCheck),
    ];

    if config.scte35_enabled {
        checks.push(Box::new(scte35::Scte35Check));
    }

    checks.push(Box::new(target_duration::TargetDurationCheck::new(config.target_duration_tolerance)));
    checks.push(Box::new(gap::GapCheck));
    checks.push(Box::new(mseq_gap::MseqGapCheck::new(config.mseq_gap_threshold)));
    checks.push(Box::new(playlist_type::PlaylistTypeCheck));
    checks.push(Box::new(segment_duration_anomaly::SegmentDurationAnomalyCheck::new(config.segment_duration_anomaly_ratio)));
    checks.push(Box::new(version::VersionCheck));

    checks
}
