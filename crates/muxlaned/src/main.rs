//! Muxlane daemon foundation.
//!
//! Phase 0 intentionally exposes metadata-only help and version output.

#![forbid(unsafe_code)]

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "muxlaned",
    version,
    about = "Muxlane daemon foundation",
    long_about = "Muxlane is in Pre-alpha. Phase 0 starts no background services."
)]
struct DaemonCli;

fn main() {
    let _cli = DaemonCli::parse();
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::DaemonCli;

    #[test]
    fn exposes_help_and_version_metadata() {
        let help = DaemonCli::command().render_long_help().to_string();
        assert!(help.contains("starts no background services"));

        let version = DaemonCli::try_parse_from(["muxlaned", "--version"])
            .expect_err("the version flag must exit through clap's display path");
        assert_eq!(version.kind(), clap::error::ErrorKind::DisplayVersion);
    }
}
