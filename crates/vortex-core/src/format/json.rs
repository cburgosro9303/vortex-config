use crate::config::ConfigMap;
use crate::error::Result;
use crate::format::{FormatParser, FormatSerializer};

pub struct JsonFormat;

impl FormatParser for JsonFormat {
    fn parse(&self, input: &str) -> Result<ConfigMap> {
        ConfigMap::from_json(input)
    }
}

impl FormatSerializer for JsonFormat {
    fn serialize(&self, config: &ConfigMap) -> Result<String> {
        config.to_json()
    }
}
