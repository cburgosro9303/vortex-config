use crate::config::ConfigMap;
use crate::error::Result;

pub mod json;
pub mod properties;
pub mod spring;
pub mod yaml;

/// Supported configuration formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Json,
    Yaml,
    Properties,
}

impl ConfigFormat {
    /// Returns the file extensions associated with this format.
    pub fn extensions(&self) -> &[&str] {
        match self {
            ConfigFormat::Json => &["json"],
            ConfigFormat::Yaml => &["yaml", "yml"],
            ConfigFormat::Properties => &["properties"],
        }
    }

    /// Guesses the format from a file extension (without dot).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "json" => Some(ConfigFormat::Json),
            "yaml" | "yml" => Some(ConfigFormat::Yaml),
            "properties" => Some(ConfigFormat::Properties),
            _ => None,
        }
    }
}

/// A trait for parsing configuration from a string.
pub trait FormatParser: Send + Sync {
    /// Parses the input string into a ConfigMap.
    fn parse(&self, input: &str) -> Result<ConfigMap>;
}

/// A trait for serializing configuration to a string.
pub trait FormatSerializer: Send + Sync {
    /// Serializes the ConfigMap into a string.
    fn serialize(&self, config: &ConfigMap) -> Result<String>;
}
