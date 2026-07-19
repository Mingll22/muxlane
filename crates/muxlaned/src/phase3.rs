//! Non-production Phase 3 terminal gateway.
//!
//! This module owns a deliberately small, stdio-only bridge to a dedicated tmux
//! socket. It is not a daemon, persistent Runtime, or stable production protocol.

use std::{
    io::{self, BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, TrySendError},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
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
const CONTROL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const BOOTSTRAP_START_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BUFFERED_CONTROL_LINES: usize = 1024;
const MAX_BUFFERED_CONTROL_BYTES: usize = 2 * 1024 * 1024;
const MAX_CONTROL_LINE_BYTES: usize = 256 * 1024;
const MAX_OUTPUT_EVENT_BYTES: usize = 64 * 1024;
const OVERFLOW_NONE: u8 = 0;
const OVERFLOW_LINE_BYTES: u8 = 1;
const OVERFLOW_BUFFER_BYTES: u8 = 2;
const OVERFLOW_BUFFER_LINES: u8 = 3;

type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

#[derive(Debug)]
struct ControlLine {
    value: Vec<u8>,
    size: usize,
    queued_bytes: Arc<AtomicUsize>,
}

impl Drop for ControlLine {
    fn drop(&mut self) {
        self.queued_bytes.fetch_sub(self.size, Ordering::AcqRel);
    }
}

#[derive(Debug)]
pub struct Gateway {
    socket: String,
    connection_id: String,
    next_attachment_id: u64,
    attached: Option<Attachment>,
}

#[derive(Debug)]
struct Attachment {
    stream: AttachedTerminal,
    control_stdin: Arc<Mutex<ChildStdin>>,
    control_lines: Option<Receiver<ControlLine>>,
    history: Option<Vec<u8>>,
    bootstrap_started: Arc<AtomicBool>,
    bootstrap_expired: Arc<AtomicBool>,
    output_overflowed: Arc<AtomicU8>,
    intentionally_detached: Arc<AtomicBool>,
    _child: Child,
}

struct ChildCleanupGuard {
    child: Option<Child>,
}

impl ChildCleanupGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child_mut(&mut self) -> Result<&mut Child, Phase3Error> {
        self.child.as_mut().ok_or_else(|| internal_error("Control Mode child guard is empty"))
    }

    fn disarm(mut self) -> Result<Child, Phase3Error> {
        self.child.take().ok_or_else(|| internal_error("Control Mode child guard is empty"))
    }
}

impl Drop for ChildCleanupGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Gateway {
    pub fn new(socket: String) -> Result<Self, Phase3Error> {
        validate_socket(&socket)?;
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| internal_error("system clock is before the Unix epoch"))?;
        let connection_id = format!("p{}-{}", std::process::id(), since_epoch.as_nanos());
        Ok(Self { socket, connection_id, next_attachment_id: 1, attached: None })
    }

    pub fn handle(
        &mut self,
        request: Phase3Request,
        output: &SharedWriter,
    ) -> Result<Phase3Response, Phase3Error> {
        match request {
            Phase3Request::Probe => Ok(Phase3Response::Probe {
                connection_id: self.connection_id.clone(),
                tmux_version: self.tmux_version()?,
            }),
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
                self.create_synthetic_window(&session_name, &project_id, &name)?;
                Ok(Phase3Response::Acknowledged)
            }
            Phase3Request::ListWindows { project_id } => {
                Ok(Phase3Response::Windows { windows: self.list_windows(&project_id)? })
            }
            Phase3Request::Attach { project_id, window_id } => {
                self.attach(project_id, window_id, output)?;
                let stream = self
                    .attached
                    .as_ref()
                    .map(|attachment| attachment.stream.clone())
                    .ok_or_else(|| internal_error("attachment was not retained"))?;
                Ok(Phase3Response::Attached { stream })
            }
            Phase3Request::StartStream { stream } => {
                self.start_stream(&stream, output)?;
                Ok(Phase3Response::StreamStarted { stream })
            }
            Phase3Request::Detach { stream } => {
                self.detach_stream(&stream)?;
                Ok(Phase3Response::Detached)
            }
            Phase3Request::SendInput { stream, bytes } => {
                self.send_input(&stream, &bytes)?;
                Ok(Phase3Response::Acknowledged)
            }
            Phase3Request::Resize { stream, columns, rows } => {
                self.resize(&stream, columns, rows)?;
                Ok(Phase3Response::Acknowledged)
            }
            Phase3Request::CloseWindow { project_id, window_id } => {
                let target = self.target_for(&project_id, &window_id)?;
                self.tmux_status(["kill-window", "-t", &target])?;
                if self.attached.as_ref().is_some_and(|attachment| {
                    attachment.stream.project_id == project_id
                        && attachment.stream.window_id == window_id
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
                    .is_some_and(|attachment| attachment.stream.project_id == project_id)
                {
                    self.detach()?;
                }
                self.tmux_status(["kill-session", "-t", &name])?;
                Ok(Phase3Response::Acknowledged)
            }
            Phase3Request::ReadState => Ok(Phase3Response::State {
                attached: self.attached.as_ref().map(|attachment| attachment.stream.clone()),
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
            "--label",
            name,
        ])
    }

    fn create_synthetic_window(
        &self,
        session_name: &str,
        project_id: &str,
        window_name: &str,
    ) -> Result<(), Phase3Error> {
        let executable = std::env::current_exe()
            .map_err(|_| internal_error("cannot locate the POC runner executable"))?;
        let executable = executable.to_string_lossy().into_owned();
        let label = format!("{project_id}-{window_name}");
        self.tmux_status([
            "new-window",
            "-d",
            "-t",
            session_name,
            "-n",
            window_name,
            &executable,
            "phase3",
            "synthetic-runner",
            "--label",
            &label,
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
        _output: &SharedWriter,
    ) -> Result<(), Phase3Error> {
        self.detach()?;
        let target = self.target_for(&project_id, &window_id)?;
        let pane_id = self.pane_for_target(&target)?;

        let child = Command::new("tmux")
            .args(["-L", &self.socket, "-C", "attach-session", "-t", &target])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| unavailable_error("cannot start tmux Control Mode"))?;
        let mut child = ChildCleanupGuard::new(child);
        let control_stdin = child
            .child_mut()?
            .stdin
            .take()
            .ok_or_else(|| internal_error("Control Mode stdin unavailable"))?;
        let control_stdout = child
            .child_mut()?
            .stdout
            .take()
            .ok_or_else(|| internal_error("Control Mode stdout unavailable"))?;
        let control_stdin = Arc::new(Mutex::new(control_stdin));
        let intentionally_detached = Arc::new(AtomicBool::new(false));
        let output_overflowed = Arc::new(AtomicU8::new(OVERFLOW_NONE));
        let control_lines = start_control_reader(
            control_stdout,
            Arc::clone(&output_overflowed),
            Arc::clone(&intentionally_detached),
        );

        wait_for_command_block(&control_lines)?;
        if self.control_client_count()? != 1 {
            return Err(conflict_error("managed pane already has another tmux client"));
        }
        write_control_command(&control_stdin, &format!("refresh-client -A '{pane_id}:off'"))?;
        wait_for_command_block(&control_lines)?;

        let history = self.tmux_output_bytes([
            "capture-pane",
            "-p",
            "-e",
            "-J",
            "-S",
            &format!("-{HISTORY_LINES}"),
            "-t",
            &target,
        ])?;
        let attachment_id = self.next_attachment_id;
        self.next_attachment_id = self.next_attachment_id.saturating_add(1);
        let stream = AttachedTerminal {
            connection_id: self.connection_id.clone(),
            attachment_id,
            bootstrap_id: attachment_id,
            project_id,
            window_id,
            pane_id: pane_id.clone(),
        };
        let bootstrap_started = Arc::new(AtomicBool::new(false));
        let bootstrap_expired = Arc::new(AtomicBool::new(false));
        let watchdog_stdin = Arc::clone(&control_stdin);
        let watchdog_started = Arc::clone(&bootstrap_started);
        let watchdog_expired = Arc::clone(&bootstrap_expired);
        let watchdog_detached = Arc::clone(&intentionally_detached);
        thread::spawn(move || {
            thread::sleep(BOOTSTRAP_START_TIMEOUT);
            if !watchdog_started.load(Ordering::Acquire)
                && !watchdog_detached.load(Ordering::Acquire)
            {
                watchdog_expired.store(true, Ordering::Release);
                let _ = write_control_command(
                    &watchdog_stdin,
                    &format!("refresh-client -A '{pane_id}:on'"),
                );
            }
        });

        self.attached = Some(Attachment {
            stream,
            control_stdin,
            control_lines: Some(control_lines),
            history: Some(history),
            bootstrap_started,
            bootstrap_expired,
            output_overflowed,
            intentionally_detached,
            _child: child.disarm()?,
        });
        Ok(())
    }

    fn start_stream(
        &mut self,
        stream: &AttachedTerminal,
        output: &SharedWriter,
    ) -> Result<(), Phase3Error> {
        let attachment = self
            .attached
            .as_mut()
            .ok_or_else(|| state_error("no terminal bootstrap is attached"))?;
        ensure_current_stream(&attachment.stream, stream)?;
        if attachment.bootstrap_expired.load(Ordering::Acquire) {
            return Err(state_error("terminal bootstrap expired before stream start"));
        }
        if attachment.bootstrap_started.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let history = attachment
            .history
            .take()
            .ok_or_else(|| internal_error("terminal bootstrap history is unavailable"))?;
        let control_lines = attachment
            .control_lines
            .take()
            .ok_or_else(|| internal_error("terminal bootstrap control stream is unavailable"))?;
        emit_event(
            output,
            Phase3Event::History { stream: stream.clone(), sequence: 0, bytes: history },
        )?;

        let live_output = Arc::clone(output);
        let live_stream = stream.clone();
        let live_overflow = Arc::clone(&attachment.output_overflowed);
        let live_detached = Arc::clone(&attachment.intentionally_detached);
        thread::spawn(move || {
            forward_control_output(
                control_lines,
                live_output,
                live_stream,
                live_overflow,
                live_detached,
            )
        });
        write_control_command(
            &attachment.control_stdin,
            &format!("refresh-client -A '{}:on'", stream.pane_id),
        )
    }

    fn detach(&mut self) -> Result<(), Phase3Error> {
        if let Some(mut attachment) = self.attached.take() {
            attachment.intentionally_detached.store(true, Ordering::Release);
            let _ = write_control_command(&attachment.control_stdin, "detach-client");
            let _ = attachment._child.kill();
            let _ = attachment._child.wait();
        }
        Ok(())
    }

    fn detach_stream(&mut self, stream: &AttachedTerminal) -> Result<(), Phase3Error> {
        if let Some(attachment) = self.attached.as_ref() {
            ensure_current_stream(&attachment.stream, stream)?;
            self.detach()
        } else {
            Ok(())
        }
    }

    fn send_input(&mut self, stream: &AttachedTerminal, bytes: &[u8]) -> Result<(), Phase3Error> {
        if bytes.is_empty() || bytes.len() > MAX_INPUT_BYTES {
            return Err(validation_error("input frame must contain 1..=16384 bytes"));
        }
        let attachment =
            self.attached.as_mut().ok_or_else(|| state_error("no terminal is attached"))?;
        ensure_current_stream(&attachment.stream, stream)?;
        if !attachment.bootstrap_started.load(Ordering::Acquire) {
            return Err(state_error("terminal stream has not started"));
        }
        let target = target_for_ids(&attachment.stream.project_id, &attachment.stream.window_id)?;
        let mut control_stdin = attachment
            .control_stdin
            .lock()
            .map_err(|_| state_error("Control Mode input channel unavailable"))?;
        for byte in bytes {
            writeln!(control_stdin, "send-keys -t {target} -H {byte:02x}")
                .map_err(|_| state_error("Control Mode input channel disconnected"))?;
        }
        control_stdin.flush().map_err(|_| state_error("Control Mode input channel disconnected"))
    }

    fn resize(
        &self,
        stream: &AttachedTerminal,
        columns: u16,
        rows: u16,
    ) -> Result<(), Phase3Error> {
        if !(MIN_COLUMNS..=MAX_COLUMNS).contains(&columns) || !(MIN_ROWS..=MAX_ROWS).contains(&rows)
        {
            return Err(validation_error("resize is outside the POC bounds"));
        }
        let attachment =
            self.attached.as_ref().ok_or_else(|| state_error("no terminal is attached"))?;
        ensure_current_stream(&attachment.stream, stream)?;
        if !attachment.bootstrap_started.load(Ordering::Acquire) {
            return Err(state_error("terminal stream has not started"));
        }
        let target = target_for_ids(&attachment.stream.project_id, &attachment.stream.window_id)?;
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

    fn pane_for_target(&self, target: &str) -> Result<String, Phase3Error> {
        let output = self.tmux_output(["list-panes", "-t", target, "-F", "#{pane_id}"])?;
        let mut panes = output.lines();
        let pane_id = panes.next().ok_or_else(|| not_found_error("managed pane does not exist"))?;
        if panes.next().is_some() {
            return Err(state_error("Phase 3 does not support multiple panes per window"));
        }
        validate_pane_id(pane_id)?;
        Ok(pane_id.to_owned())
    }

    fn control_client_count(&self) -> Result<usize, Phase3Error> {
        let output = self.tmux_output(["list-clients", "-F", "#{client_control_mode}"])?;
        Ok(output.lines().count())
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
        let output = self.tmux_output_bytes(args)?;
        String::from_utf8(output).map_err(|_| state_error("tmux query returned non-UTF-8 metadata"))
    }

    fn tmux_output_bytes<const N: usize>(&self, args: [&str; N]) -> Result<Vec<u8>, Phase3Error> {
        let output = Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|_| unavailable_error("tmux is not executable"))?;
        if output.status.success() {
            Ok(output.stdout)
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

pub fn run_synthetic_runner(label: String) -> io::Result<()> {
    validate_slug(&label, 64, "synthetic label")
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid synthetic label"))?;
    let mut stdout = io::stdout().lock();
    let (input_sender, input_receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut bytes = [0_u8; 1024];
        while let Ok(count) = stdin.read(&mut bytes) {
            if count == 0 || input_sender.send(bytes[..count].to_vec()).is_err() {
                return;
            }
        }
    });
    let mut tick = 0_u64;
    loop {
        writeln!(stdout, "\x1b[36mphase3\x1b[0m source={label} tick={tick:06} 中文 表 😀")?;
        stdout.flush()?;
        tick = tick.saturating_add(1);
        while let Ok(bytes) = input_receiver.try_recv() {
            let encoded = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
            writeln!(stdout, "\x1b[32mINPUT_HEX\x1b[0m source={label} bytes={encoded}")?;
            if let Some(count) = synthetic_burst_count(&bytes) {
                for sequence in 0..count {
                    writeln!(
                        stdout,
                        "BURST source={label} sequence={sequence:06} payload=0123456789abcdef"
                    )?;
                }
            }
            stdout.flush()?;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn synthetic_burst_count(bytes: &[u8]) -> Option<u16> {
    let command = std::str::from_utf8(bytes).ok()?.trim();
    let count = command.strip_prefix("BURST ")?.parse::<u16>().ok()?;
    (count <= 2_048).then_some(count)
}

fn start_control_reader(
    control_stdout: impl io::Read + Send + 'static,
    output_overflowed: Arc<AtomicU8>,
    intentionally_detached: Arc<AtomicBool>,
) -> Receiver<ControlLine> {
    let (sender, receiver) = mpsc::sync_channel(MAX_BUFFERED_CONTROL_LINES);
    let queued_bytes = Arc::new(AtomicUsize::new(0));
    thread::spawn(move || {
        let mut reader = BufReader::new(control_stdout);
        loop {
            let mut bytes = Vec::new();
            match reader.read_until(b'\n', &mut bytes) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            if bytes.len() > MAX_CONTROL_LINE_BYTES {
                output_overflowed.store(OVERFLOW_LINE_BYTES, Ordering::Release);
                return;
            }
            if bytes.last() == Some(&b'\n') {
                bytes.pop();
            }
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            let size = bytes.len();
            let previous_bytes = queued_bytes.fetch_add(size, Ordering::AcqRel);
            if previous_bytes.saturating_add(size) > MAX_BUFFERED_CONTROL_BYTES {
                queued_bytes.fetch_sub(size, Ordering::AcqRel);
                output_overflowed.store(OVERFLOW_BUFFER_BYTES, Ordering::Release);
                return;
            }
            let control_line =
                ControlLine { value: bytes, size, queued_bytes: Arc::clone(&queued_bytes) };
            match sender.try_send(control_line) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    output_overflowed.store(OVERFLOW_BUFFER_LINES, Ordering::Release);
                    return;
                }
                Err(TrySendError::Disconnected(_)) => return,
            }
            if intentionally_detached.load(Ordering::Acquire) {
                return;
            }
        }
    });
    receiver
}

fn wait_for_command_block(control_lines: &Receiver<ControlLine>) -> Result<(), Phase3Error> {
    let deadline = std::time::Instant::now() + CONTROL_RESPONSE_TIMEOUT;
    let mut command_number = None;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(state_error("tmux Control Mode command timed out"));
        }
        let line = match control_lines.recv_timeout(remaining) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => {
                return Err(state_error("tmux Control Mode command timed out"));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(state_error("tmux Control Mode disconnected during bootstrap"));
            }
        };
        let Ok(value) = std::str::from_utf8(&line.value) else {
            continue;
        };
        if let Some(value) = value.strip_prefix("%begin ") {
            command_number = value.split_whitespace().nth(1).map(str::to_owned);
            continue;
        }
        let is_end = value.starts_with("%end ");
        let is_error = value.starts_with("%error ");
        if is_end || is_error {
            let response_number = value.split_whitespace().nth(2);
            if response_number == command_number.as_deref() {
                return if is_end {
                    Ok(())
                } else {
                    Err(state_error("tmux Control Mode command failed"))
                };
            }
        }
    }
}

fn write_control_command(
    control_stdin: &Arc<Mutex<ChildStdin>>,
    command: &str,
) -> Result<(), Phase3Error> {
    let mut stdin =
        control_stdin.lock().map_err(|_| state_error("Control Mode input channel unavailable"))?;
    writeln!(stdin, "{command}")
        .and_then(|()| stdin.flush())
        .map_err(|_| state_error("Control Mode input channel disconnected"))
}

fn forward_control_output(
    control_lines: Receiver<ControlLine>,
    output: SharedWriter,
    stream: AttachedTerminal,
    output_overflowed: Arc<AtomicU8>,
    intentionally_detached: Arc<AtomicBool>,
) {
    let mut sequence = 1_u64;
    for line in control_lines {
        if intentionally_detached.load(Ordering::Acquire) {
            return;
        }
        if let Some((pane_id, bytes)) =
            line.value.strip_prefix(b"%output ").and_then(parse_control_output)
        {
            if pane_id != stream.pane_id {
                continue;
            }
            if bytes.len() > MAX_OUTPUT_EVENT_BYTES {
                let _ = emit_event(
                    &output,
                    Phase3Event::StreamError {
                        stream: stream.clone(),
                        sequence,
                        code: "output_frame_too_large".to_owned(),
                    },
                );
                return;
            }
            if emit_event(&output, Phase3Event::Output { stream: stream.clone(), sequence, bytes })
                .is_err()
            {
                return;
            }
            sequence = sequence.saturating_add(1);
        }
        if line.value == b"%exit" || line.value.starts_with(b"%exit ") {
            let _ =
                emit_event(&output, Phase3Event::StreamClosed { stream: stream.clone(), sequence });
            return;
        }
        if line.value == format!("%window-close {}", stream.window_id).as_bytes()
            || line.value == format!("%unlinked-window-close {}", stream.window_id).as_bytes()
        {
            let _ =
                emit_event(&output, Phase3Event::StreamClosed { stream: stream.clone(), sequence });
            return;
        }
    }
    if !intentionally_detached.load(Ordering::Acquire) {
        let code = match output_overflowed.load(Ordering::Acquire) {
            OVERFLOW_LINE_BYTES => "output_control_line_too_large",
            OVERFLOW_BUFFER_BYTES => "output_buffer_byte_limit",
            OVERFLOW_BUFFER_LINES => "output_buffer_line_limit",
            _ => "control_mode_disconnected",
        };
        let _ = emit_event(
            &output,
            Phase3Event::StreamError { stream, sequence, code: code.to_owned() },
        );
    }
}

fn parse_control_output(value: &[u8]) -> Option<(String, Vec<u8>)> {
    let separator = value.iter().position(|byte| *byte == b' ')?;
    let pane_id = std::str::from_utf8(&value[..separator]).ok()?;
    let encoded = &value[separator + 1..];
    validate_pane_id(pane_id).ok()?;
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut characters = encoded.iter().copied().peekable();
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
    Some((pane_id.to_owned(), bytes))
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

fn ensure_current_stream(
    current: &AttachedTerminal,
    requested: &AttachedTerminal,
) -> Result<(), Phase3Error> {
    if current == requested {
        Ok(())
    } else {
        Err(stale_stream_error("terminal stream identity is stale"))
    }
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

fn validate_pane_id(value: &str) -> Result<(), Phase3Error> {
    if value.len() >= 2
        && value.starts_with('%')
        && value[1..].bytes().all(|byte| byte.is_ascii_digit())
    {
        Ok(())
    } else {
        Err(validation_error("invalid managed pane id"))
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

fn stale_stream_error(message: &str) -> Phase3Error {
    error("stale_stream", message)
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
        MAX_BUFFERED_CONTROL_LINES, parse_control_output, start_control_reader,
        synthetic_burst_count, target_for_ids, validate_pane_id, validate_project_id,
        validate_socket, validate_window_id,
    };
    use std::{
        io::Cursor,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU8, Ordering},
        },
        thread,
        time::{Duration, Instant},
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
        assert!(validate_pane_id("%17").is_ok());
        assert!(validate_pane_id("%17:kill").is_err());
        assert!(target_for_ids("project-a", "@17").is_ok());
    }

    #[test]
    fn decodes_tmux_control_mode_output_without_utf8_assumptions() {
        assert_eq!(
            parse_control_output("%0 hello\\015\\012中文 😀".as_bytes()),
            Some(("%0".to_owned(), "hello\r\n中文 😀".as_bytes().to_vec()))
        );
        assert_eq!(parse_control_output(b"%0 \\999"), None);
        assert_eq!(parse_control_output(b"%0 \xe4"), Some(("%0".to_owned(), vec![0xe4])));
    }

    #[test]
    fn bounds_synthetic_burst_requests() {
        assert_eq!(synthetic_burst_count(b"BURST 2048\n"), Some(2048));
        assert_eq!(synthetic_burst_count(b"BURST 2049\n"), None);
        assert_eq!(synthetic_burst_count(b"OTHER 20\n"), None);
    }

    #[test]
    fn marks_the_bounded_control_queue_invalid_on_overflow() {
        let input = "%output %0 x\n".repeat(MAX_BUFFERED_CONTROL_LINES + 2);
        let overflowed = Arc::new(AtomicU8::new(0));
        let detached = Arc::new(AtomicBool::new(false));
        let _receiver = start_control_reader(
            Cursor::new(input.into_bytes()),
            Arc::clone(&overflowed),
            detached,
        );
        let deadline = Instant::now() + Duration::from_secs(1);
        while overflowed.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
            thread::yield_now();
        }
        assert_ne!(overflowed.load(Ordering::Acquire), 0);
    }
}
