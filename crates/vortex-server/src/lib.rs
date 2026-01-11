//! Vortex Server - HTTP server for Vortex Config
//!
//! This crate will provide the Axum-based HTTP server implementation.

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
