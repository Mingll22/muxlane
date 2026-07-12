//! Non-production Phase 3 terminal gateway.
//!
//! This module owns a deliberately small, stdio-only bridge to a dedicated tmux
//! socket. It is not a daemon, persistent Runtime, or stable production protocol.

use std::{
    io::{self, BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use muxlane_protocol::{
    AttachedTerminal, ManagedSession, ManagedWindow, Phase3Error, Phase3Event, Phase3Frame,
    Phase3Request, Phase3RequestEnvelope, Phase3Response, ResultFrame,
};

const MAX_INPUT_BYTES: usize = 16 * 1024;
const MIN_COLUMNS: u16 = 20;
const MAX_COLUMNS: u16 = 320;
const MIN_ROWS: u16 = 5;
const MAX_ROWS: u16 = 160;
const HISTORY_LINES: i32 = 300;
const MANAGED_PREFIX: &str = "mlp3-";

type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

#[derive(Debug)]
pub struct Gateway {
    socket: String,
    attached: Option<Attachment>,
}

#[derive(Debug)]
struct Attachment {
    terminal: AttachedTerminal,
    control_stdin: ChildStdin,
    intentionally_detached: Arc<AtomicBool>,
    _child: Child,
}

impl Gateway {
    pub fn new(socket: String) -> Result<Self, Phase3Error> {
        validate_socket(&socket)?;
        Ok(Self { socket, attached: None })
    }

    pub fn handle(
        &mut self,
        request: Phase3Request,
        output: &SharedWriter,
    ) -> Result<Phase3Response, Phase3Error> {
        match request {
            Phase3Request::Probe => {
                Ok(Phase3Response::Probe { tmux_version: self.tmux_version()? })
            }
            Phase3Request::CreateSyntheticSession { project_id } => {
                let session_name = session_name(&project_id)?;
                self.create_synthetic_session(&session_name)?;
                Ok(Phase3Response::Acknowledged)
            }
            Phase3Request::ListManagedSessions => {
                Ok(Phase3Response::Sessions { sessions: self.list_sessions()? })
            }
            Phase3Request::CreateWindow { project_id, name } => {
                let session_name = session_name(&project_id)?;
                validate_window_name(&name)?;
                self.ensure_session_exists(&session_name)?;
                self.tmux_status(["new-window", "-d", "-t", &session_name, "-n", &name])?;
                Ok(Phase3Response::Acknowledged)
            }
            Phase3Request::ListWindows { project_id } => {
                Ok(Phase3Response::Windows { windows: self.list_windows(&project_id)? })
            }
            Phase3Request::Attach { project_id, window_id } => {
                self.attach(project_id, window_id, output)?;
                let terminal = self
                    .attached
                    .as_ref()
                    .map(|attachment| attachment.terminal.clone())
                    .ok_or_else(|| internal_error("attachment was not retained"))?;
                Ok(Phase3Response::Attached {
                    project_id: terminal.project_id,
                    window_id: terminal.window_id,
                })
            }
            Phase3Request::Detach => {
                self.detach()?;
                Ok(Phase3Response::Detached)
            }
            Phase3Request::SendInput { bytes } => {
                self.send_input(&bytes)?;
                Ok(Phase3Response::Acknowledged)
            }
            Phase3Request::Resize { columns, rows } => {
                self.resize(columns, rows)?;
                Ok(Phase3Response::Acknowledged)
            }
            Phase3Request::CloseWindow { project_id, window_id } => {
                let target = self.target_for(&project_id, &window_id)?;
                self.tmux_status(["kill-window", "-t", &target])?;
                if self.attached.as_ref().is_some_and(|attachment| {
                    attachment.terminal.project_id == project_id
                        && attachment.terminal.window_id == window_id
                }) {
                    self.detach()?;
                }
                Ok(Phase3Response::Acknowledged)
            }
            Phase3Request::CleanupSession { project_id } => {
                let name = session_name(&project_id)?;
                if self
                    .attached
                    .as_ref()
                    .is_some_and(|attachment| attachment.terminal.project_id == project_id)
                {
                    self.detach()?;
                }
                self.tmux_status(["kill-session", "-t", &name])?;
                Ok(Phase3Response::Acknowledged)
            }
            Phase3Request::ReadState => Ok(Phase3Response::State {
                attached: self.attached.as_ref().map(|attachment| attachment.terminal.clone()),
            }),
        }
    }

    fn tmux_version(&self) -> Result<String, Phase3Error> {
        let output = Command::new("tmux")
            .arg("-V")
            .output()
            .map_err(|_| unavailable_error("tmux is not executable"))?;
        if !output.status.success() {
            return Err(unavailable_error("tmux version probe failed"));
        }
        let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if version.starts_with("tmux ") {
            Ok(version)
        } else {
            Err(unavailable_error("tmux reported an invalid version"))
        }
    }

    fn create_synthetic_session(&self, name: &str) -> Result<(), Phase3Error> {
        if self.session_exists(name)? {
            return Err(conflict_error("managed session already exists"));
        }
        let executable = std::env::current_exe()
            .map_err(|_| internal_error("cannot locate the POC runner executable"))?;
        let executable = executable.to_string_lossy().into_owned();
        self.tmux_status([
            "new-session",
            "-d",
            "-s",
            name,
            "-x",
            "100",
            "-y",
            "32",
            &executable,
            "phase3",
            "synthetic-runner",
        ])
    }

    fn list_sessions(&self) -> Result<Vec<ManagedSession>, Phase3Error> {
        let result = Command::new("tmux")
            .args(["-L", &self.socket, "list-sessions", "-F", "#{session_name}\t#{session_id}"])
            .stdin(Stdio::null())
            .output()
            .map_err(|_| unavailable_error("tmux is not executable"))?;
        if !result.status.success() {
            return Ok(Vec::new());
        }
        let output = String::from_utf8_lossy(&result.stdout);
        let sessions = output
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .filter_map(|(name, id)| {
                name.strip_prefix(MANAGED_PREFIX).and_then(|project_id| {
                    validate_project_id(project_id).ok().map(|()| ManagedSession {
                        project_id: project_id.to_owned(),
                        session_name: name.to_owned(),
                        session_id: id.to_owned(),
                    })
                })
            })
            .collect();
        Ok(sessions)
    }

    fn list_windows(&self, project_id: &str) -> Result<Vec<ManagedWindow>, Phase3Error> {
        let name = session_name(project_id)?;
        self.ensure_session_exists(&name)?;
        let output = self.tmux_output([
            "list-windows",
            "-t",
            &name,
            "-F",
            "#{window_id}\t#{window_name}\t#{window_active}",
        ])?;
        output
            .lines()
            .map(|line| {
                let mut fields = line.split('\t');
                let id = fields.next().ok_or_else(|| internal_error("tmux window id missing"))?;
                let name =
                    fields.next().ok_or_else(|| internal_error("tmux window name missing"))?;
                let active =
                    fields.next().ok_or_else(|| internal_error("tmux window activity missing"))?;
                validate_window_id(id)?;
                Ok(ManagedWindow {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    active: active == "1",
                })
            })
            .collect()
    }

    fn attach(
        &mut self,
        project_id: String,
        window_id: String,
        output: &SharedWriter,
    ) -> Result<(), Phase3Error> {
        self.detach()?;
        let target = self.target_for(&project_id, &window_id)?;
        let history = self.tmux_output([
            "capture-pane",
            "-p",
            "-e",
            "-J",
            "-S",
            &format!("-{HISTORY_LINES}"),
            "-t",
            &target,
        ])?;
        emit_event(output, Phase3Event::History { bytes: history.into_bytes() })?;

        let mut child = Command::new("tmux")
            .args(["-L", &self.socket, "-C", "attach-session", "-t", &target])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| unavailable_error("cannot start tmux Control Mode"))?;
        let control_stdin =
            child.stdin.take().ok_or_else(|| internal_error("Control Mode stdin unavailable"))?;
        let control_stdout =
            child.stdout.take().ok_or_else(|| internal_error("Control Mode stdout unavailable"))?;
        let intentionally_detached = Arc::new(AtomicBool::new(false));
        let event_output = Arc::clone(output);
        let event_detach_flag = Arc::clone(&intentionally_detached);
        thread::spawn(move || {
            forward_control_output(control_stdout, event_output, event_detach_flag)
        });

        self.attached = Some(Attachment {
            terminal: AttachedTerminal { project_id, window_id },
            control_stdin,
            intentionally_detached,
            _child: child,
        });
        Ok(())
    }

    fn detach(&mut self) -> Result<(), Phase3Error> {
        if let Some(mut attachment) = self.attached.take() {
            attachment.intentionally_detached.store(true, Ordering::Release);
            let _ = attachment.control_stdin.write_all(b"detach-client\n");
            let _ = attachment.control_stdin.flush();
            let _ = attachment._child.kill();
            let _ = attachment._child.wait();
        }
        Ok(())
    }

    fn send_input(&mut self, bytes: &[u8]) -> Result<(), Phase3Error> {
        if bytes.is_empty() || bytes.len() > MAX_INPUT_BYTES {
            return Err(validation_error("input frame must contain 1..=16384 bytes"));
        }
        let attachment =
            self.attached.as_mut().ok_or_else(|| state_error("no terminal is attached"))?;
        let target =
            target_for_ids(&attachment.terminal.project_id, &attachment.terminal.window_id)?;
        for byte in bytes {
            writeln!(attachment.control_stdin, "send-keys -t {target} -H {byte:02x}")
                .map_err(|_| state_error("Control Mode input channel disconnected"))?;
        }
        attachment
            .control_stdin
            .flush()
            .map_err(|_| state_error("Control Mode input channel disconnected"))
    }

    fn resize(&self, columns: u16, rows: u16) -> Result<(), Phase3Error> {
        if !(MIN_COLUMNS..=MAX_COLUMNS).contains(&columns) || !(MIN_ROWS..=MAX_ROWS).contains(&rows)
        {
            return Err(validation_error("resize is outside the POC bounds"));
        }
        let attachment =
            self.attached.as_ref().ok_or_else(|| state_error("no terminal is attached"))?;
        let target =
            target_for_ids(&attachment.terminal.project_id, &attachment.terminal.window_id)?;
        self.tmux_status([
            "resize-window",
            "-t",
            &target,
            "-x",
            &columns.to_string(),
            "-y",
            &rows.to_string(),
        ])
    }

    fn target_for(&self, project_id: &str, window_id: &str) -> Result<String, Phase3Error> {
        let windows = self.list_windows(project_id)?;
        if windows.iter().any(|window| window.id == window_id) {
            target_for_ids(project_id, window_id)
        } else {
            Err(not_found_error("managed window does not exist"))
        }
    }

    fn ensure_session_exists(&self, name: &str) -> Result<(), Phase3Error> {
        if self.session_exists(name)? {
            Ok(())
        } else {
            Err(not_found_error("managed session does not exist"))
        }
    }

    fn session_exists(&self, name: &str) -> Result<bool, Phase3Error> {
        let status = Command::new("tmux")
            .args(["-L", &self.socket, "has-session", "-t", name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| unavailable_error("tmux is not executable"))?;
        Ok(status.success())
    }

    fn tmux_status<const N: usize>(&self, args: [&str; N]) -> Result<(), Phase3Error> {
        let status = Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| unavailable_error("tmux is not executable"))?;
        if status.success() { Ok(()) } else { Err(state_error("tmux command did not complete")) }
    }

    fn tmux_output<const N: usize>(&self, args: [&str; N]) -> Result<String, Phase3Error> {
        let output = Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|_| unavailable_error("tmux is not executable"))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(state_error("tmux query did not complete"))
        }
    }
}

pub fn run_gateway(socket: String) -> Result<(), Phase3Error> {
    let mut gateway = Gateway::new(socket)?;
    let output: SharedWriter = Arc::new(Mutex::new(Box::new(io::stdout())));
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|_| state_error("POC bridge input disconnected"))?;
        if line.len() > 128 * 1024 {
            return Err(validation_error("POC control frame exceeds 128 KiB"));
        }
        let envelope: Phase3RequestEnvelope = serde_json::from_str(&line)
            .map_err(|_| validation_error("invalid typed POC control frame"))?;
        let result = gateway.handle(envelope.request, &output);
        emit_response(&output, envelope.id, result)?;
    }
    gateway.detach()?;
    Ok(())
}

pub fn run_synthetic_runner() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let (input_sender, input_receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut byte = [0_u8; 1];
        while stdin.read(&mut byte).ok() == Some(1) {
            if input_sender.send(()).is_err() {
                return;
            }
        }
    });
    let mut tick = 0_u64;
    loop {
        writeln!(stdout, "\x1b[36mphase3\x1b[0m tick={tick:04} 中文 表 😀")?;
        stdout.flush()?;
        tick = tick.saturating_add(1);
        while input_receiver.try_recv().is_ok() {
            writeln!(stdout, "\x1b[32mINPUT_RECEIVED\x1b[0m")?;
            stdout.flush()?;
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn forward_control_output(
    control_stdout: impl io::Read,
    output: SharedWriter,
    intentionally_detached: Arc<AtomicBool>,
) {
    let reader = BufReader::new(control_stdout);
    for line in reader.lines().map_while(Result::ok) {
        if let Some(bytes) = line.strip_prefix("%output ").and_then(parse_control_output) {
            let _ = emit_event(&output, Phase3Event::Output { bytes });
        }
        if line == "%exit" {
            if !intentionally_detached.load(Ordering::Acquire) {
                let _ = emit_event(&output, Phase3Event::StreamClosed);
            }
            return;
        }
    }
    if !intentionally_detached.load(Ordering::Acquire) {
        let _ = emit_event(
            &output,
            Phase3Event::StreamError { code: "control_mode_disconnected".to_owned() },
        );
    }
}

fn parse_control_output(value: &str) -> Option<Vec<u8>> {
    let (_, encoded) = value.split_once(' ')?;
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut characters = encoded.as_bytes().iter().copied().peekable();
    while let Some(byte) = characters.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        let mut octal = [0_u8; 3];
        for digit in &mut octal {
            *digit = characters.next()?;
            if !(*digit >= b'0' && *digit <= b'7') {
                return None;
            }
        }
        bytes.push((octal[0] - b'0') * 64 + (octal[1] - b'0') * 8 + (octal[2] - b'0'));
    }
    Some(bytes)
}

fn emit_response(
    output: &SharedWriter,
    id: u64,
    result: Result<Phase3Response, Phase3Error>,
) -> Result<(), Phase3Error> {
    let result = match result {
        Ok(response) => ResultFrame::Ok { response },
        Err(error) => ResultFrame::Error { error },
    };
    write_frame(output, &Phase3Frame::Response { id, result })
}

fn emit_event(output: &SharedWriter, event: Phase3Event) -> Result<(), Phase3Error> {
    write_frame(output, &Phase3Frame::Event { event })
}

fn write_frame(output: &SharedWriter, frame: &Phase3Frame) -> Result<(), Phase3Error> {
    let encoded =
        serde_json::to_string(frame).map_err(|_| internal_error("cannot encode POC frame"))?;
    let mut output = output.lock().map_err(|_| internal_error("POC output lock poisoned"))?;
    writeln!(output, "{encoded}").map_err(|_| state_error("POC bridge output disconnected"))?;
    output.flush().map_err(|_| state_error("POC bridge output disconnected"))
}

fn session_name(project_id: &str) -> Result<String, Phase3Error> {
    validate_project_id(project_id)?;
    Ok(format!("{MANAGED_PREFIX}{project_id}"))
}

fn target_for_ids(project_id: &str, window_id: &str) -> Result<String, Phase3Error> {
    let session = session_name(project_id)?;
    validate_window_id(window_id)?;
    Ok(format!("{session}:{window_id}"))
}

fn validate_project_id(value: &str) -> Result<(), Phase3Error> {
    validate_slug(value, 24, "project id")
}

fn validate_socket(value: &str) -> Result<(), Phase3Error> {
    validate_slug(value, 48, "tmux socket")
}

fn validate_window_name(value: &str) -> Result<(), Phase3Error> {
    validate_slug(value, 24, "window name")
}

fn validate_slug(value: &str, maximum: usize, label: &str) -> Result<(), Phase3Error> {
    let valid = !value.is_empty()
        && value.len() <= maximum
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if valid { Ok(()) } else { Err(validation_error(&format!("invalid {label}"))) }
}

fn validate_window_id(value: &str) -> Result<(), Phase3Error> {
    if value.len() >= 2
        && value.starts_with('@')
        && value[1..].bytes().all(|byte| byte.is_ascii_digit())
    {
        Ok(())
    } else {
        Err(validation_error("invalid managed window id"))
    }
}

fn validation_error(message: &str) -> Phase3Error {
    error("validation", message)
}

fn unavailable_error(message: &str) -> Phase3Error {
    error("unavailable", message)
}

fn not_found_error(message: &str) -> Phase3Error {
    error("not_found", message)
}

fn conflict_error(message: &str) -> Phase3Error {
    error("conflict", message)
}

fn state_error(message: &str) -> Phase3Error {
    error("state", message)
}

fn internal_error(message: &str) -> Phase3Error {
    error("internal", message)
}

fn error(code: &str, message: &str) -> Phase3Error {
    Phase3Error { code: code.to_owned(), message: message.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_control_output, target_for_ids, validate_project_id, validate_socket,
        validate_window_id,
    };

    #[test]
    fn rejects_target_injection_and_invalid_identifiers() {
        for invalid in ["", "A", "project_a", "project;id", "project$(id)", "project name"] {
            assert!(validate_project_id(invalid).is_err(), "{invalid} must be rejected");
        }
        assert!(validate_socket("muxlane-p3-test-1").is_ok());
        assert!(validate_socket("../default").is_err());
        assert!(validate_window_id("@17").is_ok());
        assert!(validate_window_id("@17;kill-server").is_err());
        assert!(target_for_ids("project-a", "@17").is_ok());
    }

    #[test]
    fn decodes_tmux_control_mode_output_without_utf8_assumptions() {
        assert_eq!(
            parse_control_output("%0 hello\\015\\012中文 😀"),
            Some("hello\r\n中文 😀".as_bytes().to_vec())
        );
        assert_eq!(parse_control_output("%0 \\999"), None);
    }
}
