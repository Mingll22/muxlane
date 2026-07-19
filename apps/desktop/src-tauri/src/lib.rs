//! Muxlane's desktop application entry point.
//!
//! The formal Windows adapter exposes only fixed, typed control and Terminal
//! operations. The Phase 3 POC bridge remains isolated for compatibility tests.

#![forbid(unsafe_code)]

mod lifecycle;
mod runtime;

use tauri::Manager;

/// Starts the desktop shell with the formal control and Terminal data planes.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .manage(runtime::RuntimeState::new())
        .manage(lifecycle::LifecycleState::new())
        .setup(lifecycle::setup)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<lifecycle::LifecycleState>();
                if !state.exiting() {
                    api.prevent_close();
                    lifecycle::request_close(window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            runtime::runtime_doctor,
            runtime::runtime_environment_check,
            runtime::runtime_handshake,
            runtime::runtime_control,
            runtime::runtime_open_workspace_location,
            runtime::runtime_status,
            runtime::runtime_daemon_start,
            runtime::runtime_daemon_stop,
            runtime::runtime_terminal_attach,
            runtime::runtime_terminal_start,
            runtime::runtime_terminal_detach,
            runtime::runtime_terminal_switch,
            runtime::runtime_terminal_input,
            runtime::runtime_terminal_resize,
            runtime::runtime_terminal_close,
            lifecycle::desktop_update_running_count,
            lifecycle::desktop_close_action,
            lifecycle::desktop_set_fullscreen,
        ])
        .run(tauri::generate_context!())
}
