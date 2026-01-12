use crate::handlers::response::ConfigResponse;
use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

/// Convierte ConfigResponse a formato .properties de Java.
pub fn to_response(config: &ConfigResponse) -> Result<Response, super::SerializeError> {
    let mut output = String::new();

    // Agregar comentario con metadata
    output.push_str(&format!("# Application: {}\n", config.name));
    output.push_str(&format!("# Profiles: {}\n", config.profiles.join(",")));
    if let Some(ref label) = config.label {
        output.push_str(&format!("# Label: {}\n", label));
    }
    output.push('\n');

    // Iterar property sources (en orden inverso para precedencia correcta)
    for ps in config.property_sources.iter().rev() {
        output.push_str(&format!("# Source: {}\n", ps.name));

        for (key, value) in &ps.source {
            let value_str = json_value_to_properties_string(value);
            // Escapar caracteres especiales en key
            let escaped_key = escape_properties_key(key);
            output.push_str(&format!("{}={}\n", escaped_key, value_str));
        }
        output.push('\n');
    }

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        output,
    )
        .into_response())
}

/// Convierte un JSON value a string para .properties.
fn json_value_to_properties_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => escape_properties_value(s),
        serde_json::Value::Array(arr) => {
            // Arrays como lista separada por comas
            arr.iter()
                .map(json_value_to_properties_string)
                .collect::<Vec<_>>()
                .join(",")
        },
        serde_json::Value::Object(_) => {
            // Objetos como JSON inline (no ideal, pero funcional)
            value.to_string()
        },
    }
}

/// Escapa caracteres especiales en keys de properties.
fn escape_properties_key(key: &str) -> String {
    key.replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('=', "\\=")
        .replace(' ', "\\ ")
}

/// Escapa caracteres especiales en values de properties.
fn escape_properties_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
