use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use hls_core::{HttpLoader, Monitor, MonitorConfig, MonitorEvent, StreamItem, StreamStatus};

use crate::error::ApiError;
use crate::state::AppState;

/// A stream input: either a bare URL string or `{ id, url }` object.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum StreamInput {
    Url(String),
    Object { id: Option<String>, url: String },
}

impl StreamInput {
    fn url(&self) -> &str {
        match self {
            StreamInput::Url(u) => u,
            StreamInput::Object { url, .. } => url,
        }
    }

    fn into_stream_item(self, index: usize) -> StreamItem {
        match self {
            StreamInput::Url(url) => StreamItem {
                id: format!("stream_{}", index + 1),
                url,
            },
            StreamInput::Object { id, url } => StreamItem {
                id: id.unwrap_or_else(|| format!("stream_{}", index + 1)),
                url,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateMonitorRequest {
    pub streams: Vec<StreamInput>,
    pub stale_limit: Option<u64>,
    pub poll_interval: Option<u64>,
    #[serde(default)]
    pub scte35: bool,
    pub target_duration_tolerance: Option<f64>,
    pub mseq_gap_threshold: Option<u64>,
    pub variant_sync_drift_threshold: Option<u64>,
    pub variant_failure_threshold: Option<u32>,
    pub segment_duration_anomaly_ratio: Option<f64>,
    pub max_concurrent_fetches: Option<usize>,
}

#[derive(Serialize)]
pub struct CreateMonitorResponse {
    pub id: Uuid,
    pub streams: Vec<StreamItem>,
    pub stale_limit_ms: u64,
    pub poll_interval_ms: u64,
    pub scte35: bool,
}

#[derive(Serialize)]
pub struct MonitorSummary {
    pub id: Uuid,
    pub state: String,
    pub created_at: String,
    pub stream_count: usize,
    pub error_count: usize,
}

#[derive(Serialize)]
pub struct MonitorDetail {
    pub id: Uuid,
    pub state: String,
    pub created_at: String,
    pub last_checked: Option<String>,
    pub streams: Vec<StreamItem>,
    pub stale_limit_ms: u64,
    pub poll_interval_ms: u64,
    pub scte35: bool,
    pub error_count: usize,
}

#[derive(Serialize)]
pub struct StreamsResponse {
    pub streams: Vec<StreamItem>,
}

#[derive(Debug, Deserialize)]
pub struct AddStreamsRequest {
    pub streams: Vec<StreamInput>,
}

#[derive(Serialize)]
pub struct AddStreamsResponse {
    pub message: String,
    pub streams: Vec<StreamItem>,
}

#[derive(Serialize)]
pub struct RemoveStreamResponse {
    pub message: String,
    pub streams: Vec<StreamItem>,
}

#[derive(Serialize)]
pub struct ErrorsResponse {
    pub last_checked: Option<String>,
    pub state: String,
    pub errors: Vec<hls_core::MonitorError>,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Serialize)]
pub struct DeleteMonitorResponse {
    pub message: String,
    pub id: Uuid,
}

#[derive(Serialize)]
pub struct DeleteAllResponse {
    pub message: String,
    pub deleted_count: usize,
    pub deleted_ids: Vec<Uuid>,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub monitor_id: Uuid,
    pub state: String,
    pub streams: Vec<StreamStatus>,
}

#[derive(Serialize)]
pub struct EventsResponse {
    pub monitor_id: Uuid,
    pub events: Vec<MonitorEvent>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/monitors", post(create_monitor).get(list_monitors).delete(delete_all_monitors))
        .route("/monitors/{id}", get(get_monitor).delete(delete_monitor))
        .route("/monitors/{id}/start", post(start_monitor))
        .route("/monitors/{id}/stop", post(stop_monitor))
        .route(
            "/monitors/{id}/streams",
            get(get_streams).put(add_streams),
        )
        .route("/monitors/{id}/streams/{stream_id}", delete(remove_stream))
        .route(
            "/monitors/{id}/errors",
            get(get_errors).delete(clear_errors),
        )
        .route("/monitors/{id}/status", get(get_status))
        .route("/monitors/{id}/events", get(get_events))
}

fn is_valid_url(s: &str) -> bool {
    url::Url::parse(s)
        .map(|u| u.scheme() == "http" || u.scheme() == "https")
        .unwrap_or(false)
}

fn parse_monitor_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest(format!("Invalid monitor ID: {}", id)))
}

fn get_monitor_arc(
    state: &AppState,
    id: Uuid,
) -> Result<Arc<Monitor>, ApiError> {
    state
        .monitors
        .get(&id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| ApiError::NotFound(format!("Monitor {} not found", id)))
}

/// POST /api/v1/monitors
async fn create_monitor(
    State(state): State<AppState>,
    Json(body): Json<CreateMonitorRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if body.streams.is_empty() {
        return Err(ApiError::BadRequest("streams array must not be empty".into()));
    }

    let invalid: Vec<&str> = body.streams.iter().map(|s| s.url()).filter(|u| !is_valid_url(u)).collect();
    if !invalid.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "Invalid URLs: {}",
            invalid.join(", ")
        )));
    }

    let urls: Vec<&str> = body.streams.iter().map(|s| s.url()).collect();
    let unique: std::collections::HashSet<&str> = urls.iter().copied().collect();
    if unique.len() != urls.len() {
        return Err(ApiError::BadRequest(
            "Duplicate stream URLs are not allowed within the same monitor".into(),
        ));
    }

    let config = {
        let mut c = MonitorConfig::default().with_scte35(body.scte35);
        if let Some(sl) = body.stale_limit {
            c = c.with_stale_limit(sl);
        }
        if let Some(pi) = body.poll_interval {
            c = c.with_poll_interval(pi);
        }
        if let Some(v) = body.target_duration_tolerance {
            c = c.with_target_duration_tolerance(v);
        }
        if let Some(v) = body.mseq_gap_threshold {
            c = c.with_mseq_gap_threshold(v);
        }
        if let Some(v) = body.variant_sync_drift_threshold {
            c = c.with_variant_sync_drift_threshold(v);
        }
        if let Some(v) = body.variant_failure_threshold {
            c = c.with_variant_failure_threshold(v);
        }
        if let Some(v) = body.segment_duration_anomaly_ratio {
            c = c.with_segment_duration_anomaly_ratio(v);
        }
        if let Some(v) = body.max_concurrent_fetches {
            c = c.with_max_concurrent_fetches(v);
        }
        c
    };

    let stale_limit_ms = config.stale_limit.as_millis() as u64;
    let poll_interval_ms = config.poll_interval.as_millis() as u64;

    let items: Vec<StreamItem> = body
        .streams
        .into_iter()
        .enumerate()
        .map(|(i, s)| s.into_stream_item(i))
        .collect();

    let loader = Arc::new(HttpLoader::from_config(&config));
    let monitor = Monitor::new(items.clone(), config, loader, state.notification_tx.clone());
    let id = monitor.id();

    state.monitors.insert(id, Arc::new(monitor));

    let resp = CreateMonitorResponse {
        id,
        streams: items,
        stale_limit_ms,
        poll_interval_ms,
        scte35: body.scte35,
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/v1/monitors
async fn list_monitors(State(state): State<AppState>) -> impl IntoResponse {
    let mut summaries = Vec::new();
    for entry in state.monitors.iter() {
        let m = entry.value();
        summaries.push(MonitorSummary {
            id: m.id(),
            state: m.state().await.to_string(),
            created_at: m.created_at().to_rfc3339(),
            stream_count: m.streams().await.len(),
            error_count: m.get_errors().await.len(),
        });
    }
    Json(summaries)
}

/// GET /api/v1/monitors/:id
async fn get_monitor(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MonitorDetail>, ApiError> {
    let id = parse_monitor_id(&id)?;
    let m = get_monitor_arc(&state, id)?;

    let detail = MonitorDetail {
        id: m.id(),
        state: m.state().await.to_string(),
        created_at: m.created_at().to_rfc3339(),
        last_checked: m.last_checked().await.map(|t| t.to_rfc3339()),
        streams: m.streams().await,
        stale_limit_ms: m.config().stale_limit.as_millis() as u64,
        poll_interval_ms: m.config().poll_interval.as_millis() as u64,
        scte35: m.config().scte35_enabled,
        error_count: m.get_errors().await.len(),
    };

    Ok(Json(detail))
}

/// DELETE /api/v1/monitors/:id
async fn delete_monitor(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteMonitorResponse>, ApiError> {
    let id = parse_monitor_id(&id)?;

    let (_, monitor) = state
        .monitors
        .remove(&id)
        .ok_or_else(|| ApiError::NotFound(format!("Monitor {} not found", id)))?;

    monitor.stop().await;

    Ok(Json(DeleteMonitorResponse {
        message: "Monitor stopped and deleted".into(),
        id,
    }))
}

/// DELETE /api/v1/monitors
async fn delete_all_monitors(State(state): State<AppState>) -> Json<DeleteAllResponse> {
    let mut ids = Vec::new();
    let entries: Vec<_> = state
        .monitors
        .iter()
        .map(|e| (*e.key(), Arc::clone(e.value())))
        .collect();

    for (id, monitor) in &entries {
        monitor.stop().await;
        state.monitors.remove(id);
        ids.push(*id);
    }

    Json(DeleteAllResponse {
        message: "All monitors stopped and deleted".into(),
        deleted_count: ids.len(),
        deleted_ids: ids,
    })
}

/// POST /api/v1/monitors/:id/start
async fn start_monitor(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ApiError> {
    let id = parse_monitor_id(&id)?;
    let m = get_monitor_arc(&state, id)?;

    m.start()
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(MessageResponse {
        message: "Monitor started".into(),
    }))
}

/// POST /api/v1/monitors/:id/stop
async fn stop_monitor(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ApiError> {
    let id = parse_monitor_id(&id)?;
    let m = get_monitor_arc(&state, id)?;

    m.stop().await;

    Ok(Json(MessageResponse {
        message: "Monitor stopped".into(),
    }))
}

/// GET /api/v1/monitors/:id/streams
async fn get_streams(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StreamsResponse>, ApiError> {
    let id = parse_monitor_id(&id)?;
    let m = get_monitor_arc(&state, id)?;

    Ok(Json(StreamsResponse {
        streams: m.streams().await,
    }))
}

/// PUT /api/v1/monitors/:id/streams
async fn add_streams(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AddStreamsRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let id = parse_monitor_id(&id)?;
    let m = get_monitor_arc(&state, id)?;

    if body.streams.is_empty() {
        return Err(ApiError::BadRequest("streams array must not be empty".into()));
    }

    let invalid: Vec<&str> = body.streams.iter().map(|s| s.url()).filter(|u| !is_valid_url(u)).collect();
    if !invalid.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "Invalid URLs: {}",
            invalid.join(", ")
        )));
    }

    let new_urls: Vec<&str> = body.streams.iter().map(|s| s.url()).collect();
    let unique: std::collections::HashSet<&str> = new_urls.iter().copied().collect();
    if unique.len() != new_urls.len() {
        return Err(ApiError::BadRequest(
            "Duplicate stream URLs in request".into(),
        ));
    }

    let existing = m.streams().await;
    let existing_urls: std::collections::HashSet<String> =
        existing.iter().map(|s| s.url.clone()).collect();
    let already: Vec<&str> = new_urls
        .iter()
        .filter(|u| existing_urls.contains(**u))
        .copied()
        .collect();
    if !already.is_empty() {
        return Err(ApiError::Conflict(format!(
            "{} stream(s) are already being monitored",
            already.len()
        )));
    }

    let base_index = existing.len();
    let new_items: Vec<StreamItem> = body
        .streams
        .into_iter()
        .enumerate()
        .map(|(i, s)| s.into_stream_item(base_index + i))
        .collect();

    m.add_streams(new_items).await;

    let all_streams = m.streams().await;

    Ok((
        StatusCode::CREATED,
        Json(AddStreamsResponse {
            message: "Streams added".into(),
            streams: all_streams,
        }),
    ))
}

/// DELETE /api/v1/monitors/:id/streams/:stream_id
async fn remove_stream(
    State(state): State<AppState>,
    Path((id, stream_id)): Path<(String, String)>,
) -> Result<Json<RemoveStreamResponse>, ApiError> {
    let id = parse_monitor_id(&id)?;
    let m = get_monitor_arc(&state, id)?;

    m.remove_stream(&stream_id)
        .await
        .map_err(ApiError::NotFound)?;

    let remaining = m.streams().await;

    Ok(Json(RemoveStreamResponse {
        message: "Stream removed".into(),
        streams: remaining,
    }))
}

/// GET /api/v1/monitors/:id/errors
async fn get_errors(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ErrorsResponse>, ApiError> {
    let id = parse_monitor_id(&id)?;
    let m = get_monitor_arc(&state, id)?;

    let errors = m.get_errors().await;
    let last_checked = m.last_checked().await.map(|t| t.to_rfc3339());
    let monitor_state = m.state().await.to_string();

    Ok(Json(ErrorsResponse {
        last_checked,
        state: monitor_state,
        errors,
    }))
}

/// DELETE /api/v1/monitors/:id/errors
async fn clear_errors(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ApiError> {
    let id = parse_monitor_id(&id)?;
    let m = get_monitor_arc(&state, id)?;

    m.clear_errors().await;

    Ok(Json(MessageResponse {
        message: "Errors cleared".into(),
    }))
}

/// GET /api/v1/monitors/:id/status
async fn get_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StatusResponse>, ApiError> {
    let id = parse_monitor_id(&id)?;
    let m = get_monitor_arc(&state, id)?;

    let streams = m.get_stream_status().await;
    let monitor_state = m.state().await.to_string();

    Ok(Json(StatusResponse {
        monitor_id: m.id(),
        state: monitor_state,
        streams,
    }))
}

/// GET /api/v1/monitors/:id/events
async fn get_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EventsResponse>, ApiError> {
    let id = parse_monitor_id(&id)?;
    let m = get_monitor_arc(&state, id)?;

    let events = m.get_events().await;

    Ok(Json(EventsResponse {
        monitor_id: m.id(),
        events,
    }))
}
