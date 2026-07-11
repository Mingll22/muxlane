//! Muxlane's command-line interface foundation.
//!
//! Phase 0 intentionally exposes metadata-only help and version output.

#![forbid(unsafe_code)]

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "muxlane",
    version,
    about = "Muxlane command-line interface foundation",
    long_about = "Muxlane is in Pre-alpha. Phase 0 exposes no runtime-management commands."
)]
struct Cli;

fn main() {
    let _cli = Cli::parse();
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::Cli;

    #[test]
    fn exposes_help_and_version_metadata() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("runtime-management commands"));

        let version = Cli::try_parse_from(["muxlane", "--version"])
            .expect_err("the version flag must exit through clap's display path");
        assert_eq!(version.kind(), clap::error::ErrorKind::DisplayVersion);
    }
}
