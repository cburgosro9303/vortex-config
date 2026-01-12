//! Middleware de logging estructurado.

use axum::{
    body::Body,
    http::{Request, Response},
};
use std::{
    task::{Context, Poll},
    time::Instant,
};
use tower::{Layer, Service};
use tracing::{Instrument, info, info_span};

use super::request_id::REQUEST_ID_HEADER;

/// Layer that logs requests and responses.
#[derive(Clone, Default)]
pub struct LoggingLayer;

impl<S> Layer<S> for LoggingLayer {
    type Service = LoggingMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LoggingMiddleware { inner }
    }
}

/// Middleware that logs request/response details.
#[derive(Clone)]
pub struct LoggingMiddleware<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for LoggingMiddleware<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let start = Instant::now();
        let method = request.method().clone();
        let uri = request.uri().clone();
        let path = uri.path().to_string();

        // Extract request ID (should have been set by RequestIdMiddleware)
        let request_id = request
            .headers()
            .get(&REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        // Create span with request context
        let span = info_span!(
            "http_request",
            request_id = %request_id,
            method = %method,
            path = %path,
        );

        let mut inner = self.inner.clone();

        Box::pin(
            async move {
                info!("Request started");

                let response = inner.call(request).await?;

                let status = response.status().as_u16();
                let duration = start.elapsed();

                info!(
                    status = status,
                    duration_ms = duration.as_millis() as u64,
                    "Request completed"
                );

                Ok(response)
            }
            .instrument(span),
        )
    }
}
