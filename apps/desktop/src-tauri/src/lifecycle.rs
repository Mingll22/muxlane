//! Windows tray and close policy. Runtime processes remain owned by WSL/tmux;
//! dropping the GUI never tears them down implicitly.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use tauri::{
    App, AppHandle, Emitter, Manager, State,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
};

pub struct LifecycleState {
    exiting: AtomicBool,
    tray: Mutex<Option<TrayIcon>>,
}

impl LifecycleState {
    pub fn new() -> Self {
        Self { exiting: AtomicBool::new(false), tray: Mutex::new(None) }
    }

    pub fn exiting(&self) -> bool {
        self.exiting.load(Ordering::Acquire)
    }
}

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "打开 Muxlane", true, None::<&str>)?;
    let background = MenuItem::with_id(app, "background", "保持后台运行", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出…", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &background, &quit])?;
    let mut builder = TrayIconBuilder::with_id("muxlane-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Muxlane · 0 个运行项目")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "background" => hide_main(app),
            "quit" => request_close(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;
    if let Ok(mut slot) = app.state::<LifecycleState>().tray.lock() {
        *slot = Some(tray);
    }
    Ok(())
}

pub fn request_close(app: &AppHandle) {
    show_main(app);
    let _ = app.emit("muxlane-smart-close", ());
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn hide_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn desktop_update_running_count(
    state: State<'_, LifecycleState>,
    count: u32,
) -> Result<(), String> {
    if count > 10_000 {
        return Err("running Project count is invalid".to_owned());
    }
    let tray = state.tray.lock().map_err(|_| "tray state unavailable".to_owned())?;
    tray.as_ref()
        .ok_or_else(|| "tray icon unavailable".to_owned())?
        .set_tooltip(Some(format!("Muxlane · {count} 个运行项目")))
        .map_err(|_| "tray tooltip update failed".to_owned())
}

#[tauri::command]
pub fn desktop_close_action(
    app: AppHandle,
    state: State<'_, LifecycleState>,
    action: String,
) -> Result<(), String> {
    match action.as_str() {
        "background" => {
            hide_main(&app);
            Ok(())
        }
        "exit" => {
            state.exiting.store(true, Ordering::Release);
            app.exit(0);
            Ok(())
        }
        "cancel" => Ok(()),
        _ => Err("close action is invalid".to_owned()),
    }
}

#[tauri::command]
pub fn desktop_set_fullscreen(app: AppHandle, enabled: bool) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "main window unavailable".to_owned())?
        .set_fullscreen(enabled)
        .map_err(|_| "fullscreen transition failed".to_owned())
}
