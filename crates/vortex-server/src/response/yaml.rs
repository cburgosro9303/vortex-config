use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::handlers::response::ConfigResponse;

pub fn to_response(data: &ConfigResponse) -> Result<Response, super::SerializeError> {
    let body = serde_yaml::to_string(data)?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/x-yaml")],
        body,
    )
        .into_response())
}
