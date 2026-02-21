use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::error::ErrorRing;
use super::event::EventRing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorState {
    Idle,
    Active,
    Stopping,
    Stopped,
}

impl MonitorState {
    pub fn can_transition_to(self, target: MonitorState) -> bool {
        matches!(
            (self, target),
            (MonitorState::Idle, MonitorState::Active)
                | (MonitorState::Active, MonitorState::Stopping)
                | (MonitorState::Stopping, MonitorState::Stopped)
                | (MonitorState::Stopped, MonitorState::Active)
        )
    }
}

impl std::fmt::Display for MonitorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Active => write!(f, "active"),
            Self::Stopping => write!(f, "stopping"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VariantState {
    pub media_type: String,
    pub media_sequence: u64,
    pub segment_uris: Vec<String>,
    pub discontinuity_sequence: u64,
    pub next_is_discontinuity: bool,
    pub prev_segments: Vec<SegmentInfo>,
    pub duration: f64,
    pub cue_out_count: usize,
    pub cue_in_count: usize,
    pub in_cue_out: bool,
    pub cue_out_duration: Option<f64>,
    pub version: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct SegmentInfo {
    pub uri: String,
    pub discontinuity: bool,
}

#[derive(Debug, Clone)]
pub struct PlaylistSnapshot {
    pub media_sequence: u64,
    pub discontinuity_sequence: u64,
    pub segments: Vec<SegmentSnapshot>,
    pub duration: f64,
    pub cue_out_count: usize,
    pub cue_in_count: usize,
    pub has_cue_out: bool,
    pub cue_out_duration: Option<f64>,
    pub target_duration: f64,
    pub playlist_type: Option<String>,
    pub version: Option<u16>,
    pub has_gaps: bool,
}

#[derive(Debug, Clone)]
pub struct SegmentSnapshot {
    pub uri: String,
    pub duration: f64,
    pub discontinuity: bool,
    pub cue_out: bool,
    pub cue_in: bool,
    pub cue_out_cont: Option<String>,
    pub gap: bool,
    pub program_date_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub daterange: Option<DateRangeSnapshot>,
}

#[derive(Debug, Clone)]
pub struct DateRangeSnapshot {
    pub id: String,
    pub class: Option<String>,
    pub start_date: chrono::DateTime<chrono::FixedOffset>,
    pub end_date: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub duration: Option<f64>,
    pub end_on_next: bool,
}

#[derive(Debug, Clone)]
pub struct CheckContext {
    pub stream_url: String,
    pub stream_id: String,
    pub media_type: String,
    pub variant_key: String,
}

#[derive(Debug)]
pub struct StreamData {
    pub variants: HashMap<String, VariantState>,
    pub last_content_change: DateTime<Utc>,
    pub last_fetch: DateTime<Utc>,
    pub errors: ErrorRing,
    pub events: EventRing,
    pub was_stale: bool,
}

impl StreamData {
    pub fn new(error_capacity: usize, event_capacity: usize) -> Self {
        let now = Utc::now();
        Self {
            variants: HashMap::new(),
            last_content_change: now,
            last_fetch: now,
            errors: ErrorRing::new(error_capacity),
            events: EventRing::new(event_capacity),
            was_stale: false,
        }
    }
}

/// Per-stream live status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamStatus {
    pub stream_id: String,
    pub stream_url: String,
    pub last_fetch: DateTime<Utc>,
    pub last_content_change: DateTime<Utc>,
    pub error_count: usize,
    pub variants: Vec<VariantStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantStatus {
    pub variant_key: String,
    pub media_type: String,
    pub media_sequence: u64,
    pub discontinuity_sequence: u64,
    pub segment_count: usize,
    pub playlist_duration_secs: f64,
    pub in_cue_out: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue_out_duration: Option<f64>,
    pub cue_out_count: usize,
    pub cue_in_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamItem {
    pub id: String,
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_state_transitions() {
        assert!(MonitorState::Idle.can_transition_to(MonitorState::Active));
        assert!(MonitorState::Active.can_transition_to(MonitorState::Stopping));
        assert!(MonitorState::Stopping.can_transition_to(MonitorState::Stopped));
        assert!(MonitorState::Stopped.can_transition_to(MonitorState::Active));
    }

    #[test]
    fn invalid_state_transitions() {
        assert!(!MonitorState::Idle.can_transition_to(MonitorState::Stopping));
        assert!(!MonitorState::Idle.can_transition_to(MonitorState::Stopped));
        assert!(!MonitorState::Active.can_transition_to(MonitorState::Idle));
        assert!(!MonitorState::Active.can_transition_to(MonitorState::Active));
        assert!(!MonitorState::Stopped.can_transition_to(MonitorState::Stopping));
        assert!(!MonitorState::Stopping.can_transition_to(MonitorState::Active));
    }
}
