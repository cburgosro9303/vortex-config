//! Modulo de serializacion de respuestas.
//!
//! Proporciona funciones para serializar ConfigResponse a diferentes formatos:
//! - JSON (por defecto)
//! - YAML
//! - Properties (.properties de Java)

pub mod json;
pub mod properties;
pub mod yaml;

use axum::response::{IntoResponse, Response};

use crate::extractors::accept::OutputFormat;
use crate::handlers::response::ConfigResponse;

/// Error de serializacion.
#[derive(Debug)]
pub enum SerializeError {
    Json(serde_json::Error),
    Yaml(serde_yaml::Error),
}

impl From<serde_json::Error> for SerializeError {
    fn from(err: serde_json::Error) -> Self {
        SerializeError::Json(err)
    }
}

impl From<serde_yaml::Error> for SerializeError {
    fn from(err: serde_yaml::Error) -> Self {
        SerializeError::Yaml(err)
    }
}

impl IntoResponse for SerializeError {
    fn into_response(self) -> Response {
        let message = match self {
            SerializeError::Json(e) => format!("JSON serialization error: {}", e),
            SerializeError::Yaml(e) => format!("YAML serialization error: {}", e),
        };

        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message).into_response()
    }
}

/// Convierte ConfigResponse al formato especificado.
pub fn to_format(
    config: &ConfigResponse,
    format: OutputFormat,
) -> Result<Response, SerializeError> {
    match format {
        OutputFormat::Json => json::to_response(config),
        OutputFormat::Yaml => yaml::to_response(config),
        OutputFormat::Properties => properties::to_response(config),
    }
}
