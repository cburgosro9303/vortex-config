//! Custom assertions para tests.

use serde_json::Value;

/// Verifica que una respuesta JSON tenga el schema de Spring Cloud Config.
pub fn assert_spring_config_schema(json: &Value) {
    assert!(json.is_object(), "Response should be a JSON object");

    let obj = json.as_object().unwrap();

    // Campos requeridos
    assert!(obj.contains_key("name"), "Missing 'name' field");
    assert!(obj.contains_key("profiles"), "Missing 'profiles' field");
    assert!(
        obj.contains_key("propertySources"),
        "Missing 'propertySources' field"
    );

    // Validar tipos
    assert!(obj["name"].is_string(), "'name' should be a string");
    assert!(obj["profiles"].is_array(), "'profiles' should be an array");
    assert!(
        obj["propertySources"].is_array(),
        "'propertySources' should be an array"
    );

    // Validar estructura de propertySources
    if let Some(sources) = obj["propertySources"].as_array() {
        for source in sources {
            assert!(source.is_object(), "PropertySource should be an object");
            let ps = source.as_object().unwrap();
            assert!(ps.contains_key("name"), "PropertySource missing 'name'");
            assert!(ps.contains_key("source"), "PropertySource missing 'source'");
            assert!(
                ps["source"].is_object(),
                "PropertySource 'source' should be an object"
            );
        }
    }

    // Campos opcionales pueden ser null
    if obj.contains_key("label") {
        assert!(
            obj["label"].is_null() || obj["label"].is_string(),
            "'label' should be null or string"
        );
    }

    if obj.contains_key("version") {
        assert!(
            obj["version"].is_null() || obj["version"].is_string(),
            "'version' should be null or string"
        );
    }
}

/// Verifica que el response YAML sea valido.
pub fn assert_valid_yaml(text: &str) {
    let result: Result<Value, _> = serde_yaml::from_str(text);
    assert!(result.is_ok(), "Invalid YAML: {}", text);
}

/// Verifica que el response Properties tenga formato correcto.
pub fn assert_valid_properties(text: &str) {
    for line in text.lines() {
        let trimmed = line.trim();

        // Ignorar lineas vacias y comentarios
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Debe tener formato key=value
        assert!(
            trimmed.contains('='),
            "Invalid properties line (missing '='): {}",
            line
        );
    }
}
