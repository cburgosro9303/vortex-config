use crate::config::{ConfigMap, ConfigValue};

pub mod source_list;
pub use source_list::PropertySourceList;

/// Merges an overlay configuration into a base configuration using a recursive "Deep Merge" strategy.
///
/// # Rules
/// 1. If a key exists in `overlay` but not in `base`, it is added to `base`.
/// 2. If a key exists in both:
///    a. If BOTH values are Objects (`ConfigValue::Object`), they are merged recursively.
///    b. Otherwise, the value from `overlay` overwrites the value in `base`.
/// 3. Arrays are NOT merged; the overlay array replaces the base array completely.
///
/// This function modifies `base` in-place.
pub fn deep_merge(base: &mut ConfigMap, overlay: &ConfigMap) {
    for (key, overlay_val) in overlay.as_inner() {
        match base.as_inner_mut().get_mut(key) {
            Some(base_val) => {
                merge_values(base_val, overlay_val);
            },
            None => {
                base.insert(key.clone(), overlay_val.clone());
            },
        }
    }
}

fn merge_values(base: &mut ConfigValue, overlay: &ConfigValue) {
    match (base, overlay) {
        (ConfigValue::Object(base_map), ConfigValue::Object(overlay_map)) => {
            for (key, overlay_inner_val) in overlay_map {
                match base_map.get_mut(key) {
                    Some(base_inner_val) => {
                        merge_values(base_inner_val, overlay_inner_val);
                    },
                    None => {
                        base_map.insert(key.clone(), overlay_inner_val.clone());
                    },
                }
            }
        },
        // In all other cases (primitives, arrays, mixed types), overlay wins.
        (base_val, overlay_val) => {
            *base_val = overlay_val.clone();
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigMap;

    #[test]
    fn test_deep_merge_simple() {
        let mut base = ConfigMap::from_json(r#"{"a": 1, "b": 2}"#).unwrap();
        let overlay = ConfigMap::from_json(r#"{"b": 3, "c": 4}"#).unwrap();

        deep_merge(&mut base, &overlay);

        assert_eq!(base.get("a").unwrap().as_i64(), Some(1));
        assert_eq!(base.get("b").unwrap().as_i64(), Some(3)); // Overwritten
        assert_eq!(base.get("c").unwrap().as_i64(), Some(4)); // Added
    }

    #[test]
    fn test_deep_merge_nested() {
        let mut base = ConfigMap::from_json(
            r#"
        {
            "server": {
                "port": 8080,
                "host": "localhost"
            },
            "logging": "INFO"
        }
        "#,
        )
        .unwrap();

        let overlay = ConfigMap::from_json(
            r#"
        {
            "server": {
                "host": "0.0.0.0",
                "timeout": 30
            },
            "logging": {
                "level": "DEBUG"
            }
        }
        "#,
        )
        .unwrap();

        deep_merge(&mut base, &overlay);

        // Merged object
        assert_eq!(base.get("server.port").unwrap().as_i64(), Some(8080)); // Preserved
        assert_eq!(base.get("server.host").unwrap().as_str(), Some("0.0.0.0")); // Overridden
        assert_eq!(base.get("server.timeout").unwrap().as_i64(), Some(30)); // Added

        // Type change (String -> Object)
        // logging was "INFO", now implementation detail: ConfigValue::String vs ConfigValue::Object
        // Overlay wins, so it becomes an object
        assert!(base.get("logging.level").is_some());
        assert_eq!(base.get("logging").unwrap().as_str(), None); // No longer a string
    }

    #[test]
    fn test_array_replacement() {
        let mut base = ConfigMap::from_json(r#"{"items": [1, 2]}"#).unwrap();
        let overlay = ConfigMap::from_json(r#"{"items": [3, 4, 5]}"#).unwrap();

        deep_merge(&mut base, &overlay);

        let items = base.get("items").unwrap().as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].as_i64(), Some(3));
    }
}
