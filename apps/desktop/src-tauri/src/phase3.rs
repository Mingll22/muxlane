//! Windows-host bridge for the explicitly non-production Phase 3 terminal POC.
//!
//! The WebView never receives a shell or executable argument. The host launches
//! only `wsl.exe --exec /usr/bin/env muxlaned phase3 gateway --socket muxlane-p3`, then maps
//! the POC's typed stdio frames to a finite Tauri command/event surface.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    time::Duration,
};

use muxlane_protocol::{
    AttachedTerminal, ManagedSession, ManagedWindow, Phase3Error, Phase3Frame, Phase3Request,
    Phase3RequestEnvelope, Phase3Response, ResultFrame,
};
use tauri::{AppHandle, Emitter, State};

const BRIDGE_SOCKET: &str = "muxlane-p3";
const WSL_ENV: &str = "/usr/bin/env";
const GATEWAY_EXECUTABLE: &str = "muxlaned";
const EVENT_NAME: &str = "phase3-terminal-frame";
const MAX_PENDING_REQUESTS: usize = 32;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

type Pending = Arc<Mutex<HashMap<u64, Sender<Result<Phase3Response, Phase3Error>>>>>;

pub struct Phase3Bridge {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    next_id: AtomicU64,
    pending: Pending,
    alive: Arc<AtomicBool>,
}

impl Phase3Bridge {
    fn start(app: AppHandle) -> Result<Self, Phase3Error> {
        let mut child = Command::new("wsl.exe")
            .args([
                "--exec",
                WSL_ENV,
                GATEWAY_EXECUTABLE,
                "phase3",
                "gateway",
                "--socket",
                BRIDGE_SOCKET,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| bridge_error("cannot start the fixed WSL terminal gateway"))?;
        let stdin =
            child.stdin.take().ok_or_else(|| bridge_error("WSL gateway stdin unavailable"))?;
        let stdout =
            child.stdout.take().ok_or_else(|| bridge_error("WSL gateway stdout unavailable"))?;
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        forward_frames(app, stdout, Arc::clone(&pending), Arc::clone(&alive));
        Ok(Self {
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            next_id: AtomicU64::new(1),
            pending,
            alive,
        })
    }

    fn request(&self, request: Phase3Request) -> Result<Phase3Response, Phase3Error> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| bridge_error("WSL response registry unavailable"))?;
            if pending.len() >= MAX_PENDING_REQUESTS {
                return Err(bridge_error("too many pending terminal control requests"));
            }
            pending.insert(id, sender);
        }
        let line = serde_json::to_string(&Phase3RequestEnvelope { id, request })
            .map_err(|_| bridge_error("cannot encode WSL terminal request"))?;
        let write_result =
            self.stdin.lock().map_err(|_| bridge_error("WSL gateway stdin unavailable")).and_then(
                |mut stdin| {
                    writeln!(stdin, "{line}")
                        .and_then(|()| stdin.flush())
                        .map_err(|_| bridge_error("WSL gateway input disconnected"))
                },
            );
        if let Err(error) = write_result {
            self.remove_pending(id);
            return Err(error);
        }
        receiver.recv_timeout(RESPONSE_TIMEOUT).map_err(|_| {
            self.remove_pending(id);
            bridge_error("WSL terminal request timed out")
        })?
    }

    fn remove_pending(&self, id: u64) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&id);
        }
    }
}

impl Drop for Phase3Bridge {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub struct Phase3State {
    bridge: Mutex<Option<Phase3Bridge>>,
}

impl Phase3State {
    pub fn new() -> Self {
        Self { bridge: Mutex::new(None) }
    }

    fn request(&self, app: AppHandle, request: Phase3Request) -> Result<Phase3Response, String> {
        let mut bridge =
            self.bridge.lock().map_err(|_| "Phase 3 bridge state unavailable".to_owned())?;
        if bridge.as_ref().is_some_and(|bridge| !bridge.alive.load(Ordering::Acquire)) {
            *bridge = None;
        }
        if bridge.is_none() {
            *bridge = Some(Phase3Bridge::start(app).map_err(display_error)?);
        }
        bridge
            .as_ref()
            .ok_or_else(|| "Phase 3 bridge not initialized".to_owned())?
            .request(request)
            .map_err(display_error)
    }
}

fn forward_frames(
    app: AppHandle,
    stdout: impl std::io::Read + Send + 'static,
    pending: Pending,
    alive: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(frame) = serde_json::from_str::<Phase3Frame>(&line) else {
                continue;
            };
            match frame {
                Phase3Frame::Response { id, result } => {
                    if let Ok(mut pending) = pending.lock() {
                        if let Some(sender) = pending.remove(&id) {
                            let result = match result {
                                ResultFrame::Ok { response } => Ok(response),
                                ResultFrame::Error { error } => Err(error),
                            };
                            let _ = sender.send(result);
                        }
                    }
                }
                Phase3Frame::Event { event } => {
                    let _ = app.emit(EVENT_NAME, event);
                }
            }
        }
        alive.store(false, Ordering::Release);
        if let Ok(mut pending) = pending.lock() {
            for (_, sender) in pending.drain() {
                let _ = sender.send(Err(bridge_error("WSL terminal gateway disconnected")));
            }
        }
    });
}

#[tauri::command]
pub fn phase3_probe(app: AppHandle, state: State<'_, Phase3State>) -> Result<String, String> {
    match state.request(app, Phase3Request::Probe)? {
        Phase3Response::Probe { tmux_version, .. } => Ok(tmux_version),
        _ => Err("Phase 3 bridge returned an unexpected probe response".to_owned()),
    }
}

#[tauri::command]
pub fn phase3_list_sessions(
    app: AppHandle,
    state: State<'_, Phase3State>,
) -> Result<Vec<ManagedSession>, String> {
    match state.request(app, Phase3Request::ListManagedSessions)? {
        Phase3Response::Sessions { sessions } => Ok(sessions),
        _ => Err("Phase 3 bridge returned an unexpected sessions response".to_owned()),
    }
}

#[tauri::command]
pub fn phase3_create_synthetic_session(
    app: AppHandle,
    state: State<'_, Phase3State>,
    project_id: String,
) -> Result<(), String> {
    acknowledged(state.request(app, Phase3Request::CreateSyntheticSession { project_id })?)
}

#[tauri::command]
pub fn phase3_list_windows(
    app: AppHandle,
    state: State<'_, Phase3State>,
    project_id: String,
) -> Result<Vec<ManagedWindow>, String> {
    match state.request(app, Phase3Request::ListWindows { project_id })? {
        Phase3Response::Windows { windows } => Ok(windows),
        _ => Err("Phase 3 bridge returned an unexpected windows response".to_owned()),
    }
}

#[tauri::command]
pub fn phase3_create_window(
    app: AppHandle,
    state: State<'_, Phase3State>,
    project_id: String,
    name: String,
) -> Result<(), String> {
    acknowledged(state.request(app, Phase3Request::CreateWindow { project_id, name })?)
}

#[tauri::command]
pub fn phase3_attach(
    app: AppHandle,
    state: State<'_, Phase3State>,
    project_id: String,
    window_id: String,
) -> Result<AttachedTerminal, String> {
    match state.request(app, Phase3Request::Attach { project_id, window_id })? {
        Phase3Response::Attached { stream } => Ok(stream),
        _ => Err("Phase 3 bridge returned an unexpected attach response".to_owned()),
    }
}

#[tauri::command]
pub fn phase3_start_stream(
    app: AppHandle,
    state: State<'_, Phase3State>,
    stream: AttachedTerminal,
) -> Result<(), String> {
    match state.request(app, Phase3Request::StartStream { stream })? {
        Phase3Response::StreamStarted { .. } => Ok(()),
        _ => Err("Phase 3 bridge returned an unexpected stream-start response".to_owned()),
    }
}

#[tauri::command]
pub fn phase3_detach(
    app: AppHandle,
    state: State<'_, Phase3State>,
    stream: AttachedTerminal,
) -> Result<(), String> {
    match state.request(app, Phase3Request::Detach { stream })? {
        Phase3Response::Detached => Ok(()),
        _ => Err("Phase 3 bridge returned an unexpected detach response".to_owned()),
    }
}

#[tauri::command]
pub fn phase3_send_input(
    app: AppHandle,
    state: State<'_, Phase3State>,
    stream: AttachedTerminal,
    bytes: Vec<u8>,
) -> Result<(), String> {
    acknowledged(state.request(app, Phase3Request::SendInput { stream, bytes })?)
}

#[tauri::command]
pub fn phase3_resize(
    app: AppHandle,
    state: State<'_, Phase3State>,
    stream: AttachedTerminal,
    columns: u16,
    rows: u16,
) -> Result<(), String> {
    acknowledged(state.request(app, Phase3Request::Resize { stream, columns, rows })?)
}

#[tauri::command]
pub fn phase3_close_window(
    app: AppHandle,
    state: State<'_, Phase3State>,
    project_id: String,
    window_id: String,
) -> Result<(), String> {
    acknowledged(state.request(app, Phase3Request::CloseWindow { project_id, window_id })?)
}

#[tauri::command]
pub fn phase3_cleanup_session(
    app: AppHandle,
    state: State<'_, Phase3State>,
    project_id: String,
) -> Result<(), String> {
    acknowledged(state.request(app, Phase3Request::CleanupSession { project_id })?)
}

fn acknowledged(response: Phase3Response) -> Result<(), String> {
    if matches!(response, Phase3Response::Acknowledged) {
        Ok(())
    } else {
        Err("Phase 3 bridge returned an unexpected acknowledgement".to_owned())
    }
}

fn bridge_error(message: &str) -> Phase3Error {
    Phase3Error { code: "bridge".to_owned(), message: message.to_owned() }
}

fn display_error(error: Phase3Error) -> String {
    format!("{}: {}", error.code, error.message)
}

#[cfg(test)]
mod tests {
    use super::{BRIDGE_SOCKET, EVENT_NAME, GATEWAY_EXECUTABLE, MAX_PENDING_REQUESTS, WSL_ENV};

    #[test]
    fn bridge_contract_is_fixed_and_bounded() {
        assert_eq!(BRIDGE_SOCKET, "muxlane-p3");
        assert_eq!(WSL_ENV, "/usr/bin/env");
        assert_eq!(GATEWAY_EXECUTABLE, "muxlaned");
        assert_eq!(EVENT_NAME, "phase3-terminal-frame");
        assert_eq!(MAX_PENDING_REQUESTS, 32);
    }
}
