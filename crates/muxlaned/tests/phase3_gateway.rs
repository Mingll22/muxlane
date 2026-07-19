#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use clap as _;
use muxlane_protocol::{
    AttachedTerminal, ManagedWindow, Phase3Event, Phase3Frame, Phase3Request,
    Phase3RequestEnvelope, Phase3Response, ResultFrame,
};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const EVENT_TIMEOUT: Duration = Duration::from_secs(5);

struct GatewayHarness {
    socket: String,
    child: Child,
    stdin: ChildStdin,
    frames: Receiver<Phase3Frame>,
    events: VecDeque<Phase3Event>,
    next_id: u64,
}

impl GatewayHarness {
    fn start(suffix: &str) -> Self {
        let socket = format!("muxlane-p3-{suffix}-{}", std::process::id());
        let _ = Command::new("tmux")
            .args(["-L", &socket, "kill-server"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let mut child = Command::new(env!("CARGO_BIN_EXE_muxlaned"))
            .args(["phase3", "gateway", "--socket", &socket])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("starts the dedicated phase 3 gateway");
        let stdin = child.stdin.take().expect("gateway stdin is piped");
        let stdout = child.stdout.take().expect("gateway stdout is piped");
        let (frame_sender, frames) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let frame = serde_json::from_str(&line).expect("gateway frame is typed JSON");
                if frame_sender.send(frame).is_err() {
                    return;
                }
            }
        });
        Self { socket, child, stdin, frames, events: VecDeque::new(), next_id: 1 }
    }

    fn request(&mut self, request: Phase3Request) -> Result<Phase3Response, String> {
        let id = self.next_id;
        self.next_id += 1;
        let encoded = serde_json::to_string(&Phase3RequestEnvelope { id, request })
            .expect("test request serializes");
        writeln!(self.stdin, "{encoded}").expect("gateway receives request");
        self.stdin.flush().expect("gateway input flushes");
        let deadline = Instant::now() + RESPONSE_TIMEOUT;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let frame = self.frames.recv_timeout(remaining).expect("gateway returns a response");
            match frame {
                Phase3Frame::Response { id: response_id, result } if response_id == id => {
                    return match result {
                        ResultFrame::Ok { response } => Ok(response),
                        ResultFrame::Error { error } => Err(error.code),
                    };
                }
                Phase3Frame::Response { .. } => panic!("received an unrelated response"),
                Phase3Frame::Event { event } => self.events.push_back(event),
            }
        }
        panic!("timed out waiting for response {id}");
    }

    fn attach_and_start(&mut self, project_id: &str, window_id: &str) -> AttachedTerminal {
        let stream = match self
            .request(Phase3Request::Attach {
                project_id: project_id.to_owned(),
                window_id: window_id.to_owned(),
            })
            .expect("attaches a managed window")
        {
            Phase3Response::Attached { stream } => stream,
            unexpected => panic!("expected attached response, got {unexpected:?}"),
        };
        assert_eq!(stream.project_id, project_id);
        assert_eq!(stream.window_id, window_id);
        assert!(matches!(
            self.request(Phase3Request::StartStream { stream: stream.clone() }),
            Ok(Phase3Response::StreamStarted { .. })
        ));
        stream
    }

    fn next_event(&mut self) -> Phase3Event {
        if let Some(event) = self.events.pop_front() {
            return event;
        }
        match self.frames.recv_timeout(EVENT_TIMEOUT).expect("gateway emits terminal data") {
            Phase3Frame::Event { event } => event,
            Phase3Frame::Response { .. } => panic!("received a response without a request"),
        }
    }

    fn collect_stream_bytes(
        &mut self,
        stream: &AttachedTerminal,
        minimum_output_events: usize,
    ) -> (Vec<u8>, Vec<u64>) {
        let mut bytes = Vec::new();
        let mut sequences = Vec::new();
        let mut outputs = 0;
        let deadline = Instant::now() + EVENT_TIMEOUT;
        while Instant::now() < deadline && outputs < minimum_output_events {
            match self.next_event() {
                Phase3Event::History { stream: event_stream, sequence, bytes: event_bytes }
                | Phase3Event::Output { stream: event_stream, sequence, bytes: event_bytes } => {
                    assert_eq!(&event_stream, stream, "old or cross-terminal frame leaked");
                    if sequence > 0 {
                        outputs += 1;
                    }
                    sequences.push(sequence);
                    bytes.extend(event_bytes);
                }
                Phase3Event::StreamClosed { stream: event_stream, sequence }
                | Phase3Event::StreamError { stream: event_stream, sequence, .. } => {
                    assert_eq!(&event_stream, stream);
                    sequences.push(sequence);
                    break;
                }
                Phase3Event::ConnectionClosed { .. } => panic!("gateway connection closed"),
            }
        }
        (bytes, sequences)
    }

    fn collect_until_contains(
        &mut self,
        stream: &AttachedTerminal,
        needle: &str,
    ) -> (Vec<u8>, Vec<u64>) {
        let mut bytes = Vec::new();
        let mut sequences = Vec::new();
        let deadline = Instant::now() + EVENT_TIMEOUT;
        while Instant::now() < deadline {
            match self.next_event() {
                Phase3Event::History { stream: event_stream, sequence, bytes: event_bytes }
                | Phase3Event::Output { stream: event_stream, sequence, bytes: event_bytes } => {
                    assert_eq!(&event_stream, stream, "old or cross-terminal frame leaked");
                    sequences.push(sequence);
                    bytes.extend(event_bytes);
                    if String::from_utf8_lossy(&bytes).contains(needle) {
                        return (bytes, sequences);
                    }
                }
                ref event @ (Phase3Event::StreamClosed { stream: ref event_stream, .. }
                | Phase3Event::StreamError { stream: ref event_stream, .. }) => {
                    assert_eq!(event_stream, stream);
                    panic!("terminal stream ended before marker: {event:?}");
                }
                Phase3Event::ConnectionClosed { .. } => panic!("gateway connection closed"),
            }
        }
        let text = String::from_utf8_lossy(&bytes);
        let tail =
            text.chars().rev().take(2_000).collect::<String>().chars().rev().collect::<String>();
        panic!(
            "terminal stream did not contain the expected synthetic marker: {needle}; tail={tail}"
        );
    }

    fn pane_size(&self, stream: &AttachedTerminal) -> String {
        Command::new("tmux")
            .args([
                "-L",
                &self.socket,
                "display-message",
                "-p",
                "-t",
                &format!("mlp3-{}:{}", stream.project_id, stream.window_id),
                "#{window_width}x#{window_height}",
            ])
            .output()
            .and_then(|output| {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
                } else {
                    Err(std::io::Error::other("tmux size query failed"))
                }
            })
            .expect("reads the managed window size")
    }
}

impl Drop for GatewayHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = Command::new("tmux")
            .args(["-L", &self.socket, "kill-server"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn windows(response: Phase3Response) -> Vec<ManagedWindow> {
    match response {
        Phase3Response::Windows { windows } => windows,
        unexpected => panic!("expected windows response, got {unexpected:?}"),
    }
}

fn tick_values(bytes: &[u8]) -> Vec<u64> {
    let text = String::from_utf8_lossy(bytes);
    let mut values = Vec::new();
    let mut remainder = text.as_ref();
    while let Some(index) = remainder.find("tick=") {
        remainder = &remainder[index + 5..];
        let digits = remainder.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 {
            values.push(remainder[..digits].parse().expect("tick is numeric"));
        }
        remainder = &remainder[digits..];
    }
    values
}

fn burst_values(bytes: &[u8]) -> Vec<u64> {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .filter_map(|line| line.split("sequence=").nth(1))
        .filter_map(|value| value.split_whitespace().next())
        .filter_map(|value| value.parse().ok())
        .collect()
}

fn assert_contiguous(values: &[u64]) {
    assert!(values.len() >= 2, "expected more than one synthetic sequence value");
    for pair in values.windows(2) {
        assert_eq!(pair[1], pair[0] + 1, "synthetic output must be gap-free and ordered");
    }
}

#[test]
fn bootstrap_reconnect_is_gap_free_ordered_and_rejects_old_streams() {
    let mut gateway = GatewayHarness::start("sequence");
    gateway
        .request(Phase3Request::CreateSyntheticSession { project_id: "project-a".to_owned() })
        .expect("creates project A");
    let main = windows(
        gateway
            .request(Phase3Request::ListWindows { project_id: "project-a".to_owned() })
            .expect("lists project A windows"),
    )[0]
    .id
    .clone();
    thread::sleep(Duration::from_millis(750));

    let mut prior_stream: Option<AttachedTerminal> = None;
    let mut total_sequences = 0;
    for reconnect in 0..3 {
        let stream = gateway.attach_and_start("project-a", &main);
        if let Some(old_stream) = prior_stream.as_ref() {
            assert_eq!(
                gateway.request(Phase3Request::SendInput {
                    stream: old_stream.clone(),
                    bytes: b"stale\n".to_vec(),
                }),
                Err("stale_stream".to_owned())
            );
        }
        let (bytes, event_sequences) = gateway.collect_stream_bytes(&stream, 12);
        assert_eq!(event_sequences, (0..event_sequences.len() as u64).collect::<Vec<_>>());
        let ticks = tick_values(&bytes);
        assert_contiguous(&ticks);
        total_sequences += ticks.len();
        prior_stream = Some(stream);
        eprintln!(
            "phase3_sequence reconnect={reconnect} values={} missing=0 duplicate=0 reordered=0 cross_stream=0",
            ticks.len()
        );
    }
    assert!(total_sequences >= 36);
}

#[test]
fn isolates_two_projects_two_windows_input_resize_unicode_and_ctrl_c() {
    let mut gateway = GatewayHarness::start("isolation");
    for project_id in ["project-a", "project-b"] {
        gateway
            .request(Phase3Request::CreateSyntheticSession { project_id: project_id.to_owned() })
            .expect("creates a project session");
        gateway
            .request(Phase3Request::CreateWindow {
                project_id: project_id.to_owned(),
                name: "aux".to_owned(),
            })
            .expect("creates a project auxiliary window");
    }

    let project_a_windows = windows(
        gateway
            .request(Phase3Request::ListWindows { project_id: "project-a".to_owned() })
            .expect("lists project A windows"),
    );
    let project_b_windows = windows(
        gateway
            .request(Phase3Request::ListWindows { project_id: "project-b".to_owned() })
            .expect("lists project B windows"),
    );
    assert_eq!(project_a_windows.len(), 2);
    assert_eq!(project_b_windows.len(), 2);
    assert_eq!(project_a_windows[1].name, project_b_windows[1].name);
    assert_ne!(project_a_windows[0].id, project_b_windows[0].id);

    let a_main = gateway.attach_and_start("project-a", &project_a_windows[0].id);
    gateway
        .request(Phase3Request::SendInput {
            stream: a_main.clone(),
            bytes: "中文😀\n".as_bytes().to_vec(),
        })
        .expect("sends UTF-8 input to project A");
    gateway
        .request(Phase3Request::Resize { stream: a_main.clone(), columns: 101, rows: 31 })
        .expect("resizes project A only");
    assert_eq!(gateway.pane_size(&a_main), "101x31");
    let (a_bytes, a_sequences) =
        gateway.collect_until_contains(&a_main, "bytes=e4b8ade69687f09f98800a");
    assert_eq!(a_sequences, (0..a_sequences.len() as u64).collect::<Vec<_>>());
    let a_text = String::from_utf8_lossy(&a_bytes);
    assert!(a_text.contains("source=mlp3-project-a"));
    assert!(
        a_text.contains("bytes=e4b8ade69687f09f98800a"),
        "synthetic UTF-8 input evidence was missing: {a_text}"
    );
    assert!(!a_text.contains("source=mlp3-project-b"));

    let b_main = gateway.attach_and_start("project-b", &project_b_windows[0].id);
    assert_eq!(
        gateway.request(Phase3Request::Resize { stream: a_main, columns: 80, rows: 24 }),
        Err("stale_stream".to_owned())
    );
    assert_eq!(gateway.pane_size(&b_main), "100x32");
    gateway
        .request(Phase3Request::SendInput {
            stream: b_main.clone(),
            bytes: b"project-b\n".to_vec(),
        })
        .expect("sends input to project B");
    let (b_bytes, b_sequences) =
        gateway.collect_until_contains(&b_main, "bytes=70726f6a6563742d620a");
    assert_eq!(b_sequences, (0..b_sequences.len() as u64).collect::<Vec<_>>());
    let b_text = String::from_utf8_lossy(&b_bytes);
    assert!(b_text.contains("source=mlp3-project-b"));
    assert!(!b_text.contains("source=mlp3-project-a"));

    let a_aux = gateway.attach_and_start("project-a", &project_a_windows[1].id);
    gateway
        .request(Phase3Request::SendInput { stream: a_aux.clone(), bytes: vec![3] })
        .expect("sends Ctrl+C to the exact foreground process");
    let deadline = Instant::now() + EVENT_TIMEOUT;
    let mut closed = false;
    while Instant::now() < deadline && !closed {
        match gateway.next_event() {
            Phase3Event::StreamClosed { stream, .. } => {
                assert_eq!(stream, a_aux);
                closed = true;
            }
            Phase3Event::History { stream, .. }
            | Phase3Event::Output { stream, .. }
            | Phase3Event::StreamError { stream, .. } => assert_eq!(stream, a_aux),
            Phase3Event::ConnectionClosed { .. } => panic!("gateway connection closed"),
        }
    }
    assert!(closed, "Ctrl+C must close the targeted synthetic foreground process");

    assert!(matches!(
        gateway.request(Phase3Request::Detach { stream: a_aux.clone() }),
        Ok(Phase3Response::Detached)
    ));
    assert!(matches!(
        gateway.request(Phase3Request::Detach { stream: a_aux }),
        Ok(Phase3Response::Detached)
    ));
    for project_id in ["project-a", "project-b"] {
        gateway
            .request(Phase3Request::CleanupSession { project_id: project_id.to_owned() })
            .expect("cleans the dedicated synthetic session");
    }
}

#[test]
fn rejects_invalid_targets_dimensions_and_input_bounds() {
    let mut gateway = GatewayHarness::start("validation");
    assert_eq!(
        gateway.request(Phase3Request::Attach {
            project_id: "project-a;kill-server".to_owned(),
            window_id: "@0".to_owned(),
        }),
        Err("validation".to_owned())
    );
    gateway
        .request(Phase3Request::CreateSyntheticSession { project_id: "project-a".to_owned() })
        .expect("creates the validation session");
    let window_id = windows(
        gateway
            .request(Phase3Request::ListWindows { project_id: "project-a".to_owned() })
            .expect("lists the validation window"),
    )[0]
    .id
    .clone();
    let stream = gateway.attach_and_start("project-a", &window_id);
    assert_eq!(
        gateway.request(Phase3Request::Resize { stream: stream.clone(), columns: 19, rows: 32 }),
        Err("validation".to_owned())
    );
    assert_eq!(
        gateway.request(Phase3Request::SendInput { stream: stream.clone(), bytes: Vec::new() }),
        Err("validation".to_owned())
    );
    assert_eq!(
        gateway.request(Phase3Request::SendInput { stream, bytes: vec![0; 16 * 1024 + 1] }),
        Err("validation".to_owned())
    );
}

#[test]
fn preserves_a_large_ordered_burst_with_a_bounded_data_plane() {
    let mut gateway = GatewayHarness::start("burst");
    gateway
        .request(Phase3Request::CreateSyntheticSession { project_id: "project-a".to_owned() })
        .expect("creates the burst session");
    let window_id = windows(
        gateway
            .request(Phase3Request::ListWindows { project_id: "project-a".to_owned() })
            .expect("lists the burst window"),
    )[0]
    .id
    .clone();
    let stream = gateway.attach_and_start("project-a", &window_id);
    gateway
        .request(Phase3Request::SendInput {
            stream: stream.clone(),
            bytes: b"BURST 512\n".to_vec(),
        })
        .expect("requests the bounded synthetic burst");
    let (bytes, sequences) = gateway.collect_until_contains(&stream, "sequence=000511");
    assert_eq!(sequences, (0..sequences.len() as u64).collect::<Vec<_>>());
    let bursts = burst_values(&bytes);
    assert_eq!(bursts, (0..512).collect::<Vec<_>>());
    eprintln!("phase3_burst values={} missing=0 duplicate=0 reordered=0 overflow=0", bursts.len());
}
