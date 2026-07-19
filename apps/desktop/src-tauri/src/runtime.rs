//! Minimal Windows host adapters for the formal WSL control and Terminal planes.
//! No command accepts an executable, shell text, filesystem path, or tmux target.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    time::Duration,
};

use muxlane_protocol::{
    TERMINAL_DATA_PROTOCOL_MAJOR, TERMINAL_DATA_PROTOCOL_MINOR, TerminalDataError,
    TerminalDataFrame, TerminalDataRequest, TerminalDataRequestEnvelope, TerminalDataResponse,
    TerminalDataResult, TerminalStream,
};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

const EVENT_NAME: &str = "muxlane-terminal-frame";
const MAX_PENDING: usize = 32;
const MAX_CLI_OUTPUT: u64 = 1024 * 1024;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

type Pending = Arc<Mutex<HashMap<u64, Sender<Result<TerminalDataResponse, TerminalDataError>>>>>;

struct Bridge {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    next_id: AtomicU64,
    pending: Pending,
    alive: Arc<AtomicBool>,
}

impl Bridge {
    fn start(app: AppHandle) -> Result<Self, String> {
        let mut child = Command::new("wsl.exe")
            .args(["--exec", "/usr/bin/env", "muxlaned", "terminal-gateway"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "cannot start the fixed WSL Terminal Gateway".to_owned())?;
        let stdin =
            child.stdin.take().ok_or_else(|| "WSL Terminal input unavailable".to_owned())?;
        let stdout =
            child.stdout.take().ok_or_else(|| "WSL Terminal output unavailable".to_owned())?;
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        forward(app, stdout, Arc::clone(&pending), Arc::clone(&alive));
        Ok(Self {
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            next_id: AtomicU64::new(1),
            pending,
            alive,
        })
    }

    fn request(&self, request: TerminalDataRequest) -> Result<TerminalDataResponse, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| "Terminal response registry unavailable".to_owned())?;
            if pending.len() >= MAX_PENDING {
                return Err("too many pending Terminal requests".to_owned());
            }
            pending.insert(id, sender);
        }
        let encoded = serde_json::to_string(&TerminalDataRequestEnvelope { id, request })
            .map_err(|_| "cannot encode Terminal request".to_owned())?;
        let write_result = {
            let mut stdin =
                self.stdin.lock().map_err(|_| "Terminal input unavailable".to_owned())?;
            stdin.write_all(format!("{encoded}\n").as_bytes()).and_then(|()| stdin.flush())
        };
        if write_result.is_err() {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&id);
            }
            return Err("WSL Terminal input disconnected".to_owned());
        }
        receiver
            .recv_timeout(RESPONSE_TIMEOUT)
            .map_err(|_| "WSL Terminal request timed out".to_owned())?
            .map_err(|error| format!("{}: {}", error.code, error.message))
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub struct RuntimeState {
    bridge: Mutex<Option<Bridge>>,
}
impl RuntimeState {
    pub fn new() -> Self {
        Self { bridge: Mutex::new(None) }
    }
    fn request(
        &self,
        app: AppHandle,
        request: TerminalDataRequest,
    ) -> Result<TerminalDataResponse, String> {
        let mut bridge =
            self.bridge.lock().map_err(|_| "Runtime bridge state unavailable".to_owned())?;
        if bridge.as_ref().is_some_and(|value| !value.alive.load(Ordering::Acquire)) {
            *bridge = None;
        }
        if bridge.is_none() {
            let created = Bridge::start(app)?;
            created.request(TerminalDataRequest::Handshake {
                protocol_major: TERMINAL_DATA_PROTOCOL_MAJOR,
                protocol_minor: TERMINAL_DATA_PROTOCOL_MINOR,
                client_name: "muxlane_windows_desktop".to_owned(),
            })?;
            *bridge = Some(created);
        }
        bridge.as_ref().ok_or_else(|| "Runtime bridge unavailable".to_owned())?.request(request)
    }
}

fn forward(
    app: AppHandle,
    stdout: impl Read + Send + 'static,
    pending: Pending,
    alive: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(frame) = serde_json::from_str::<TerminalDataFrame>(&line) else { continue };
            match frame {
                TerminalDataFrame::Response { id, result } => {
                    if let Ok(mut pending) = pending.lock()
                        && let Some(sender) = pending.remove(&id)
                    {
                        let result = match result {
                            TerminalDataResult::Ok { response } => Ok(response),
                            TerminalDataResult::Error { error } => Err(error),
                        };
                        let _ = sender.send(result);
                    }
                }
                TerminalDataFrame::Event { event } => {
                    let _ = app.emit(EVENT_NAME, event);
                }
            }
        }
        alive.store(false, Ordering::Release);
        if let Ok(mut pending) = pending.lock() {
            for (_, sender) in pending.drain() {
                let _ = sender.send(Err(TerminalDataError {
                    code: "DAEMON_UNAVAILABLE".to_owned(),
                    message: "WSL Terminal Gateway disconnected".to_owned(),
                }));
            }
        }
    });
}

fn fixed_cli(arguments: &[&str]) -> Result<Value, String> {
    let mut child = Command::new("wsl.exe")
        .arg("--exec")
        .arg("/usr/bin/env")
        .arg("muxlane")
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "cannot start fixed muxlane CLI command".to_owned())?;
    let mut bytes = Vec::new();
    child
        .stdout
        .take()
        .ok_or_else(|| "muxlane CLI output unavailable".to_owned())?
        .take(MAX_CLI_OUTPUT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "muxlane CLI output failed".to_owned())?;
    let status = child.wait().map_err(|_| "muxlane CLI wait failed".to_owned())?;
    if bytes.len() as u64 > MAX_CLI_OUTPUT {
        return Err("muxlane CLI output exceeded limit".to_owned());
    }
    let value = serde_json::from_slice(&bytes)
        .map_err(|_| "muxlane CLI returned invalid JSON".to_owned())?;
    if !status.success() {
        return Err("muxlane CLI command failed".to_owned());
    }
    Ok(value)
}

#[tauri::command]
pub fn runtime_doctor() -> Result<Value, String> {
    fixed_cli(&["doctor"])
}
#[tauri::command]
pub fn runtime_status() -> Result<Value, String> {
    fixed_cli(&["status"])
}
#[tauri::command]
pub fn runtime_daemon_start() -> Result<Value, String> {
    fixed_cli(&["daemon", "start"])
}
#[tauri::command]
pub fn runtime_daemon_stop() -> Result<Value, String> {
    fixed_cli(&["daemon", "stop"])
}

#[tauri::command]
pub fn runtime_terminal_attach(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    terminal_id: String,
) -> Result<TerminalStream, String> {
    match state.request(app, TerminalDataRequest::Attach { terminal_id })? {
        TerminalDataResponse::Attached { stream } => Ok(stream),
        _ => Err("unexpected Terminal attach response".to_owned()),
    }
}
#[tauri::command]
pub fn runtime_terminal_start(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    stream: TerminalStream,
) -> Result<(), String> {
    state.request(app, TerminalDataRequest::StartStream { stream }).map(|_| ())
}
#[tauri::command]
pub fn runtime_terminal_detach(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    stream: TerminalStream,
) -> Result<(), String> {
    state.request(app, TerminalDataRequest::Detach { stream }).map(|_| ())
}
#[tauri::command]
pub fn runtime_terminal_switch(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    terminal_id: String,
) -> Result<TerminalStream, String> {
    match state.request(app, TerminalDataRequest::Switch { terminal_id })? {
        TerminalDataResponse::Attached { stream } => Ok(stream),
        _ => Err("unexpected Terminal switch response".to_owned()),
    }
}
#[tauri::command]
pub fn runtime_terminal_input(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    stream: TerminalStream,
    bytes: Vec<u8>,
) -> Result<(), String> {
    state.request(app, TerminalDataRequest::SendInput { stream, bytes }).map(|_| ())
}
#[tauri::command]
pub fn runtime_terminal_resize(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    stream: TerminalStream,
    columns: u16,
    rows: u16,
) -> Result<(), String> {
    state.request(app, TerminalDataRequest::Resize { stream, columns, rows }).map(|_| ())
}
#[tauri::command]
pub fn runtime_terminal_close(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    terminal_id: String,
) -> Result<(), String> {
    state.request(app, TerminalDataRequest::Close { terminal_id }).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_surface_uses_fixed_executables_and_bounded_messages() {
        assert_eq!(EVENT_NAME, "muxlane-terminal-frame");
        assert_eq!(MAX_PENDING, 32);
        assert!(MAX_CLI_OUTPUT <= 1024 * 1024);
    }
}
