use crate::config::{ConfigMap, ConfigValue};
use crate::error::{Result, VortexError};
use crate::format::{FormatParser, FormatSerializer};
use indexmap::IndexMap;

pub struct PropertiesFormat;

impl FormatParser for PropertiesFormat {
    fn parse(&self, input: &str) -> Result<ConfigMap> {
        let mut root = IndexMap::new();

        for (line_num, line) in input.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
                continue;
            }

            if let Some((key, value)) = split_property_line(line) {
                insert_nested(&mut root, key.trim(), value.trim());
            } else {
                return Err(VortexError::parse_error(
                    "properties",
                    format!("Invalid syntax at line {}: missing separator", line_num + 1),
                ));
            }
        }

        Ok(ConfigMap::from_inner(root))
    }
}

impl FormatSerializer for PropertiesFormat {
    fn serialize(&self, config: &ConfigMap) -> Result<String> {
        // Reuse the flattening logic from spring module if available,
        // or implement local flattening to ensure simple "key=value" output.
        // For properties, we generally want Dot Notation.

        // We use the flatten function defined in `spring` module as it does exactly what we need:
        // transforms nested map into dot-notation flat map.
        use crate::format::spring::flatten_config_map;

        let flat_map = flatten_config_map(config);
        let mut output = String::new();

        for (key, value) in flat_map {
            let val_str = match value {
                ConfigValue::String(s) => escape_value(&s),
                ConfigValue::Null => "".to_string(),
                ConfigValue::Bool(b) => b.to_string(),
                ConfigValue::Integer(i) => i.to_string(),
                ConfigValue::Float(f) => f.to_string(),
                // Arrays and Objects shouldn't happen if flattened correctly,
                // but if an array is a leaf, we print it as string representation for now
                // or just skip. Spring Properties handling of arrays is complex (indices).
                // MVP: Debug print
                v => format!("{:?}", v),
            };

            output.push_str(&format!("{}={}\n", key, val_str));
        }

        Ok(output)
    }
}

fn split_property_line(line: &str) -> Option<(&str, &str)> {
    // Split on first '=' or ':'
    line.split_once(['=', ':'])
}

fn insert_nested(root: &mut IndexMap<String, ConfigValue>, key: &str, value: &str) {
    if !key.contains('.') {
        root.insert(key.to_string(), ConfigValue::String(value.to_string()));
        return;
    }

    let parts: Vec<&str> = key.split('.').collect();
    let val = ConfigValue::String(value.to_string());

    // Recursive insertion simulation using references
    // This is tricky with Rust ownership.
    // Easier approach: Recursive function or iterative pointer chase.

    // Iterative approach to find/create the parent object
    let mut current_map = root;

    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // Last part: insert value
            current_map.insert(part.to_string(), val.clone());
        } else {
            // Intermediate part: ensure it exists and is an object
            current_map
                .entry(part.to_string())
                .and_modify(|v| {
                    if !matches!(v, ConfigValue::Object(_)) {
                        // Conflict: key exists but is not an object.
                        // In properties logic, last write usually wins or merges.
                        // We overwrite with a new object to support the nesting.
                        *v = ConfigValue::Object(IndexMap::new());
                    }
                })
                .or_insert_with(|| ConfigValue::Object(IndexMap::new()));

            // Move pointer down
            // We need to re-get mutably to bypass borrow checker limitations with `entry` when moving deeper
            // Unwrapping is safe because we just inserted/ensured it.
            if let Some(ConfigValue::Object(next_map)) = current_map.get_mut(*part) {
                current_map = next_map;
            } else {
                unreachable!("Should be an object");
            }
        }
    }
}

fn escape_value(s: &str) -> String {
    // Basic escaping for .properties
    s.replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_properties() {
        let input = "
        # Server config
        server.port=8080
        server.host: localhost
        app.name = Test App
        ";

        let parser = PropertiesFormat;
        let config = parser.parse(input).unwrap();

        assert_eq!(config.get("server.port").unwrap().as_str(), Some("8080")); // Parsed as string by default
        assert_eq!(
            config.get("server.host").unwrap().as_str(),
            Some("localhost")
        );
        assert_eq!(config.get("app.name").unwrap().as_str(), Some("Test App"));
    }

    #[test]
    fn test_serialize_properties() {
        let json = r#"{"a": {"b": "c"}, "d": 10}"#;
        let config = ConfigMap::from_json(json).unwrap();

        let serializer = PropertiesFormat;
        let output = serializer.serialize(&config).unwrap();

        assert!(output.contains("a.b=c"));
        assert!(output.contains("d=10"));
    }
}
