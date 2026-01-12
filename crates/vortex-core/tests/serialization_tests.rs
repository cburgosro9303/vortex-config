use vortex_core::ConfigMap;
use vortex_core::format::json::JsonFormat;
use vortex_core::format::properties::PropertiesFormat;
use vortex_core::format::yaml::YamlFormat;
use vortex_core::format::{FormatParser, FormatSerializer};

mod common;

#[test]
fn test_json_yaml_interop() {
    let original = common::complex_config();

    // Test using the Traits abstraction
    let json_fmt = JsonFormat;
    let yaml_fmt = YamlFormat;

    // Config -> JSON (via Trait)
    let json = json_fmt.serialize(&original).unwrap();

    // JSON -> Config -> YAML (via Trait)
    let from_json = json_fmt.parse(&json).unwrap();
    let yaml = yaml_fmt.serialize(&from_json).unwrap();

    // YAML -> Config (via Trait)
    let final_config = yaml_fmt.parse(&yaml).unwrap();

    assert_eq!(
        original, final_config,
        "Lost data during JSON <-> YAML conversion"
    );
}

#[test]
fn test_properties_serialization() {
    let original = common::complex_config();
    let serializer = PropertiesFormat;

    let props = serializer.serialize(&original).unwrap();

    assert!(props.contains("server.port=8080"));
    assert!(props.contains("server.ssl.enabled=true"));
    // Arrays might have debug representation in current MVP properties impl
    assert!(props.contains("features="));
}

#[test]
fn test_special_characters_preservation() {
    let json = r#"{
        "msg": "Hello\nWorld",
        "path": "C:\\Windows\\System32",
        "unicode": "Ñandú 🐍"
    }"#;

    let config = common::config_from_json(json);

    // Test JSON roundtrip
    let generated_json = config.to_json().unwrap();
    let roundtrip = ConfigMap::from_json(&generated_json).unwrap();

    assert_eq!(config, roundtrip);

    let msg = roundtrip.get("msg").unwrap().as_str().unwrap();
    assert_eq!(msg, "Hello\nWorld");

    let unicode = roundtrip.get("unicode").unwrap().as_str().unwrap();
    assert_eq!(unicode, "Ñandú 🐍");
}
