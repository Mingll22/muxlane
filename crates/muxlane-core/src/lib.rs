//! Shared non-domain foundations for Muxlane.
//!
//! Phase 0 establishes only the crate boundary. Domain models are intentionally deferred.

#![forbid(unsafe_code)]

/// Stable package identifier for build and integration checks.
pub const CRATE_IDENTIFIER: &str = "muxlane-core";

/// Package version supplied by Cargo at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::{CRATE_IDENTIFIER, VERSION};

    #[test]
    fn exposes_stable_build_metadata() {
        assert_eq!(CRATE_IDENTIFIER, env!("CARGO_PKG_NAME"));
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
