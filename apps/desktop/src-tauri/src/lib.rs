//! Muxlane's desktop application entry point.
//!
//! Phase 3 exposes only a fixed, typed terminal POC bridge. It does not expose
//! filesystem, shell, network, account, or production Runtime capabilities.

#![forbid(unsafe_code)]

mod phase3;

/// Starts the desktop shell with Phase 3's finite terminal POC command surface.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .manage(phase3::Phase3State::new())
        .invoke_handler(tauri::generate_handler![
            phase3::phase3_probe,
            phase3::phase3_list_sessions,
            phase3::phase3_create_synthetic_session,
            phase3::phase3_list_windows,
            phase3::phase3_create_window,
            phase3::phase3_attach,
            phase3::phase3_start_stream,
            phase3::phase3_detach,
            phase3::phase3_send_input,
            phase3::phase3_resize,
            phase3::phase3_close_window,
            phase3::phase3_cleanup_session,
        ])
        .run(tauri::generate_context!())
}
