//! Muxlane's desktop application entry point.
//!
//! Phase 0 intentionally registers no application commands or privileged plugins.

#![forbid(unsafe_code)]

/// Starts the desktop shell without exposing runtime-management capabilities.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default().run(tauri::generate_context!())
}
