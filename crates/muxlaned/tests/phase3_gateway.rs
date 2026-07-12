#![forbid(unsafe_code)]

use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant},
};

use clap as _;
use muxlane_protocol::{
    ManagedWindow, Phase3Frame, Phase3Request, Phase3RequestEnvelope, Phase3Response, ResultFrame,
};

struct GatewayHarness {
    socket: String,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl GatewayHarness {
    fn start() -> Self {
        let socket = format!("muxlane-p3-test-{}", std::process::id());
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
        Self { socket, child, stdin, stdout: BufReader::new(stdout), next_id: 1 }
    }

    fn request(&mut self, request: Phase3Request) -> Result<Phase3Response, String> {
        let id = self.next_id;
        self.next_id += 1;
        let encoded = serde_json::to_string(&Phase3RequestEnvelope { id, request })
            .expect("test request serializes");
        writeln!(self.stdin, "{encoded}").expect("gateway receives request");
        self.stdin.flush().expect("gateway input flushes");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).expect("gateway stdout reads");
            assert_ne!(read, 0, "gateway exited before response {id}");
            let frame: Phase3Frame =
                serde_json::from_str(line.trim()).expect("gateway frame is typed JSON");
            if let Phase3Frame::Response { id: response_id, result } = frame
                && response_id == id
            {
                return match result {
                    ResultFrame::Ok { response } => Ok(response),
                    ResultFrame::Error { error } => Err(error.code),
                };
            }
        }
        panic!("timed out waiting for response {id}");
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

#[test]
fn isolates_two_managed_sessions_and_their_windows_on_a_dedicated_socket() {
    if Command::new("tmux").arg("-V").stdout(Stdio::null()).stderr(Stdio::null()).status().is_err()
    {
        return;
    }
    let mut gateway = GatewayHarness::start();
    for project_id in ["project-a", "project-b"] {
        assert!(matches!(
            gateway.request(Phase3Request::CreateSyntheticSession {
                project_id: project_id.to_owned(),
            }),
            Ok(Phase3Response::Acknowledged)
        ));
        assert!(matches!(
            gateway.request(Phase3Request::CreateWindow {
                project_id: project_id.to_owned(),
                name: "aux".to_owned(),
            }),
            Ok(Phase3Response::Acknowledged)
        ));
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
    assert_ne!(project_a_windows[0].id, project_b_windows[0].id);

    let a_main = project_a_windows[0].id.clone();
    let b_main = project_b_windows[0].id.clone();
    assert!(matches!(
        gateway.request(Phase3Request::Attach {
            project_id: "project-a".to_owned(),
            window_id: a_main
        }),
        Ok(Phase3Response::Attached { .. })
    ));
    assert!(matches!(
        gateway.request(Phase3Request::SendInput { bytes: vec![3] }),
        Ok(Phase3Response::Acknowledged)
    ));
    assert!(matches!(
        gateway.request(Phase3Request::Attach {
            project_id: "project-b".to_owned(),
            window_id: b_main
        }),
        Ok(Phase3Response::Attached { .. })
    ));
    assert!(matches!(
        gateway.request(Phase3Request::Resize { columns: 101, rows: 31 }),
        Ok(Phase3Response::Acknowledged)
    ));
    assert_eq!(
        gateway.request(Phase3Request::Attach {
            project_id: "project-a;kill-server".to_owned(),
            window_id: "@0".to_owned(),
        }),
        Err("validation".to_owned())
    );

    for project_id in ["project-a", "project-b"] {
        assert!(matches!(
            gateway.request(Phase3Request::CleanupSession { project_id: project_id.to_owned() }),
            Ok(Phase3Response::Acknowledged)
        ));
    }
}
