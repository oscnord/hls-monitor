use std::collections::VecDeque;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    CueOutStarted,
    CueInReturned,
    CueOutCont,
    DiscontinuityChanged,
    ManifestUpdated,
    StaleRecovered,
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CueOutStarted => write!(f, "CUE-OUT"),
            Self::CueInReturned => write!(f, "CUE-IN"),
            Self::CueOutCont => write!(f, "CUE-OUT-CONT"),
            Self::DiscontinuityChanged => write!(f, "DISC"),
            Self::ManifestUpdated => write!(f, "UPDATE"),
            Self::StaleRecovered => write!(f, "RECOVERED"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub kind: EventKind,
    pub stream_id: String,
    pub media_type: String,
    pub variant_key: String,
    pub details: String,
}

impl MonitorEvent {
    pub fn new(
        kind: EventKind,
        media_type: impl Into<String>,
        variant_key: impl Into<String>,
        details: impl Into<String>,
        stream_id: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            kind,
            stream_id: stream_id.into(),
            media_type: media_type.into(),
            variant_key: variant_key.into(),
            details: details.into(),
        }
    }
}

/// Fixed-capacity circular buffer for recent events. O(1) insert, evicts oldest when full.
#[derive(Debug, Clone)]
pub struct EventRing {
    buffer: VecDeque<MonitorEvent>,
    capacity: usize,
}

impl EventRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, event: MonitorEvent) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(event);
    }

    pub fn list(&self) -> Vec<MonitorEvent> {
        self.buffer.iter().rev().cloned().collect()
    }

    pub fn list_chronological(&self) -> Vec<MonitorEvent> {
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

    fn make_event(kind: EventKind, detail: &str) -> MonitorEvent {
        MonitorEvent::new(kind, "VIDEO", "1200000", detail, "stream_1")
    }

    #[test]
    fn ring_push_and_list() {
        let mut ring = EventRing::new(5);
        ring.push(make_event(EventKind::CueOutStarted, "ad break 1"));
        ring.push(make_event(EventKind::CueInReturned, "ad break end"));
        assert_eq!(ring.len(), 2);

        let events = ring.list();
        assert_eq!(events[0].kind, EventKind::CueInReturned);
        assert_eq!(events[1].kind, EventKind::CueOutStarted);
    }

    #[test]
    fn ring_evicts_oldest() {
        let mut ring = EventRing::new(2);
        ring.push(make_event(EventKind::CueOutStarted, "e1"));
        ring.push(make_event(EventKind::CueInReturned, "e2"));
        ring.push(make_event(EventKind::ManifestUpdated, "e3"));
        assert_eq!(ring.len(), 2);
        let events = ring.list_chronological();
        assert_eq!(events[0].kind, EventKind::CueInReturned);
        assert_eq!(events[1].kind, EventKind::ManifestUpdated);
    }

    #[test]
    fn ring_clear() {
        let mut ring = EventRing::new(5);
        ring.push(make_event(EventKind::CueOutStarted, "e1"));
        ring.clear();
        assert!(ring.is_empty());
    }

    #[test]
    fn event_display() {
        assert_eq!(format!("{}", EventKind::CueOutStarted), "CUE-OUT");
        assert_eq!(format!("{}", EventKind::CueInReturned), "CUE-IN");
        assert_eq!(format!("{}", EventKind::DiscontinuityChanged), "DISC");
    }
}
