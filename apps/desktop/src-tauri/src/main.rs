//! Native executable entry point for the Muxlane desktop shell.

#![forbid(unsafe_code)]

use {muxlane_protocol as _, serde_json as _, tauri as _};

fn main() {
    if let Err(error) = muxlane_desktop::run() {
        eprintln!("Muxlane desktop shell failed to start: {error}");
        std::process::exit(1);
    }
}
