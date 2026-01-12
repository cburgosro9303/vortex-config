use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vortex_server::create_router;

#[tokio::test]
async fn get_config_with_label_returns_200() {
    let app = create_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/myapp/dev/main")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_config_with_label_includes_label_in_response() {
    let app = create_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/myapp/dev/feature-branch")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(config["label"], "feature-branch");
}

#[tokio::test]
async fn get_config_without_label_has_null_label() {
    let app = create_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/myapp/dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(config["label"].is_null());
}

#[tokio::test]
async fn get_config_decodes_url_encoded_label() {
    let app = create_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/myapp/dev/feature%2Fawesome")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(config["label"], "feature/awesome");
}

#[tokio::test]
async fn get_config_with_query_params() {
    let app = create_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/myapp/dev/main?useDefaultLabel=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_config_rejects_path_traversal() {
    let app = create_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/myapp/dev/..%2F..%2Fetc%2Fpasswd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
