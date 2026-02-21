//! TOML configuration file schema and parsing.
//!
//! Example config file:
//!
//! ```toml
//! [server]
//! listen = "0.0.0.0:8080"
//! log_format = "json"
//!
//! [defaults]
//! stale_limit_ms = 6000
//! scte35 = false
//!
//! [[webhook]]
//! url = "https://hooks.example.com/hls-alerts"
//! events = ["error", "cue_out_started", "cue_in_returned"]
//!
//! [[monitor]]
//! id = "live-channel-1"
//! stale_limit_ms = 8000
//! scte35 = true
//! streams = [
//!   { id = "cdn-primary", url = "https://cdn1.example.com/live/master.m3u8" },
//!   { url = "https://cdn2.example.com/live/master.m3u8" },
//! ]
//! ```

use std::net::SocketAddr;
use std::path::Path;

use serde::Deserialize;

use hls_core::{MonitorConfig, StreamItem, WebhookConfig};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub defaults: DefaultsConfig,

    #[serde(default)]
    pub webhook: Vec<WebhookConfig>,

    #[serde(default)]
    pub monitor: Vec<MonitorDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: SocketAddr,

    #[serde(default = "default_log_format")]
    pub log_format: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            log_format: default_log_format(),
        }
    }
}

fn default_listen() -> SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}

fn default_log_format() -> String {
    "pretty".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct DefaultsConfig {
    #[serde(default = "default_stale_limit_ms")]
    pub stale_limit_ms: u64,

    #[serde(default)]
    pub poll_interval_ms: Option<u64>,

    #[serde(default)]
    pub scte35: bool,

    #[serde(default = "default_error_limit")]
    pub error_limit: usize,

    #[serde(default = "default_event_limit")]
    pub event_limit: usize,

    #[serde(default)]
    pub target_duration_tolerance: Option<f64>,

    #[serde(default)]
    pub mseq_gap_threshold: Option<u64>,

    #[serde(default)]
    pub variant_sync_drift_threshold: Option<u64>,

    #[serde(default)]
    pub variant_failure_threshold: Option<u32>,

    #[serde(default)]
    pub segment_duration_anomaly_ratio: Option<f64>,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            stale_limit_ms: default_stale_limit_ms(),
            poll_interval_ms: None,
            scte35: false,
            error_limit: default_error_limit(),
            event_limit: default_event_limit(),
            target_duration_tolerance: None,
            mseq_gap_threshold: None,
            variant_sync_drift_threshold: None,
            variant_failure_threshold: None,
            segment_duration_anomaly_ratio: None,
        }
    }
}

fn default_stale_limit_ms() -> u64 {
    6000
}

fn default_error_limit() -> usize {
    100
}

fn default_event_limit() -> usize {
    200
}

impl DefaultsConfig {
    pub fn to_monitor_config(&self) -> MonitorConfig {
        let mut c = MonitorConfig::default()
            .with_stale_limit(self.stale_limit_ms)
            .with_scte35(self.scte35)
            .with_error_limit(self.error_limit)
            .with_event_limit(self.event_limit);
        if let Some(pi) = self.poll_interval_ms {
            c = c.with_poll_interval(pi);
        }
        if let Some(v) = self.target_duration_tolerance {
            c = c.with_target_duration_tolerance(v);
        }
        if let Some(v) = self.mseq_gap_threshold {
            c = c.with_mseq_gap_threshold(v);
        }
        if let Some(v) = self.variant_sync_drift_threshold {
            c = c.with_variant_sync_drift_threshold(v);
        }
        if let Some(v) = self.variant_failure_threshold {
            c = c.with_variant_failure_threshold(v);
        }
        if let Some(v) = self.segment_duration_anomaly_ratio {
            c = c.with_segment_duration_anomaly_ratio(v);
        }
        c
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MonitorDef {
    pub id: String,
    pub stale_limit_ms: Option<u64>,
    pub poll_interval_ms: Option<u64>,
    pub scte35: Option<bool>,
    pub target_duration_tolerance: Option<f64>,
    pub mseq_gap_threshold: Option<u64>,
    pub variant_sync_drift_threshold: Option<u64>,
    pub variant_failure_threshold: Option<u32>,
    pub segment_duration_anomaly_ratio: Option<f64>,

    #[serde(default)]
    pub streams: Vec<StreamDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamDef {
    pub id: Option<String>,
    pub url: String,
}

impl MonitorDef {
    pub fn to_monitor_config(&self, defaults: &DefaultsConfig) -> MonitorConfig {
        let mut c = defaults.to_monitor_config();
        if let Some(sl) = self.stale_limit_ms {
            c = c.with_stale_limit(sl);
        }
        if let Some(pi) = self.poll_interval_ms {
            c = c.with_poll_interval(pi);
        }
        if let Some(scte) = self.scte35 {
            c = c.with_scte35(scte);
        }
        if let Some(v) = self.target_duration_tolerance {
            c = c.with_target_duration_tolerance(v);
        }
        if let Some(v) = self.mseq_gap_threshold {
            c = c.with_mseq_gap_threshold(v);
        }
        if let Some(v) = self.variant_sync_drift_threshold {
            c = c.with_variant_sync_drift_threshold(v);
        }
        if let Some(v) = self.variant_failure_threshold {
            c = c.with_variant_failure_threshold(v);
        }
        if let Some(v) = self.segment_duration_anomaly_ratio {
            c = c.with_segment_duration_anomaly_ratio(v);
        }
        c
    }

    pub fn to_stream_items(&self) -> Vec<StreamItem> {
        self.streams
            .iter()
            .enumerate()
            .map(|(i, s)| StreamItem {
                id: s.id.clone().unwrap_or_else(|| format!("stream_{}", i + 1)),
                url: s.url.clone(),
            })
            .collect()
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file {}: {}", path.display(), e))?;

        let config: AppConfig = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse config file {}: {}", path.display(), e))?;

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        for (i, wh) in self.webhook.iter().enumerate() {
            url::Url::parse(&wh.url)
                .map_err(|e| format!("Invalid webhook URL at index {}: {} ({})", i, wh.url, e))?;
        }

        let mut monitor_ids = std::collections::HashSet::new();
        for m in &self.monitor {
            if m.id.is_empty() {
                return Err("Monitor ID must not be empty".into());
            }
            if !monitor_ids.insert(&m.id) {
                return Err(format!("Duplicate monitor ID: {}", m.id));
            }
            if m.streams.is_empty() {
                return Err(format!("Monitor '{}' has no streams", m.id));
            }
            for (j, s) in m.streams.iter().enumerate() {
                let parsed = url::Url::parse(&s.url).map_err(|e| {
                    format!(
                        "Invalid stream URL in monitor '{}' at index {}: {} ({})",
                        m.id, j, s.url, e
                    )
                })?;
                if parsed.scheme() != "http" && parsed.scheme() != "https" {
                    return Err(format!(
                        "Stream URL must use http or https in monitor '{}': {}",
                        m.id, s.url
                    ));
                }
            }
            let stream_urls: Vec<&str> = m.streams.iter().map(|s| s.url.as_str()).collect();
            let unique: std::collections::HashSet<&str> = stream_urls.iter().copied().collect();
            if unique.len() != stream_urls.len() {
                return Err(format!("Duplicate stream URLs in monitor '{}'", m.id));
            }
        }

        match self.server.log_format.as_str() {
            "pretty" | "json" => {}
            other => {
                return Err(format!(
                    "Invalid log_format '{}': must be 'pretty' or 'json'",
                    other
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml = r#"
[[monitor]]
id = "live"
streams = [
  { url = "https://example.com/master.m3u8" },
]
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        config.validate().unwrap();
        assert_eq!(config.monitor.len(), 1);
        assert_eq!(config.monitor[0].id, "live");
        assert_eq!(
            config.monitor[0].streams[0].url,
            "https://example.com/master.m3u8"
        );
        assert_eq!(config.defaults.stale_limit_ms, 6000);
        assert_eq!(config.server.log_format, "pretty");
    }

    #[test]
    fn parse_full_config() {
        let toml = r#"
[server]
listen = "127.0.0.1:9090"
log_format = "json"

[defaults]
stale_limit_ms = 8000
scte35 = true
error_limit = 50

[[webhook]]
url = "https://hooks.example.com/alerts"
events = ["error", "cue_out_started"]
secret = "my-key"

[[monitor]]
id = "channel-1"
stale_limit_ms = 10000
streams = [
  { id = "primary", url = "https://cdn1.example.com/master.m3u8" },
  { url = "https://cdn2.example.com/master.m3u8" },
]

[[monitor]]
id = "channel-2"
scte35 = false
streams = [
  { url = "https://cdn3.example.com/master.m3u8" },
]
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        config.validate().unwrap();

        assert_eq!(config.server.listen.port(), 9090);
        assert_eq!(config.server.log_format, "json");
        assert_eq!(config.defaults.stale_limit_ms, 8000);
        assert!(config.defaults.scte35);
        assert_eq!(config.defaults.error_limit, 50);
        assert_eq!(config.webhook.len(), 1);
        assert_eq!(config.webhook[0].events, vec!["error", "cue_out_started"]);
        assert_eq!(config.webhook[0].secret.as_deref(), Some("my-key"));
        assert_eq!(config.monitor.len(), 2);

        let m1_config = config.monitor[0].to_monitor_config(&config.defaults);
        assert_eq!(m1_config.stale_limit.as_millis(), 10000);
        assert!(m1_config.scte35_enabled); // inherited from defaults

        let items = config.monitor[0].to_stream_items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "primary");
        assert_eq!(items[1].id, "stream_2"); // auto-generated

        let m2_config = config.monitor[1].to_monitor_config(&config.defaults);
        assert!(!m2_config.scte35_enabled);
    }

    #[test]
    fn validate_rejects_duplicate_monitor_ids() {
        let toml = r#"
[[monitor]]
id = "same"
streams = [{ url = "https://a.com/m.m3u8" }]

[[monitor]]
id = "same"
streams = [{ url = "https://b.com/m.m3u8" }]
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.contains("Duplicate monitor ID"), "{}", err);
    }

    #[test]
    fn validate_rejects_empty_streams() {
        let toml = r#"
[[monitor]]
id = "empty"
streams = []
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.contains("has no streams"), "{}", err);
    }

    #[test]
    fn validate_rejects_invalid_url() {
        let toml = r#"
[[monitor]]
id = "bad"
streams = [{ url = "not-a-url" }]
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.contains("Invalid stream URL"), "{}", err);
    }

    #[test]
    fn validate_rejects_invalid_webhook_url() {
        let toml = r#"
[[webhook]]
url = "not-valid"

[[monitor]]
id = "ok"
streams = [{ url = "https://example.com/m.m3u8" }]
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.contains("Invalid webhook URL"), "{}", err);
    }

    #[test]
    fn validate_rejects_invalid_log_format() {
        let toml = r#"
[server]
log_format = "xml"

[[monitor]]
id = "ok"
streams = [{ url = "https://example.com/m.m3u8" }]
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.contains("Invalid log_format"), "{}", err);
    }
}
