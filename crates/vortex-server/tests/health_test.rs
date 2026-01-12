use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vortex_server::create_router;

#[tokio::test]
async fn health_check_returns_200() {
    let app = create_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_check_returns_json() {
    let app = create_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();

    assert!(content_type.contains("application/json"));
}

#[tokio::test]
async fn health_check_body_contains_status_up() {
    let app = create_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let health: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(health["status"], "UP");
}

#[test]
fn health_response_serializes_correctly() {
    use vortex_server::HealthResponse;

    let response = HealthResponse::default();
    let json = serde_json::to_string(&response).unwrap();

    assert_eq!(json, r#"{"status":"UP"}"#);
}
