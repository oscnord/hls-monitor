#![forbid(unsafe_code)]

pub mod config;
pub mod loader;
pub mod monitor;
pub mod webhook;

pub use config::MonitorConfig;
pub use loader::{HttpLoader, LoadError, ManifestLoader};
pub use monitor::{
    ErrorRing, ErrorType, EventKind, EventRing, Monitor, MonitorError, MonitorEvent, MonitorState,
    StreamItem, StreamStatus, VariantStatus,
};
pub use webhook::{
    notification_channel, Notification, WebhookConfig, WebhookDispatcher, WebhookPayload,
};
