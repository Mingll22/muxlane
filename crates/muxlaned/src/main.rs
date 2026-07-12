//! Muxlane's non-production Phase 3 terminal POC entry point.

#![forbid(unsafe_code)]

mod phase3;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "muxlaned",
    version,
    about = "Muxlane non-production terminal POC",
    long_about = "Phase 3 validates a local tmux terminal bridge. It starts no production daemon or Runtime."
)]
struct DaemonCli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Phase 3-only commands. These are not a stable production API.
    Phase3 {
        #[command(subcommand)]
        command: Phase3Command,
    },
}

#[derive(Debug, Subcommand)]
enum Phase3Command {
    /// Serve typed JSON-lines control frames and terminal data events over stdio.
    Gateway {
        #[arg(long, default_value = "muxlane-p3")]
        socket: String,
    },
    /// A deterministic terminal program used only by Phase 3 tmux tests.
    SyntheticRunner,
}

fn main() {
    let cli = DaemonCli::parse();
    let result = match cli.command {
        Some(Command::Phase3 { command: Phase3Command::Gateway { socket } }) => {
            phase3::run_gateway(socket)
                .map_err(|error| format!("{}: {}", error.code, error.message))
        }
        Some(Command::Phase3 { command: Phase3Command::SyntheticRunner }) => {
            phase3::run_synthetic_runner().map_err(|error| error.to_string())
        }
        None => Ok(()),
    };
    if let Err(error) = result {
        eprintln!("muxlaned phase 3 POC failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::DaemonCli;

    #[test]
    fn exposes_a_scoped_phase3_gateway_without_a_daemon_mode() {
        let help = DaemonCli::command().render_long_help().to_string();
        assert!(help.contains("Phase 3"));
        assert!(help.contains("starts no production daemon"));

        let version = DaemonCli::try_parse_from(["muxlaned", "--version"])
            .expect_err("the version flag exits through clap's display path");
        assert_eq!(version.kind(), clap::error::ErrorKind::DisplayVersion);
    }
}
