#![cfg(target_os = "linux")]

use std::{
    collections::VecDeque,
    fs,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use clap as _;
use muxlane_core::{
    layout::Layout,
    model::Terminal,
    service::register_project,
    storage::{Storage, now},
};
use muxlane_protocol::{
    TERMINAL_DATA_PROTOCOL_MAJOR, TERMINAL_DATA_PROTOCOL_MINOR, TerminalDataEvent,
    TerminalDataFrame, TerminalDataRequest, TerminalDataRequestEnvelope, TerminalDataResponse,
    TerminalDataResult,
};
use nix as _;
use tempfile::TempDir;
use uuid as _;

struct Harness {
    child: Child,
    stdin: ChildStdin,
    frames: Receiver<TerminalDataFrame>,
    events: VecDeque<TerminalDataEvent>,
    next_id: u64,
}

impl Harness {
    fn start(root: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_muxlaned"))
            .arg("terminal-gateway")
            .env("MUXLANE_DATA_DIR", root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, frames) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                sender.send(serde_json::from_str(&line).unwrap()).unwrap();
            }
        });
        Self { child, stdin, frames, events: VecDeque::new(), next_id: 1 }
    }

    fn request(&mut self, request: TerminalDataRequest) -> Result<TerminalDataResponse, String> {
        let id = self.next_id;
        self.next_id += 1;
        writeln!(
            self.stdin,
            "{}",
            serde_json::to_string(&TerminalDataRequestEnvelope { id, request }).unwrap()
        )
        .unwrap();
        self.stdin.flush().unwrap();
        loop {
            match self.frames.recv_timeout(Duration::from_secs(5)).unwrap() {
                TerminalDataFrame::Response { id: response_id, result } if response_id == id => {
                    return match result {
                        TerminalDataResult::Ok { response } => Ok(response),
                        TerminalDataResult::Error { error } => Err(error.code),
                    };
                }
                TerminalDataFrame::Event { event } => self.events.push_back(event),
                TerminalDataFrame::Response { .. } => panic!("unrelated response"),
            }
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn fixture(label: &str) -> (TempDir, Storage, Terminal) {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(Layout::initialize(temp.path().join("runtime")).unwrap()).unwrap();
    let terminal = add_terminal(&storage, temp.path(), label);
    (temp, storage, terminal)
}

fn add_terminal(storage: &Storage, root: &std::path::Path, label: &str) -> Terminal {
    let source = root.join(label);
    fs::create_dir(&source).unwrap();
    let project = register_project(storage, &source, label).unwrap();
    let output = Command::new("tmux")
        .args([
            "-L",
            "muxlane-runtime",
            "new-session",
            "-d",
            "-P",
            "-F",
            "#{window_id}",
            "-s",
            &project.tmux_session_name,
            "-n",
            "shell",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        Command::new("tmux")
            .args([
                "-L",
                "muxlane-runtime",
                "set-option",
                "-t",
                &project.tmux_session_name,
                "@muxlane-project-id",
                &project.project_id
            ])
            .status()
            .unwrap()
            .success()
    );
    let terminal = Terminal {
        terminal_id: format!("terminal_{label}"),
        project_id: project.project_id,
        kind: "auxiliary".to_owned(),
        display_name: label.to_owned(),
        tmux_window_identity: String::from_utf8(output.stdout).unwrap().trim().to_owned(),
        lifecycle_status: "running".to_owned(),
        created_at: now(),
        closed_at: None,
    };
    storage.insert_terminal(&terminal, 0).unwrap();
    terminal
}

#[test]
fn formal_gateway_handshake_attach_live_reconnect_and_stale_rejection() {
    let (temp, storage, terminal) = fixture("formal");
    let parallel_terminal = add_terminal(&storage, temp.path(), "parallel");
    let mut first = Harness::start(storage.layout().root());
    assert!(matches!(
        first
            .request(TerminalDataRequest::Handshake {
                protocol_major: TERMINAL_DATA_PROTOCOL_MAJOR,
                protocol_minor: TERMINAL_DATA_PROTOCOL_MINOR,
                client_name: "integration-test".to_owned(),
            })
            .unwrap(),
        TerminalDataResponse::Handshake { .. }
    ));
    let stream = match first
        .request(TerminalDataRequest::Attach { terminal_id: terminal.terminal_id.clone() })
        .unwrap()
    {
        TerminalDataResponse::Attached { stream } => stream,
        other => panic!("unexpected response {other:?}"),
    };
    assert!(matches!(
        first.request(TerminalDataRequest::StartStream { stream: stream.clone() }).unwrap(),
        TerminalDataResponse::StreamStarted { .. }
    ));
    assert!(matches!(
        first
            .request(TerminalDataRequest::Resize { stream: stream.clone(), columns: 100, rows: 30 })
            .unwrap(),
        TerminalDataResponse::Acknowledged
    ));
    first
        .request(TerminalDataRequest::SendInput {
            stream: stream.clone(),
            bytes: b"printf FORMAL_LIVE\n".to_vec(),
        })
        .unwrap();
    let mut parallel = Harness::start(storage.layout().root());
    parallel
        .request(TerminalDataRequest::Handshake {
            protocol_major: 1,
            protocol_minor: 0,
            client_name: "parallel".to_owned(),
        })
        .unwrap();
    assert!(matches!(
        parallel
            .request(TerminalDataRequest::Attach {
                terminal_id: parallel_terminal.terminal_id.clone()
            })
            .unwrap(),
        TerminalDataResponse::Attached { .. }
    ));
    parallel
        .request(TerminalDataRequest::Close { terminal_id: parallel_terminal.terminal_id.clone() })
        .unwrap();
    drop(parallel);
    let mut observed = Vec::new();
    for _ in 0..20 {
        let event = first.events.pop_front().unwrap_or_else(|| {
            first.frames.recv_timeout(Duration::from_secs(2)).unwrap().into_event()
        });
        match event {
            TerminalDataEvent::History { bytes, .. } | TerminalDataEvent::Output { bytes, .. } => {
                observed.extend(bytes)
            }
            _ => {}
        }
        if String::from_utf8_lossy(&observed).contains("FORMAL_LIVE") {
            break;
        }
    }
    assert!(String::from_utf8_lossy(&observed).contains("FORMAL_LIVE"));
    drop(first);

    let mut second = Harness::start(storage.layout().root());
    second
        .request(TerminalDataRequest::Handshake {
            protocol_major: 1,
            protocol_minor: 0,
            client_name: "reconnect".to_owned(),
        })
        .unwrap();
    let new_stream = match second
        .request(TerminalDataRequest::Attach { terminal_id: terminal.terminal_id.clone() })
        .unwrap()
    {
        TerminalDataResponse::Attached { stream } => stream,
        _ => unreachable!(),
    };
    assert_ne!(new_stream.connection_id, stream.connection_id);
    assert_eq!(
        second.request(TerminalDataRequest::SendInput { stream, bytes: vec![3] }).unwrap_err(),
        "STALE_STREAM"
    );
    assert!(matches!(
        second
            .request(TerminalDataRequest::Close { terminal_id: terminal.terminal_id.clone() })
            .unwrap(),
        TerminalDataResponse::Closed { .. }
    ));
    assert_eq!(storage.terminal(&terminal.terminal_id).unwrap().lifecycle_status, "closed");
    drop(temp);
}

trait IntoEvent {
    fn into_event(self) -> TerminalDataEvent;
}
impl IntoEvent for TerminalDataFrame {
    fn into_event(self) -> TerminalDataEvent {
        match self {
            TerminalDataFrame::Event { event } => event,
            _ => panic!("expected event"),
        }
    }
}
