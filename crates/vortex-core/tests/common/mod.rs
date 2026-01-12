#![allow(dead_code)]
use vortex_core::{ConfigMap, PropertySource};

/// Helper to create a ConfigMap from a JSON string slice.
/// Panics if the JSON is invalid (intended for tests).
pub fn config_from_json(json: &str) -> ConfigMap {
    ConfigMap::from_json(json).expect("Failed to create test config from JSON")
}

/// Helper to create a PropertySource with a given priority.
pub fn source(name: &str, priority: i32, json_content: &str) -> PropertySource {
    let mut s = PropertySource::new(name, config_from_json(json_content));
    s.priority = priority;
    s
}

/// Returns a complex nested configuration fixture.
pub fn complex_config() -> ConfigMap {
    config_from_json(r#"{
        "server": {
            "port": 8080,
            "host": "localhost",
            "ssl": {
                "enabled": true,
                "cert": "/path/to/cert"
            }
        },
        "database": {
            "primary": {
                "url": "jdbc:postgres://local",
                "pool": 10
            }
        },
        "features": ["new-ui", "beta-api"]
    }"#)
}
