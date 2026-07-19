#![cfg(target_os = "linux")]

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use clap as _;
use muxlane_core as _;
use muxlane_protocol::{
    CAPABILITIES, CommandPresetTemplate, ControlRequest, ControlResponse, HandshakeRequest,
    PROTOCOL_MAJOR, PROTOCOL_MINOR, RpcRequest, RpcResponse, RpcResponseBody,
    TerminalPresetTemplate,
};
use nix as _;
use tempfile::TempDir;
use uuid as _;

struct Daemon {
    _temp: TempDir,
    root: std::path::PathBuf,
    child: Child,
}

#[test]
fn formal_workbench_services_persist_safe_configuration_and_confine_files() {
    let daemon = Daemon::start();
    let source = daemon._temp.path().join("workbench-project");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("README.md"), "# fixture\nhello\n").unwrap();
    let mut client = Client::connect(&daemon.root);
    let project = match client
        .request(ControlRequest::ProjectRegister {
            source_path: source.to_string_lossy().into_owned(),
            name: "workbench".to_owned(),
            operation_id: operation(),
        })
        .unwrap()
    {
        ControlResponse::Project(project) => project,
        _ => unreachable!(),
    };
    let template = match client
        .request(ControlRequest::WorkbenchTemplateSave {
            template_id: None,
            name: "Rust".to_owned(),
            description: "safe defaults".to_owned(),
            default_model: "gpt-5.6-sol".to_owned(),
            reasoning: "high".to_owned(),
            terminal_presets: vec![TerminalPresetTemplate {
                name: "Shell".to_owned(),
                kind: "shell".to_owned(),
            }],
            command_presets: vec![CommandPresetTemplate {
                name: "Test".to_owned(),
                description: "Run tests".to_owned(),
                terminal_kind: "shell".to_owned(),
                working_directory: "".to_owned(),
                command: "cargo test".to_owned(),
            }],
            operation_id: operation(),
        })
        .unwrap()
    {
        ControlResponse::ProjectTemplate(template) => template,
        _ => unreachable!(),
    };
    assert!(matches!(
        client
            .request(ControlRequest::WorkbenchTemplateApply {
                project_id: project.project_id.clone(),
                template_id: template.template_id,
                operation_id: operation(),
            })
            .unwrap(),
        ControlResponse::ProjectSettings(_)
    ));
    assert!(matches!(
        client
            .request(ControlRequest::WorkspacePreview {
                project_id: project.project_id.clone(),
                relative_path: "README.md".to_owned(),
            })
            .unwrap(),
        ControlResponse::WorkspacePreview(preview) if preview.content.contains("fixture")
    ));
    assert_eq!(
        client
            .request(ControlRequest::WorkspacePreview {
                project_id: project.project_id.clone(),
                relative_path: "../outside".to_owned(),
            })
            .unwrap_err(),
        "PATH_REJECTED"
    );
    assert!(matches!(
        client
            .request(ControlRequest::WorkbenchHistoryAppend {
                project_id: project.project_id.clone(),
                terminal_id: None,
                thread_id: None,
                kind: "prompt".to_owned(),
                input_text: "review this module".to_owned(),
                operation_id: operation(),
            })
            .unwrap(),
        ControlResponse::InputHistoryEntry(_)
    ));
    assert_eq!(
        client
            .request(ControlRequest::WorkbenchHistoryAppend {
                project_id: project.project_id,
                terminal_id: None,
                thread_id: None,
                kind: "shell".to_owned(),
                input_text: "Authorization: Bearer do-not-store".to_owned(),
                operation_id: operation(),
            })
            .unwrap_err(),
        "SENSITIVE_CONTENT_REJECTED"
    );
}

impl Daemon {
    fn start() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runtime");
        let child = Command::new(env!("CARGO_BIN_EXE_muxlaned"))
            .arg("serve")
            .env("MUXLANE_DATA_DIR", &root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("run/muxlaned.sock").exists() {
            assert!(Instant::now() < deadline, "daemon socket did not appear");
            thread::sleep(Duration::from_millis(20));
        }
        Self { _temp: temp, root, child }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Client {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
    id: u64,
}
impl Client {
    fn connect(root: &std::path::Path) -> Self {
        let stream = UnixStream::connect(root.join("run/muxlaned.sock")).unwrap();
        let mut client =
            Self { writer: stream.try_clone().unwrap(), reader: BufReader::new(stream), id: 1 };
        assert!(matches!(
            client
                .request(ControlRequest::SystemHandshake(HandshakeRequest {
                    protocol_major: PROTOCOL_MAJOR,
                    protocol_minor: PROTOCOL_MINOR,
                    client_name: "integration_test".to_owned(),
                    client_version: "1".to_owned(),
                    requested_capabilities: CAPABILITIES
                        .iter()
                        .map(|value| (*value).to_owned())
                        .collect(),
                }))
                .unwrap(),
            ControlResponse::Handshake(_)
        ));
        client
    }

    fn request(&mut self, request: ControlRequest) -> Result<ControlResponse, String> {
        let id = format!("request_{}", self.id);
        self.id += 1;
        serde_json::to_writer(&mut self.writer, &RpcRequest::new(&id, request)).unwrap();
        self.writer.write_all(b"\n").unwrap();
        self.writer.flush().unwrap();
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        let response: RpcResponse = serde_json::from_str(&line).unwrap();
        match response.body {
            RpcResponseBody::Result { result } => Ok(result),
            RpcResponseBody::Error { error } => Err(error.data.error_code),
        }
    }
}

fn operation() -> String {
    format!("operation_{}", uuid::Uuid::new_v4().simple())
}

#[test]
fn single_instance_and_formal_domain_services_close_end_to_end() {
    let mut daemon = Daemon::start();
    let mut duplicate = Command::new(env!("CARGO_BIN_EXE_muxlaned"))
        .arg("serve")
        .env("MUXLANE_DATA_DIR", &daemon.root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(!duplicate.wait().unwrap().success());

    let auth = daemon._temp.path().join("fixture-auth.json");
    fs::write(&auth, br#"{"fixture":"control-plane"}"#).unwrap();
    fs::set_permissions(&auth, fs::Permissions::from_mode(0o600)).unwrap();
    let source = daemon._temp.path().join("project");
    fs::create_dir(&source).unwrap();
    let mut client = Client::connect(&daemon.root);
    let account = match client
        .request(ControlRequest::AccountImport {
            source_path: auth.to_string_lossy().into_owned(),
            display_name: "fixture".to_owned(),
            operation_id: operation(),
        })
        .unwrap()
    {
        ControlResponse::Account(account) => account,
        _ => unreachable!(),
    };
    let project = match client
        .request(ControlRequest::ProjectRegister {
            source_path: source.to_string_lossy().into_owned(),
            name: "fixture".to_owned(),
            operation_id: operation(),
        })
        .unwrap()
    {
        ControlResponse::Project(project) => project,
        _ => unreachable!(),
    };
    assert!(!account.credential_hash.is_empty());
    let sessions =
        daemon.root.join(format!("projects/{}/codex-home/sessions/2026/07", project.project_id));
    fs::create_dir_all(&sessions).unwrap();
    fs::write(sessions.join("thread.jsonl"), format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"thread_fixture\",\"cwd\":{:?}}}}}\nprivate prompt", project.canonical_wsl_path)).unwrap();
    assert!(
        matches!(client.request(ControlRequest::ThreadRefresh { project_id: project.project_id.clone(), operation_id: operation() }).unwrap(), ControlResponse::Threads(values) if values.len() == 1)
    );
    let archived = match client
        .request(ControlRequest::ProjectArchive {
            project_id: project.project_id.clone(),
            operation_id: operation(),
        })
        .unwrap()
    {
        ControlResponse::Project(project) => project,
        _ => unreachable!(),
    };
    assert!(archived.archived_at.is_some());
    assert_eq!(
        client
            .request(ControlRequest::LaunchStart {
                account_id: account.account_id,
                project_id: project.project_id,
                operation_id: operation()
            })
            .unwrap_err(),
        "NOT_FOUND"
    );
    assert!(matches!(
        client.request(ControlRequest::DiagnosticsExport { operation_id: operation() }).unwrap(),
        ControlResponse::Diagnostics(_)
    ));
    assert!(matches!(
        client.request(ControlRequest::SystemShutdown { operation_id: operation() }).unwrap(),
        ControlResponse::Acknowledged
    ));
    assert!(daemon.child.wait().unwrap().success());
}
