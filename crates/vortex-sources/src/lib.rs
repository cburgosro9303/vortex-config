//! Vortex Sources - Configuration backends
//!
//! This crate will provide implementations for different configuration sources
//! including Git, S3, and SQL backends.

/// Returns the crate version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_defined() {
        assert!(!version().is_empty());
    }
}
