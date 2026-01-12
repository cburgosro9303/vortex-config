use crate::config::ConfigMap;
use crate::error::Result;
use crate::format::{FormatParser, FormatSerializer};

pub struct YamlFormat;

impl FormatParser for YamlFormat {
    fn parse(&self, input: &str) -> Result<ConfigMap> {
        ConfigMap::from_yaml(input)
    }
}

impl FormatSerializer for YamlFormat {
    fn serialize(&self, config: &ConfigMap) -> Result<String> {
        config.to_yaml()
    }
}
