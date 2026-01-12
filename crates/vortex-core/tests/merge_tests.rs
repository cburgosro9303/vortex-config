use vortex_core::merge::PropertySourceList;
use vortex_core::merge::deep_merge;

mod common;

#[test]
fn test_cascading_merge_scenario() {
    // 1. Base (Application defaults)
    let mut base = common::config_from_json(
        r#"{
        "server": { "port": 8000, "host": "localhost" },
        "logging": { "level": "INFO", "file": "app.log" }
    }"#,
    );

    // 2. Overlay (Production profile)
    let prod = common::config_from_json(
        r#"{
        "server": { "port": 80 },
        "logging": { "level": "WARN" }
    }"#,
    );

    deep_merge(&mut base, &prod);

    // Verification
    assert_eq!(base.get("server.port").unwrap().as_i64(), Some(80)); // Overridden
    assert_eq!(base.get("server.host").unwrap().as_str(), Some("localhost")); // Preserved
    assert_eq!(base.get("logging.level").unwrap().as_str(), Some("WARN")); // Overridden
    assert_eq!(base.get("logging.file").unwrap().as_str(), Some("app.log")); // Preserved
}

#[test]
fn test_property_source_list_precedence() {
    let mut list = PropertySourceList::new();

    // Priority 10: Defaults
    list.add(common::source(
        "defaults",
        10,
        r#"{"app": {"timeout": 5000, "retries": 3}}"#,
    ));

    // Priority 20: App Config
    list.add(common::source(
        "app.yml",
        20,
        r#"{"app": {"timeout": 1000}}"#,
    ));

    // Priority 100: Env Vars (simulated)
    list.add(common::source("env", 100, r#"{"app": {"retries": 5}}"#));

    let merged = list.merge();

    // Timeout: app.yml (20) overrides defaults (10). Env (100) didn't specify it.
    assert_eq!(merged.get("app.timeout").unwrap().as_i64(), Some(1000));

    // Retries: Env (100) overrides defaults (10). App (20) didn't specify it.
    assert_eq!(merged.get("app.retries").unwrap().as_i64(), Some(5));
}

#[test]
fn test_array_semantics_replacement() {
    // Arrays should be replaced, not merged
    let mut list = PropertySourceList::new();

    list.add(common::source("base", 1, r#"{"whitelist": ["127.0.0.1"]}"#));
    list.add(common::source(
        "overlay",
        2,
        r#"{"whitelist": ["10.0.0.1", "10.0.0.2"]}"#,
    ));

    let merged = list.merge();

    let whitelist = merged.get("whitelist").unwrap().as_array().unwrap();
    assert_eq!(whitelist.len(), 2);
    assert_eq!(whitelist[0].as_str(), Some("10.0.0.1"));
}
