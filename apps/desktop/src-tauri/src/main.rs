//! Native executable entry point for the Muxlane desktop shell.

#![forbid(unsafe_code)]

fn main() {
    if let Err(error) = muxlane_desktop::run() {
        eprintln!("Muxlane desktop shell failed to start: {error}");
        std::process::exit(1);
    }
}
