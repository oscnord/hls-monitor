use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use hls_core::{Monitor, MonitorConfig, Notification};

#[derive(Clone)]
pub struct AppState {
    pub monitors: Arc<DashMap<Uuid, Arc<Monitor>>>,
    pub default_config: MonitorConfig,
    pub notification_tx: Option<UnboundedSender<Notification>>,
    pub allowed_origins: Vec<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            monitors: Arc::new(DashMap::new()),
            default_config: MonitorConfig::default(),
            notification_tx: None,
            allowed_origins: Vec::new(),
        }
    }

    pub fn with_notification_tx(mut self, tx: UnboundedSender<Notification>) -> Self {
        self.notification_tx = Some(tx);
        self
    }

    pub fn with_default_config(mut self, config: MonitorConfig) -> Self {
        self.default_config = config;
        self
    }

    pub fn with_allowed_origins(mut self, origins: Vec<String>) -> Self {
        self.allowed_origins = origins;
        self
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
