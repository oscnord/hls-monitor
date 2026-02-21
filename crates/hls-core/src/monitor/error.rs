use std::collections::VecDeque;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    ManifestRetrieval,
    MediaSequence,
    PlaylistSize,
    PlaylistContent,
    SegmentContinuity,
    DiscontinuitySequence,
    StaleManifest,
    Scte35Violation,
}

impl fmt::Display for ErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManifestRetrieval => write!(f, "Manifest Retrieval"),
            Self::MediaSequence => write!(f, "Media Sequence"),
            Self::PlaylistSize => write!(f, "Playlist Size"),
            Self::PlaylistContent => write!(f, "Playlist Content"),
            Self::SegmentContinuity => write!(f, "Segment Continuity"),
            Self::DiscontinuitySequence => write!(f, "Discontinuity Sequence"),
            Self::StaleManifest => write!(f, "Stale Manifest"),
            Self::Scte35Violation => write!(f, "SCTE-35 Violation"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorError {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub error_type: ErrorType,
    pub media_type: String,
    pub variant: String,
    pub details: String,
    pub stream_url: String,
    pub stream_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
}

impl MonitorError {
    pub fn new(
        error_type: ErrorType,
        media_type: impl Into<String>,
        variant: impl Into<String>,
        details: impl Into<String>,
        stream_url: impl Into<String>,
        stream_id: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            error_type,
            media_type: media_type.into(),
            variant: variant.into(),
            details: details.into(),
            stream_url: stream_url.into(),
            stream_id: stream_id.into(),
            status_code: None,
        }
    }

    pub fn with_status_code(mut self, code: u16) -> Self {
        self.status_code = Some(code);
        self
    }
}

/// Fixed-capacity circular buffer for recent errors. O(1) insert, evicts oldest when full.
#[derive(Debug, Clone)]
pub struct ErrorRing {
    buffer: VecDeque<MonitorError>,
    capacity: usize,
}

impl ErrorRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, error: MonitorError) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(error);
    }

    pub fn list(&self) -> Vec<MonitorError> {
        self.buffer.iter().rev().cloned().collect()
    }

    pub fn list_chronological(&self) -> Vec<MonitorError> {
        self.buffer.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_error(detail: &str) -> MonitorError {
        MonitorError::new(
            ErrorType::MediaSequence,
            "VIDEO",
            "1200000",
            detail,
            "http://example.com/master.m3u8",
            "stream_1",
        )
    }

    #[test]
    fn ring_push_within_capacity() {
        let mut ring = ErrorRing::new(5);
        ring.push(make_error("e1"));
        ring.push(make_error("e2"));
        ring.push(make_error("e3"));
        assert_eq!(ring.len(), 3);
    }

    #[test]
    fn ring_evicts_oldest_at_capacity() {
        let mut ring = ErrorRing::new(3);
        ring.push(make_error("e1"));
        ring.push(make_error("e2"));
        ring.push(make_error("e3"));
        ring.push(make_error("e4"));
        assert_eq!(ring.len(), 3);
        let errors = ring.list_chronological();
        assert_eq!(errors[0].details, "e2");
        assert_eq!(errors[1].details, "e3");
        assert_eq!(errors[2].details, "e4");
    }

    #[test]
    fn ring_list_returns_newest_first() {
        let mut ring = ErrorRing::new(5);
        ring.push(make_error("e1"));
        ring.push(make_error("e2"));
        ring.push(make_error("e3"));
        let errors = ring.list();
        assert_eq!(errors[0].details, "e3");
        assert_eq!(errors[1].details, "e2");
        assert_eq!(errors[2].details, "e1");
    }

    #[test]
    fn ring_clear_empties_buffer() {
        let mut ring = ErrorRing::new(5);
        ring.push(make_error("e1"));
        ring.push(make_error("e2"));
        ring.clear();
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn ring_with_status_code() {
        let err = make_error("fetch failed").with_status_code(503);
        assert_eq!(err.status_code, Some(503));
    }

    #[test]
    fn ring_single_capacity() {
        let mut ring = ErrorRing::new(1);
        ring.push(make_error("e1"));
        ring.push(make_error("e2"));
        assert_eq!(ring.len(), 1);
        assert_eq!(ring.list()[0].details, "e2");
    }
}
