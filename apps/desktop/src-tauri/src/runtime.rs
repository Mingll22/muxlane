//! Minimal Windows host adapters for the formal WSL control and Terminal planes.
//! No command accepts an executable, shell text, filesystem path, or tmux target.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    time::{Duration, Instant},
};

use muxlane_protocol::{
    CAPABILITIES, ControlRequest, ControlResponse, HandshakeRequest, PROTOCOL_MAJOR,
    PROTOCOL_MINOR, TERMINAL_DATA_PROTOCOL_MAJOR, TERMINAL_DATA_PROTOCOL_MINOR, TerminalDataError,
    TerminalDataFrame, TerminalDataRequest, TerminalDataRequestEnvelope, TerminalDataResponse,
    TerminalDataResult, TerminalStream,
};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

const EVENT_NAME: &str = "muxlane-terminal-frame";
const MAX_PENDING: usize = 32;
const MAX_CLI_OUTPUT: u64 = 1024 * 1024;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(60);

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

fn run_bounded(command: &mut Command, timeout: Duration) -> Result<(ExitStatus, Vec<u8>), String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "cannot start fixed WSL command".to_owned())?;
    let stdout = child.stdout.take().ok_or_else(|| "WSL command output unavailable".to_owned())?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(MAX_CLI_OUTPUT + 1).read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|_| "WSL command wait failed".to_owned())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("fixed WSL command timed out".to_owned());
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let bytes = reader
        .join()
        .map_err(|_| "WSL command output reader failed".to_owned())?
        .map_err(|_| "WSL command output failed".to_owned())?;
    Ok((status, bytes))
}

fn fixed_cli(arguments: &[&str]) -> Result<Value, String> {
    let mut command = Command::new("wsl.exe");
    command.arg("--exec").arg("/usr/bin/env").arg("muxlane").args(arguments);
    let (status, bytes) = run_bounded(&mut command, CONTROL_TIMEOUT)?;
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

fn fixed_control(request: &ControlRequest) -> Result<Value, String> {
    let encoded = serde_json::to_string(request)
        .map_err(|_| "cannot encode fixed muxlane control request".to_owned())?;
    fixed_cli(&["control", &encoded])
}

#[derive(Debug, Serialize)]
pub struct EnvironmentCheck {
    key: &'static str,
    status: &'static str,
    version: Option<String>,
    suggestion: Option<&'static str>,
}

fn fixed_wsl_probe(key: &'static str, executable: &str, version_arg: &str) -> EnvironmentCheck {
    let mut command = Command::new("wsl.exe");
    command.args(["--exec", "/usr/bin/env", executable, version_arg]);
    let output = run_bounded(&mut command, PROBE_TIMEOUT);
    match output {
        Ok((status, stdout)) if status.success() => EnvironmentCheck {
            key,
            status: "ready",
            version: String::from_utf8(stdout)
                .ok()
                .map(|value| value.lines().next().unwrap_or_default().trim().to_owned())
                .filter(|value| !value.is_empty()),
            suggestion: None,
        },
        _ => EnvironmentCheck {
            key,
            status: "unavailable",
            version: None,
            suggestion: Some("请在默认 WSL 发行版安装并确认该组件可从 PATH 访问。"),
        },
    }
}

#[tauri::command]
pub async fn runtime_doctor() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| fixed_cli(&["doctor"]))
        .await
        .map_err(|_| "runtime doctor task failed".to_owned())?
}

#[tauri::command]
pub async fn runtime_environment_check() -> Vec<EnvironmentCheck> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut command = Command::new("wsl.exe");
        command.arg("--status");
        let wsl = match run_bounded(&mut command, PROBE_TIMEOUT) {
            Ok((status, _)) if status.success() => {
                EnvironmentCheck { key: "wsl", status: "ready", version: None, suggestion: None }
            }
            _ => EnvironmentCheck {
                key: "wsl",
                status: "unavailable",
                version: None,
                suggestion: Some("请启用 WSL2 并安装默认 Ubuntu 发行版。"),
            },
        };
        vec![
            EnvironmentCheck {
                key: "windows",
                status: if cfg!(target_os = "windows") { "ready" } else { "unsupported" },
                version: None,
                suggestion: if cfg!(target_os = "windows") {
                    None
                } else {
                    Some("Muxlane Desktop 的正式目标是 Windows 10/11 x64。")
                },
            },
            wsl,
            fixed_wsl_probe("codex", "codex", "--version"),
            fixed_wsl_probe("tmux", "tmux", "-V"),
            fixed_wsl_probe("muxlaned", "muxlaned", "--version"),
        ]
    })
    .await
    .unwrap_or_else(|_| {
        vec![EnvironmentCheck {
            key: "windows",
            status: "unavailable",
            version: None,
            suggestion: Some("环境检查任务异常结束，请重试或重启 Muxlane。"),
        }]
    })
}

#[tauri::command]
pub async fn runtime_handshake() -> Result<Value, String> {
    let request = ControlRequest::SystemHandshake(HandshakeRequest {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        client_name: "muxlane_windows_desktop".to_owned(),
        client_version: env!("CARGO_PKG_VERSION").to_owned(),
        requested_capabilities: CAPABILITIES.iter().map(|value| (*value).to_owned()).collect(),
    });
    tauri::async_runtime::spawn_blocking(move || fixed_control(&request))
        .await
        .map_err(|_| "runtime handshake task failed".to_owned())?
}

/// The IPC accepts only the closed `ControlRequest` enum. It cannot express an
/// executable, shell command, tmux target, or network destination beyond the
/// finite Protocol 1.0 methods compiled into this binary.
#[tauri::command]
pub async fn runtime_control(request: ControlRequest) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || fixed_control(&request))
        .await
        .map_err(|_| "runtime control task failed".to_owned())?
}

#[tauri::command]
pub async fn runtime_open_workspace_location(
    project_id: String,
    relative_path: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let value =
            fixed_control(&ControlRequest::WorkspaceLocation { project_id, relative_path })?;
        let response: ControlResponse = serde_json::from_value(
            value
                .get("result")
                .cloned()
                .ok_or_else(|| "muxlane CLI omitted workspace location".to_owned())?,
        )
        .map_err(|_| "muxlane CLI returned an invalid workspace location".to_owned())?;
        let ControlResponse::WorkspaceLocation(location) = response else {
            return Err("unexpected workspace location response".to_owned());
        };
        let windows_path = location
            .canonical_windows_path
            .ok_or_else(|| "workspace path has no Windows mapping".to_owned())?;
        let status = Command::new("explorer.exe")
            .arg(format!("/select,{windows_path}"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "Windows Explorer is unavailable".to_owned())?;
        if status.success() {
            Ok(())
        } else {
            Err("Windows Explorer rejected the workspace location".to_owned())
        }
    })
    .await
    .map_err(|_| "workspace location task failed".to_owned())?
}
#[tauri::command]
pub async fn runtime_status() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| fixed_cli(&["status"]))
        .await
        .map_err(|_| "runtime status task failed".to_owned())?
}
#[tauri::command]
pub async fn runtime_daemon_start() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| fixed_cli(&["daemon", "start"]))
        .await
        .map_err(|_| "daemon start task failed".to_owned())?
}
#[tauri::command]
pub async fn runtime_daemon_stop() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| fixed_cli(&["daemon", "stop"]))
        .await
        .map_err(|_| "daemon stop task failed".to_owned())?
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
    }
}
