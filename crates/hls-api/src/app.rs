use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::metrics::metrics_handler;
use crate::routes;
use crate::state::AppState;

pub fn build_app(state: AppState) -> Router {
    let api_v1 = routes::router();

    Router::new()
        .nest("/api/v1", api_v1)
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}
