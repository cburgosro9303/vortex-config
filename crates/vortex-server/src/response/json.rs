use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::handlers::response::ConfigResponse;

pub fn to_response(data: &ConfigResponse) -> Result<Response, super::SerializeError> {
    let body = serde_json::to_string_pretty(data)?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response())
}
