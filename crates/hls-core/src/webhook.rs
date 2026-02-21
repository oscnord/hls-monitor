//! Webhook notification system.
//!
//! When the monitor detects errors or notable events, it can optionally push
//! them through an mpsc channel. The [`WebhookDispatcher`] reads from that
//! channel and POSTs JSON payloads to all configured webhook endpoints.

use std::time::Duration;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::sync::mpsc;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::monitor::error::MonitorError;
use crate::monitor::event::{EventKind, MonitorEvent};

/// Configuration for a single webhook endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// The URL to POST payloads to.
    pub url: String,

    /// Which notification types to deliver. Empty means all.
    #[serde(default)]
    pub events: Vec<String>,

    #[serde(default = "default_webhook_timeout_ms")]
    pub timeout_ms: u64,

    #[serde(default = "default_webhook_retries")]
    pub max_retries: u32,

    /// Optional HMAC-SHA256 signing secret for `X-HLS-Signature-256` header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

fn default_webhook_timeout_ms() -> u64 {
    5000
}

fn default_webhook_retries() -> u32 {
    2
}

impl WebhookConfig {
    pub fn accepts(&self, notification_type: &str) -> bool {
        if self.events.is_empty() {
            return true;
        }
        self.events.iter().any(|e| e == notification_type)
    }
}

/// A notification produced by the monitoring engine, dispatched as a webhook.
#[derive(Debug, Clone)]
pub enum Notification {
    Error {
        monitor_id: String,
        error: MonitorError,
    },
    Event {
        monitor_id: String,
        event: MonitorEvent,
    },
}

impl Notification {
    pub fn notification_type(&self) -> &str {
        match self {
            Notification::Error { .. } => "error",
            Notification::Event { event, .. } => match event.kind {
                EventKind::CueOutStarted => "cue_out_started",
                EventKind::CueInReturned => "cue_in_returned",
                EventKind::CueOutCont => "cue_out_cont",
                EventKind::DiscontinuityChanged => "discontinuity_changed",
                EventKind::ManifestUpdated => "manifest_updated",
                EventKind::StaleRecovered => "stale_recovered",
            },
        }
    }
}

/// The JSON envelope POSTed to webhook endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    pub version: u8,
    pub id: String,
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "type")]
    pub notification_type: String,
    pub monitor_id: String,
    pub stream_id: String,
    pub data: serde_json::Value,
}

impl WebhookPayload {
    pub fn from_notification(notification: &Notification) -> Self {
        match notification {
            Notification::Error { monitor_id, error } => Self {
                version: 1,
                id: Uuid::new_v4().to_string(),
                timestamp: error.timestamp,
                notification_type: "error".to_string(),
                monitor_id: monitor_id.clone(),
                stream_id: error.stream_id.clone(),
                data: serde_json::json!({
                    "error_type": error.error_type.to_string(),
                    "media_type": error.media_type,
                    "variant": error.variant,
                    "details": error.details,
                    "url": error.stream_url,
                    "status_code": error.status_code,
                }),
            },
            Notification::Event { monitor_id, event } => Self {
                version: 1,
                id: Uuid::new_v4().to_string(),
                timestamp: event.timestamp,
                notification_type: notification.notification_type().to_string(),
                monitor_id: monitor_id.clone(),
                stream_id: event.stream_id.clone(),
                data: serde_json::json!({
                    "kind": event.kind,
                    "media_type": event.media_type,
                    "variant_key": event.variant_key,
                    "details": event.details,
                }),
            },
        }
    }
}

/// Asynchronous webhook dispatcher.
///
/// Spawned as a background tokio task, it reads from the notification channel
/// and POSTs payloads to all configured webhook endpoints.
pub struct WebhookDispatcher {
    rx: mpsc::UnboundedReceiver<Notification>,
    webhooks: Vec<WebhookConfig>,
    client: Client,
}

impl WebhookDispatcher {
    pub fn new(
        rx: mpsc::UnboundedReceiver<Notification>,
        webhooks: Vec<WebhookConfig>,
        client: Client,
    ) -> Self {
        Self {
            rx,
            webhooks,
            client,
        }
    }

    /// Run the dispatcher loop. Returns when all senders are dropped.
    pub async fn run(mut self) {
        debug!(
            webhook_count = self.webhooks.len(),
            "Webhook dispatcher started"
        );

        while let Some(notification) = self.rx.recv().await {
            let payload = WebhookPayload::from_notification(&notification);
            let notification_type = notification.notification_type().to_string();

            for wh in &self.webhooks {
                if !wh.accepts(&notification_type) {
                    continue;
                }

                let json_bytes = match serde_json::to_vec(&payload) {
                    Ok(b) => b,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize webhook payload");
                        continue;
                    }
                };

                let timeout = Duration::from_millis(wh.timeout_ms);

                if let Err(e) = deliver(
                    &self.client,
                    &wh.url,
                    &json_bytes,
                    wh.secret.as_deref(),
                    timeout,
                    wh.max_retries,
                )
                .await
                {
                    warn!(
                        url = %wh.url,
                        notification_type,
                        error = %e,
                        "Webhook delivery failed"
                    );
                } else {
                    debug!(url = %wh.url, notification_type, "Webhook delivered");
                }
            }
        }

        debug!("Webhook dispatcher shutting down");
    }
}

pub fn notification_channel() -> (mpsc::UnboundedSender<Notification>, mpsc::UnboundedReceiver<Notification>) {
    mpsc::unbounded_channel()
}

async fn deliver(
    client: &Client,
    url: &str,
    body: &[u8],
    secret: Option<&str>,
    timeout: Duration,
    max_retries: u32,
) -> Result<(), String> {
    let mut last_error = String::new();

    for attempt in 0..=max_retries {
        if attempt > 0 {
            let backoff = Duration::from_millis(500 * 2u64.pow(attempt - 1));
            tokio::time::sleep(backoff).await;
        }

        let mut req = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "hls-monitor/0.1")
            .timeout(timeout)
            .body(body.to_vec());

        if let Some(secret) = secret {
            let signature = sign_payload(body, secret);
            req = req.header("X-HLS-Signature-256", format!("sha256={}", signature));
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => {
                let status = resp.status();
                last_error = format!("HTTP {} from {}", status, url);
                if status.as_u16() >= 400 && status.as_u16() < 500 && status.as_u16() != 429 {
                    return Err(last_error);
                }
            }
            Err(e) => {
                last_error = format!("Request to {} failed: {}", url, e);
            }
        }
    }

    Err(last_error)
}

fn sign_payload(body: &[u8], secret: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_config_accepts_all_when_empty() {
        let wh = WebhookConfig {
            url: "https://example.com".into(),
            events: vec![],
            timeout_ms: 5000,
            max_retries: 2,
            secret: None,
        };
        assert!(wh.accepts("error"));
        assert!(wh.accepts("cue_out_started"));
        assert!(wh.accepts("manifest_updated"));
    }

    #[test]
    fn webhook_config_filters_by_event_type() {
        let wh = WebhookConfig {
            url: "https://example.com".into(),
            events: vec!["error".into(), "cue_out_started".into()],
            timeout_ms: 5000,
            max_retries: 2,
            secret: None,
        };
        assert!(wh.accepts("error"));
        assert!(wh.accepts("cue_out_started"));
        assert!(!wh.accepts("manifest_updated"));
        assert!(!wh.accepts("stale_recovered"));
    }

    #[test]
    fn notification_type_for_error() {
        let n = Notification::Error {
            monitor_id: "m1".into(),
            error: MonitorError::new(
                crate::monitor::error::ErrorType::StaleManifest,
                "VIDEO",
                "1200000",
                "stale",
                "https://example.com/",
                "s1",
            ),
        };
        assert_eq!(n.notification_type(), "error");
    }

    #[test]
    fn notification_type_for_events() {
        let make = |kind: EventKind| Notification::Event {
            monitor_id: "m1".into(),
            event: MonitorEvent::new(kind, "VIDEO", "1200000", "detail", "s1"),
        };
        assert_eq!(make(EventKind::CueOutStarted).notification_type(), "cue_out_started");
        assert_eq!(make(EventKind::CueInReturned).notification_type(), "cue_in_returned");
        assert_eq!(make(EventKind::ManifestUpdated).notification_type(), "manifest_updated");
        assert_eq!(make(EventKind::StaleRecovered).notification_type(), "stale_recovered");
    }

    #[test]
    fn payload_from_error_notification() {
        let n = Notification::Error {
            monitor_id: "m1".into(),
            error: MonitorError::new(
                crate::monitor::error::ErrorType::StaleManifest,
                "VIDEO",
                "1200000",
                "Manifest stale for 8000ms",
                "https://example.com/",
                "stream_1",
            ),
        };
        let payload = WebhookPayload::from_notification(&n);
        assert_eq!(payload.version, 1);
        assert_eq!(payload.notification_type, "error");
        assert_eq!(payload.monitor_id, "m1");
        assert_eq!(payload.stream_id, "stream_1");
        assert_eq!(payload.data["error_type"], "Stale Manifest");
        assert_eq!(payload.data["details"], "Manifest stale for 8000ms");
    }

    #[test]
    fn payload_from_event_notification() {
        let n = Notification::Event {
            monitor_id: "live-1".into(),
            event: MonitorEvent::new(
                EventKind::CueOutStarted,
                "VIDEO",
                "1200000",
                "Ad break started at mseq 42",
                "stream_1",
            ),
        };
        let payload = WebhookPayload::from_notification(&n);
        assert_eq!(payload.version, 1);
        assert_eq!(payload.notification_type, "cue_out_started");
        assert_eq!(payload.monitor_id, "live-1");
        assert_eq!(payload.data["kind"], "cue_out_started");
        assert_eq!(payload.data["details"], "Ad break started at mseq 42");
    }

    #[test]
    fn hmac_signature_is_deterministic() {
        let body = b"test payload";
        let sig1 = sign_payload(body, "my-secret");
        let sig2 = sign_payload(body, "my-secret");
        assert_eq!(sig1, sig2);
        assert!(!sig1.is_empty());

        let sig3 = sign_payload(body, "other-secret");
        assert_ne!(sig1, sig3);
    }

    #[tokio::test]
    async fn dispatcher_processes_and_shuts_down() {
        let (tx, rx) = notification_channel();
        let dispatcher = WebhookDispatcher::new(rx, vec![], Client::new());

        // Send a notification then drop the sender
        tx.send(Notification::Error {
            monitor_id: "m1".into(),
            error: MonitorError::new(
                crate::monitor::error::ErrorType::StaleManifest,
                "VIDEO",
                "1200000",
                "stale",
                "https://example.com/",
                "s1",
            ),
        })
        .unwrap();
        drop(tx);

        tokio::time::timeout(Duration::from_secs(2), dispatcher.run())
            .await
            .expect("Dispatcher should exit after sender is dropped");
    }
}
