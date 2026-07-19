//! Stable JSON CLI for Muxlane diagnostics, recovery, and runtime control.

#![forbid(unsafe_code)]

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use clap::{Parser, Subcommand};
use muxlane_core::layout::Layout;
use muxlane_protocol::{
    CAPABILITIES, ControlRequest, ControlResponse, HandshakeRequest, PROTOCOL_MAJOR,
    PROTOCOL_MINOR, RpcRequest, RpcResponse, RpcResponseBody,
};
use serde_json::json;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "muxlane", version, about = "Muxlane runtime diagnostics and recovery client")]
struct Cli {
    #[command(subcommand)]
    command: TopLevel,
}

#[derive(Debug, Subcommand)]
enum TopLevel {
    #[command(hide = true)]
    Control {
        request_json: String,
    },
    Doctor,
    Status,
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Account {
        #[command(subcommand)]
        command: AccountCommand,
    },
    Launch {
        #[command(subcommand)]
        command: LaunchCommand,
    },
    Terminal {
        #[command(subcommand)]
        command: TerminalCommand,
    },
    Thread {
        #[command(subcommand)]
        command: ThreadCommand,
    },
    Incident {
        #[command(subcommand)]
        command: IncidentCommand,
    },
    Usage {
        #[command(subcommand)]
        command: UsageCommand,
    },
    Recover,
    Diagnostics {
        #[command(subcommand)]
        command: DiagnosticsCommand,
    },
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    Start,
    Stop,
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    List,
    Register { path: PathBuf, name: String },
    Archive { project_id: String },
}

#[derive(Debug, Subcommand)]
enum AccountCommand {
    List,
    Import { auth_json: PathBuf, name: String },
}

#[derive(Debug, Subcommand)]
enum LaunchCommand {
    List,
    Start { account_id: String, project_id: String },
}

#[derive(Debug, Subcommand)]
enum TerminalCommand {
    List { project_id: String },
    Create { project_id: String, name: String },
    History { terminal_id: String },
    Close { terminal_id: String },
}

#[derive(Debug, Subcommand)]
enum ThreadCommand {
    List { project_id: String },
    Refresh { project_id: String },
}

#[derive(Debug, Subcommand)]
enum IncidentCommand {
    List {
        #[arg(long)]
        include_resolved: bool,
    },
    Resolve {
        incident_id: String,
        action: String,
    },
}

#[derive(Debug, Subcommand)]
enum UsageCommand {
    Probe { account_id: String },
    Read { account_id: String },
    Refresh { account_id: String },
    RefreshBatch { account_ids: Vec<String> },
}

#[derive(Debug, Subcommand)]
enum DiagnosticsCommand {
    Export,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        println!("{}", json!({"status":"error","error_code":error}));
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    if matches!(cli.command, TopLevel::Daemon { command: DaemonCommand::Start }) {
        return start_daemon();
    }
    let request = match cli.command {
        TopLevel::Control { request_json } => {
            serde_json::from_str(&request_json).map_err(|_| "INVALID_REQUEST".to_owned())?
        }
        TopLevel::Doctor => ControlRequest::SystemHealth,
        TopLevel::Status => ControlRequest::SystemStatus,
        TopLevel::Daemon { command: DaemonCommand::Stop } => {
            ControlRequest::SystemShutdown { operation_id: operation_id() }
        }
        TopLevel::Daemon { command: DaemonCommand::Start } => unreachable!(),
        TopLevel::Project { command: ProjectCommand::List } => ControlRequest::ProjectList,
        TopLevel::Project { command: ProjectCommand::Register { path, name } } => {
            ControlRequest::ProjectRegister {
                source_path: absolute(path)?.to_string_lossy().into_owned(),
                name,
                operation_id: operation_id(),
            }
        }
        TopLevel::Project { command: ProjectCommand::Archive { project_id } } => {
            ControlRequest::ProjectArchive { project_id, operation_id: operation_id() }
        }
        TopLevel::Account { command: AccountCommand::List } => ControlRequest::AccountList,
        TopLevel::Account { command: AccountCommand::Import { auth_json, name } } => {
            ControlRequest::AccountImport {
                source_path: absolute(auth_json)?.to_string_lossy().into_owned(),
                display_name: name,
                operation_id: operation_id(),
            }
        }
        TopLevel::Launch { command: LaunchCommand::List } => ControlRequest::LaunchList,
        TopLevel::Launch { command: LaunchCommand::Start { account_id, project_id } } => {
            ControlRequest::LaunchStart { account_id, project_id, operation_id: operation_id() }
        }
        TopLevel::Terminal { command: TerminalCommand::List { project_id } } => {
            ControlRequest::TerminalList { project_id }
        }
        TopLevel::Terminal { command: TerminalCommand::Create { project_id, name } } => {
            ControlRequest::TerminalCreate { project_id, name, operation_id: operation_id() }
        }
        TopLevel::Terminal { command: TerminalCommand::History { terminal_id } } => {
            ControlRequest::TerminalHistory { terminal_id }
        }
        TopLevel::Terminal { command: TerminalCommand::Close { terminal_id } } => {
            ControlRequest::TerminalClose { terminal_id, operation_id: operation_id() }
        }
        TopLevel::Thread { command: ThreadCommand::List { project_id } } => {
            ControlRequest::ThreadList { project_id }
        }
        TopLevel::Thread { command: ThreadCommand::Refresh { project_id } } => {
            ControlRequest::ThreadRefresh { project_id, operation_id: operation_id() }
        }
        TopLevel::Incident { command: IncidentCommand::List { include_resolved } } => {
            ControlRequest::RecoveryIncidentList { include_resolved }
        }
        TopLevel::Incident { command: IncidentCommand::Resolve { incident_id, action } } => {
            ControlRequest::RecoveryIncidentResolve {
                incident_id,
                action,
                operation_id: operation_id(),
            }
        }
        TopLevel::Usage { command: UsageCommand::Probe { account_id } } => {
            ControlRequest::UsageProbe { account_id }
        }
        TopLevel::Usage { command: UsageCommand::Read { account_id } } => {
            ControlRequest::UsageRead { account_id }
        }
        TopLevel::Usage { command: UsageCommand::Refresh { account_id } } => {
            ControlRequest::UsageRefresh { account_id, operation_id: operation_id() }
        }
        TopLevel::Usage { command: UsageCommand::RefreshBatch { account_ids } } => {
            ControlRequest::UsageRefreshBatch { account_ids, operation_id: operation_id() }
        }
        TopLevel::Recover => ControlRequest::RecoveryScan { operation_id: operation_id() },
        TopLevel::Diagnostics { command: DiagnosticsCommand::Export } => {
            ControlRequest::DiagnosticsExport { operation_id: operation_id() }
        }
    };
    let response = Client::connect()?.request(request)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"status":"ok","result":response}))
            .map_err(|_| "INTERNAL_ERROR".to_owned())?
    );
    Ok(())
}

struct Client {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
    next_id: u64,
}

impl Client {
    fn connect() -> Result<Self, String> {
        let root = Layout::discover_root().map_err(|error| error.code.to_owned())?;
        let stream = UnixStream::connect(root.join("run/muxlaned.sock"))
            .map_err(|_| "DAEMON_UNAVAILABLE".to_owned())?;
        let mut client = Self {
            writer: stream.try_clone().map_err(|_| "DAEMON_UNAVAILABLE".to_owned())?,
            reader: BufReader::new(stream),
            next_id: 1,
        };
        let response = client.request_raw(ControlRequest::SystemHandshake(HandshakeRequest {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            client_name: "muxlane_cli".to_owned(),
            client_version: env!("CARGO_PKG_VERSION").to_owned(),
            requested_capabilities: CAPABILITIES.iter().map(|value| (*value).to_owned()).collect(),
        }))?;
        if !matches!(response, ControlResponse::Handshake(_)) {
            return Err("PROTOCOL_INCOMPATIBLE".to_owned());
        }
        Ok(client)
    }

    fn request(&mut self, request: ControlRequest) -> Result<ControlResponse, String> {
        self.request_raw(request)
    }

    fn request_raw(&mut self, request: ControlRequest) -> Result<ControlResponse, String> {
        let id = format!("req_{}", self.next_id);
        self.next_id += 1;
        serde_json::to_writer(&mut self.writer, &RpcRequest::new(&id, request))
            .map_err(|_| "INTERNAL_ERROR".to_owned())?;
        self.writer.write_all(b"\n").map_err(|_| "DAEMON_UNAVAILABLE".to_owned())?;
        self.writer.flush().map_err(|_| "DAEMON_UNAVAILABLE".to_owned())?;
        let mut line = String::new();
        self.reader.read_line(&mut line).map_err(|_| "DAEMON_UNAVAILABLE".to_owned())?;
        let response: RpcResponse = serde_json::from_str(line.trim_end())
            .map_err(|_| "PROTOCOL_INCOMPATIBLE".to_owned())?;
        if response.id != id {
            return Err("PROTOCOL_INCOMPATIBLE".to_owned());
        }
        match response.body {
            RpcResponseBody::Result { result } => Ok(result),
            RpcResponseBody::Error { error } => Err(error.data.error_code),
        }
    }
}

fn start_daemon() -> Result<(), String> {
    if let Ok(mut client) = Client::connect() {
        let response = client.request(ControlRequest::SystemHealth)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"status":"ok","result":response})).unwrap()
        );
        return Ok(());
    }
    let current = std::env::current_exe().map_err(|_| "DAEMON_UNAVAILABLE".to_owned())?;
    let daemon = current.parent().ok_or_else(|| "DAEMON_UNAVAILABLE".to_owned())?.join("muxlaned");
    Command::new(daemon)
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "DAEMON_UNAVAILABLE".to_owned())?;
    for _ in 0..100 {
        thread::sleep(Duration::from_millis(50));
        if let Ok(mut client) = Client::connect() {
            let response = client.request(ControlRequest::SystemHealth)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"status":"ok","result":response})).unwrap()
            );
            return Ok(());
        }
    }
    Err("DAEMON_UNAVAILABLE".to_owned())
}

fn absolute(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .map_err(|_| "PATH_REJECTED".to_owned())
    }
}

fn operation_id() -> String {
    format!("operation_{}", Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn exposes_required_runtime_commands() {
        let help = Cli::command().render_long_help().to_string();
        for command in
            ["doctor", "status", "daemon", "project", "account", "recover", "diagnostics"]
        {
            assert!(help.contains(command), "missing command {command}");
        }
    }
}
