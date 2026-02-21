pub mod app;
pub mod error;
pub mod metrics;
pub mod routes;
pub mod state;

use std::future::Future;
use std::net::SocketAddr;

use crate::app::build_app;
use crate::state::AppState;

pub async fn serve(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    serve_with_state(addr, AppState::new(), shutdown_signal()).await
}

pub async fn serve_with_state(
    addr: SocketAddr,
    state: AppState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Listening on {}", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}

pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
