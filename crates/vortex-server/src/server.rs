use std::net::SocketAddr;

use axum::{Router, routing::get};
use tower::ServiceBuilder;

use crate::handlers::{
    config::{get_config, get_config_with_label},
    health::health_check,
};
use crate::middleware::{LoggingLayer, RequestIdLayer};

pub fn create_router() -> Router {
    // Middleware stack: RequestId primero, luego Logging
    let middleware = ServiceBuilder::new()
        .layer(RequestIdLayer)
        .layer(LoggingLayer);

    Router::new()
        .route("/health", get(health_check))
        // Rutas con mas segmentos primero
        .route("/:app/:profile/:label", get(get_config_with_label))
        .route("/:app/:profile", get(get_config))
        .layer(middleware)
}

pub async fn run_server(addr: SocketAddr) -> Result<(), std::io::Error> {
    let app = create_router();

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received");
}
