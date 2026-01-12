use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
}

impl Default for HealthResponse {
    fn default() -> Self {
        Self {
            status: "UP".to_string(),
        }
    }
}

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse::default())
}
