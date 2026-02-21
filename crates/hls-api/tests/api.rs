//! API integration tests for hls-api routes.
//!
//! Uses Axum's `tower::ServiceExt` to send requests directly to the app
//! without binding a TCP socket.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use hls_api::app::build_app;
use hls_api::state::AppState;

fn app() -> axum::Router {
    let state = AppState::new();
    build_app(state)
}

async fn body_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn json_request(method: &str, uri: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(b) = body {
        builder.body(Body::from(serde_json::to_vec(&b).unwrap())).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    }
}

#[tokio::test]
async fn health_returns_ok() {
    let app = app();
    let resp = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"ok");
}

#[tokio::test]
async fn metrics_returns_openmetrics() {
    let app = app();
    let resp = app
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("openmetrics-text"));
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(text.contains("# EOF"));
}

#[tokio::test]
async fn create_monitor_returns_201() {
    let app = app();
    let resp = app
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({
                "streams": ["https://example.com/master.m3u8"],
                "stale_limit": 8000
            })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    assert!(body["id"].is_string());
    assert_eq!(body["stale_limit_ms"], 8000);
    assert_eq!(body["poll_interval_ms"], 4000);
    assert_eq!(body["streams"][0]["url"], "https://example.com/master.m3u8");
}

#[tokio::test]
async fn create_monitor_with_object_streams() {
    let app = app();
    let resp = app
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({
                "streams": [
                    "https://example.com/a.m3u8",
                    { "id": "custom_id", "url": "https://example.com/b.m3u8" }
                ]
            })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["streams"][1]["id"], "custom_id");
    assert_eq!(body["streams"][1]["url"], "https://example.com/b.m3u8");
}

#[tokio::test]
async fn create_monitor_rejects_empty_streams() {
    let app = app();
    let resp = app
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({ "streams": [] })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_monitor_rejects_invalid_urls() {
    let app = app();
    let resp = app
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({ "streams": ["not-a-url"] })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_json(resp.into_body()).await;
    assert!(body["message"].as_str().unwrap().contains("Invalid URLs"));
}

#[tokio::test]
async fn create_monitor_rejects_duplicate_urls() {
    let app = app();
    let resp = app
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({
                "streams": [
                    "https://example.com/a.m3u8",
                    "https://example.com/a.m3u8"
                ]
            })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_json(resp.into_body()).await;
    assert!(body["message"].as_str().unwrap().contains("Duplicate"));
}

#[tokio::test]
async fn list_monitors_empty() {
    let app = app();
    let resp = app
        .oneshot(Request::builder().uri("/api/v1/monitors").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert!(body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_monitor_not_found() {
    let app = app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/monitors/00000000-0000-0000-0000-000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_monitor_invalid_id() {
    let app = app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/monitors/not-a-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn full_monitor_lifecycle() {
    let state = AppState::new();
    let app = build_app(state.clone());

    // Create
    let resp = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({
                "streams": ["https://example.com/master.m3u8"]
            })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    let monitor_id = body["id"].as_str().unwrap().to_string();

    // Get detail
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/monitors/{}", monitor_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["state"], "idle");
    assert_eq!(body["streams"][0]["url"], "https://example.com/master.m3u8");

    // List (should have 1)
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/monitors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Get streams
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/monitors/{}/streams", monitor_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["streams"].as_array().unwrap().len(), 1);

    // Get errors (should be empty)
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/monitors/{}/errors", monitor_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["state"], "idle");
    assert!(body["errors"].as_array().unwrap().is_empty());

    // Delete
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/monitors/{}", monitor_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // List again (should be empty)
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/monitors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    assert!(body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn add_streams_to_monitor() {
    let state = AppState::new();
    let app = build_app(state);

    // Create monitor
    let resp = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({
                "streams": ["https://example.com/a.m3u8"]
            })),
        ))
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    let id = body["id"].as_str().unwrap().to_string();

    // Add streams
    let resp = app
        .clone()
        .oneshot(json_request(
            "PUT",
            &format!("/api/v1/monitors/{}/streams", id),
            Some(json!({
                "streams": ["https://example.com/b.m3u8"]
            })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["streams"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn add_streams_rejects_duplicates() {
    let state = AppState::new();
    let app = build_app(state);

    // Create
    let resp = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({
                "streams": ["https://example.com/a.m3u8"]
            })),
        ))
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    let id = body["id"].as_str().unwrap().to_string();

    // Try adding duplicate
    let resp = app
        .oneshot(json_request(
            "PUT",
            &format!("/api/v1/monitors/{}/streams", id),
            Some(json!({
                "streams": ["https://example.com/a.m3u8"]
            })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn remove_stream_from_monitor() {
    let state = AppState::new();
    let app = build_app(state);

    // Create
    let resp = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({
                "streams": [
                    "https://example.com/a.m3u8",
                    { "id": "keep_me", "url": "https://example.com/b.m3u8" }
                ]
            })),
        ))
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    let id = body["id"].as_str().unwrap().to_string();

    // Remove stream_1
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/monitors/{}/streams/stream_1", id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["streams"].as_array().unwrap().len(), 1);
    assert_eq!(body["streams"][0]["id"], "keep_me");
}

#[tokio::test]
async fn remove_stream_not_found() {
    let state = AppState::new();
    let app = build_app(state);

    let resp = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({
                "streams": ["https://example.com/a.m3u8"]
            })),
        ))
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    let id = body["id"].as_str().unwrap().to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/monitors/{}/streams/nonexistent", id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_all_monitors() {
    let state = AppState::new();
    let app = build_app(state);

    // Create two monitors
    app.clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({ "streams": ["https://example.com/a.m3u8"] })),
        ))
        .await
        .unwrap();
    app.clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({ "streams": ["https://example.com/b.m3u8"] })),
        ))
        .await
        .unwrap();

    // Delete all
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/monitors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["deleted_count"], 2);

    // Verify empty
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/monitors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    assert!(body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_status_returns_empty_streams() {
    let state = AppState::new();
    let app = build_app(state);

    // Create monitor
    let resp = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({
                "streams": ["https://example.com/master.m3u8"]
            })),
        ))
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    let id = body["id"].as_str().unwrap().to_string();

    // Get status (no polls yet, so streams array should be empty)
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/monitors/{}/status", id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["state"], "idle");
    assert!(body["streams"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_status_not_found() {
    let app = app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/monitors/00000000-0000-0000-0000-000000000000/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_events_returns_empty() {
    let state = AppState::new();
    let app = build_app(state);

    // Create monitor
    let resp = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({
                "streams": ["https://example.com/master.m3u8"]
            })),
        ))
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    let id = body["id"].as_str().unwrap().to_string();

    // Get events (none yet)
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/monitors/{}/events", id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert!(body["events"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_events_not_found() {
    let app = app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/monitors/00000000-0000-0000-0000-000000000000/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn clear_errors_returns_ok() {
    let state = AppState::new();
    let app = build_app(state);

    let resp = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/monitors",
            Some(json!({ "streams": ["https://example.com/a.m3u8"] })),
        ))
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    let id = body["id"].as_str().unwrap().to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/monitors/{}/errors", id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["message"], "Errors cleared");
}
