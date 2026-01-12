use crate::config::{ConfigMap, PropertySource};
use crate::merge::deep_merge;

/// Helper to manage and merge multiple `PropertySource`s.
///
/// Sources are applied in order of priority (lowest priority first, highest priority last).
/// This ensures that higher priority sources overwrite values from lower priority ones.
#[derive(Debug, Default)]
pub struct PropertySourceList {
    sources: Vec<PropertySource>,
}

impl PropertySourceList {
    /// Creates a new empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a property source to the list.
    /// The list is re-sorted by priority after insertion to ensure correct merge order.
    pub fn add(&mut self, source: PropertySource) {
        self.sources.push(source);
        // Sort by priority ascending.
        // Lower priority (e.g., 10) comes first.
        // Higher priority (e.g., 100) comes later and overwrites previous values.
        self.sources.sort_by_key(|s| s.priority);
    }

    /// Merges all sources into a single ConfigMap.
    pub fn merge(&self) -> ConfigMap {
        let mut result = ConfigMap::new();

        for source in &self.sources {
            deep_merge(&mut result, &source.config);
        }

        result
    }

    /// Returns a reference to the sorted sources.
    pub fn sources(&self) -> &[PropertySource] {
        &self.sources
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_order() {
        let mut list = PropertySourceList::new();

        let mut t1 = ConfigMap::new();
        t1.insert("key", "low");
        list.add(PropertySource {
            name: "low".into(),
            priority: 10,
            config: t1,
            origin: "".into(),
        });

        let mut t2 = ConfigMap::new();
        t2.insert("key", "high");
        list.add(PropertySource {
            name: "high".into(),
            priority: 100,
            config: t2,
            origin: "".into(),
        });

        // Add middle one last to verify sorting
        let mut t3 = ConfigMap::new();
        t3.insert("key", "mid");
        list.add(PropertySource {
            name: "mid".into(),
            priority: 50,
            config: t3,
            origin: "".into(),
        });

        // Expected order application: 10 (low) -> 50 (mid) -> 100 (high)
        // Final value should be "high"

        let merged = list.merge();
        assert_eq!(merged.get("key").unwrap().as_str(), Some("high"));
    }
}
