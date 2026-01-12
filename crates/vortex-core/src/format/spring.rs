use crate::config::{ConfigMap, ConfigValue, PropertySource};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Response struct compatible with Spring Cloud Config Server v2.
///
/// This serves as a Data Transfer Object (DTO) to ensure the JSON output
/// matches exactly what Spring Boot clients expect.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringConfigResponse {
    pub name: String,
    pub profiles: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,

    #[serde(rename = "propertySources")]
    pub property_sources: Vec<SpringPropertySource>,
}

/// Property source in Spring format (flattened keys).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringPropertySource {
    pub name: String,
    pub source: IndexMap<String, ConfigValue>,
}

impl SpringConfigResponse {
    /// Creates a new builder/response.
    pub fn new(name: impl Into<String>, profiles: Vec<String>) -> Self {
        Self {
            name: name.into(),
            profiles,
            label: None,
            version: None,
            state: None,
            property_sources: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn add_source(mut self, source: SpringPropertySource) -> Self {
        self.property_sources.push(source);
        self
    }
}

// ==========================================
// Conversion Logic (Adapters)
// ==========================================

impl From<&PropertySource> for SpringPropertySource {
    fn from(ps: &PropertySource) -> Self {
        SpringPropertySource {
            name: ps.name.clone(),
            source: flatten_config_map(&ps.config),
        }
    }
}

/// Flattens a hierarchical ConfigMap into a flat map with dot-notation keys.
///
/// Example:
/// {"server": {"port": 80}} -> {"server.port": 80}
pub fn flatten_config_map(config: &ConfigMap) -> IndexMap<String, ConfigValue> {
    let mut flat_map = IndexMap::new();
    for (key, value) in config.as_inner() {
        flatten_value(key, value, &mut flat_map);
    }
    flat_map
}

fn flatten_value(prefix: &str, value: &ConfigValue, target: &mut IndexMap<String, ConfigValue>) {
    match value {
        ConfigValue::Object(map) => {
            for (curr_key, curr_val) in map {
                let new_key = format!("{}.{}", prefix, curr_key);
                flatten_value(&new_key, curr_val, target);
            }
        },
        // For Spring compatibility, arrays are often treated as values or indexed keys.
        // Here we treat array as a value (leaf) as per our planning decision.
        _ => {
            target.insert(prefix.to_string(), value.clone());
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigMap;

    #[test]
    fn test_flattening_logic() {
        let json = r#"{
            "server": {
                "port": 8080,
                "ssl": {
                    "enabled": true
                }
            },
            "app": "test"
        }"#;
        let config = ConfigMap::from_json(json).unwrap();
        let flat = flatten_config_map(&config);

        assert_eq!(flat.get("server.port").unwrap().as_i64(), Some(8080));
        assert_eq!(
            flat.get("server.ssl.enabled").unwrap().as_bool(),
            Some(true)
        );
        assert_eq!(flat.get("app").unwrap().as_str(), Some("test"));

        // Ensure intermediate keys shouldn't exist as separate entries
        assert!(flat.get("server").is_none());
        assert!(flat.get("server.ssl").is_none());
    }

    #[test]
    fn test_spring_response_serialization() {
        let mut response = SpringConfigResponse::new("myapp", vec!["prod".into()]);
        response = response.with_version("v1");

        let mut source_map = IndexMap::new();
        source_map.insert("key".into(), ConfigValue::String("value".into()));

        response.property_sources.push(SpringPropertySource {
            name: "test.yml".into(),
            source: source_map,
        });

        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains(r#""name":"myapp""#));
        assert!(json.contains(r#""propertySources""#)); // camelCase check
        assert!(json.contains(r#""profiles":["prod"]"#));
        assert!(json.contains(r#""version":"v1""#));
        assert!(!json.contains("state")); // Skipped none
    }
}
