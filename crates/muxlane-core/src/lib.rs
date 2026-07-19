//! Durable domain, persistence, credential, recovery, and terminal foundations
//! for the Muxlane WSL runtime control plane.

#![forbid(unsafe_code)]

pub mod credential;
pub mod diagnostics;
pub mod error;
pub mod incident;
pub mod layout;
pub mod lock;
pub mod model;
pub mod process;
pub mod recovery;
pub mod service;
pub mod session;
pub mod storage;
pub mod terminal;
pub mod usage;
pub mod workbench;
pub mod workspace;

pub use error::{CoreError, CoreResult};

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
