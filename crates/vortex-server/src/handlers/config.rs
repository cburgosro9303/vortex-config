use axum::{
    extract::{Path, Query},
    response::Response,
};
use tracing::instrument;

use crate::error::AppError;
use crate::extractors::{
    accept::AcceptFormat,
    path::{AppProfileLabelPath, AppProfilePath},
    query::ConfigQuery,
};
use crate::handlers::response::{ConfigResponse, PropertySourceResponse};
use crate::response::to_format;

/// Handler para GET /{app}/{profile}
#[instrument(skip_all, fields(app = %path.app, profile = %path.profile))]
pub async fn get_config(
    Path(path): Path<AppProfilePath>,
    Query(_query): Query<ConfigQuery>,
    AcceptFormat(format): AcceptFormat,
) -> Result<Response, AppError> {
    // Validar parametros
    path.validate().map_err(AppError::BadRequest)?;

    let profiles = path.profiles();

    tracing::info!("Fetching config for {}/{:?}", path.app, profiles);

    // TODO: Integrar con ConfigSource real
    // Por ahora retornamos datos mock
    let response = create_mock_response(&path.app, profiles, None);

    to_format(&response, format).map_err(|e| AppError::Internal(format!("{:?}", e)))
}

/// Handler para GET /{app}/{profile}/{label}
#[instrument(skip_all, fields(
    app = %path.app,
    profile = %path.profile,
    label = %path.label
))]
pub async fn get_config_with_label(
    Path(path): Path<AppProfileLabelPath>,
    Query(query): Query<ConfigQuery>,
    AcceptFormat(format): AcceptFormat,
) -> Result<Response, AppError> {
    path.validate().map_err(AppError::BadRequest)?;

    let profiles = path.profiles();
    let label = path.sanitized_label();

    tracing::info!(
        use_default_label = query.use_default_label,
        "Fetching config with label"
    );

    // Validar caracteres peligrosos en label
    validate_label(&label)?;

    let response = create_mock_response(&path.app, profiles, Some(label));
    to_format(&response, format).map_err(|e| AppError::Internal(format!("{:?}", e)))
}

/// Valida que el label no contenga caracteres peligrosos.
fn validate_label(label: &str) -> Result<(), AppError> {
    // Prevenir path traversal
    if label.contains("..") {
        return Err(AppError::BadRequest(
            "Label cannot contain '..'".to_string(),
        ));
    }

    // Prevenir caracteres de control
    if label.chars().any(|c| c.is_control()) {
        return Err(AppError::BadRequest(
            "Label cannot contain control characters".to_string(),
        ));
    }

    Ok(())
}

fn create_mock_response(app: &str, profiles: Vec<String>, label: Option<String>) -> ConfigResponse {
    use std::collections::HashMap;

    let mut source = HashMap::new();
    source.insert(
        "server.port".to_string(),
        serde_json::Value::Number(8080.into()),
    );
    source.insert(
        "spring.application.name".to_string(),
        serde_json::Value::String(app.to_string()),
    );

    if let Some(l) = &label {
        source.insert(
            "label".to_string(),
            serde_json::Value::String(l.to_string()),
        );
    }

    let source_name = match &label {
        Some(l) => format!("git:{}:config/{}.yml", l, app),
        None => format!("file:config/{}.yml", app),
    };

    ConfigResponse {
        name: app.to_string(),
        profiles: profiles.clone(),
        label,
        version: None,
        state: None,
        property_sources: vec![PropertySourceResponse {
            name: source_name,
            source,
        }],
    }
}
