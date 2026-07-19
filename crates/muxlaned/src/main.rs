//! Muxlane's single-instance WSL runtime control-plane daemon.

#![forbid(unsafe_code)]

mod phase3;
mod terminal_data;

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use clap::{Parser, Subcommand};
use muxlane_core::{
    CoreError, credential, diagnostics, incident,
    layout::{Layout, hex_sha256, validate_id},
    lock::ManagedLock,
    recovery, service, session,
    storage::{OperationClaim, Storage},
    terminal, usage,
};
use muxlane_protocol::{
    CAPABILITIES, ControlRequest, ControlResponse, HandshakeResponse, HealthResponse,
    MAX_CONTROL_MESSAGE_BYTES, PROTOCOL_MAJOR, PROTOCOL_MINOR, RpcError, RpcErrorData, RpcRequest,
    RpcResponse, RpcResponseBody, StatusResponse,
};
use nix::{
    sys::socket::{getsockopt, sockopt::PeerCredentials},
    unistd::Uid,
};
use serde_json::json;
use uuid::Uuid;

const MAX_CONTROL_CONNECTIONS: usize = 64;

#[derive(Debug, Parser)]
#[command(name = "muxlaned", version, about = "Muxlane WSL runtime control-plane daemon")]
struct DaemonCli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the single-instance local control server.
    Serve,
    /// Internal tmux entrypoint for one durable Launch Transaction.
    #[command(hide = true)]
    ManagedRunner {
        #[arg(long)]
        transaction_id: String,
    },
    /// Run one formal, bounded Terminal data-plane connection over stdio.
    TerminalGateway,
    /// Retained compatibility surface for the explicitly non-production Phase 3 POC.
    Phase3 {
        #[command(subcommand)]
        command: Phase3Command,
    },
}

#[derive(Debug, Subcommand)]
enum Phase3Command {
    Gateway {
        #[arg(long, default_value = "muxlane-p3")]
        socket: String,
    },
    SyntheticRunner {
        #[arg(long, default_value = "synthetic")]
        label: String,
    },
}

#[derive(Clone)]
struct ServerState {
    storage: Storage,
    instance_id: String,
    shutdown: Arc<AtomicBool>,
}

fn main() {
    let result = match DaemonCli::parse().command {
        Command::Serve => run_server(),
        Command::ManagedRunner { transaction_id } => run_managed_runner(&transaction_id),
        Command::TerminalGateway => terminal_data::run_gateway(),
        Command::Phase3 { command: Phase3Command::Gateway { socket } } => {
            phase3::run_gateway(socket).map_err(|error| {
                CoreError::new("INTERNAL_ERROR", format!("{}: {}", error.code, error.message))
            })
        }
        Command::Phase3 { command: Phase3Command::SyntheticRunner { label } } => {
            phase3::run_synthetic_runner(label).map_err(CoreError::io)
        }
    };
    if let Err(error) = result {
        eprintln!("{}: {}", error.code, error.message);
        std::process::exit(1);
    }
}

fn run_server() -> Result<(), CoreError> {
    let layout = Layout::discover()?;
    let _instance_lock = ManagedLock::try_acquire(&layout.daemon_lock(), "CONFLICT")?;
    let storage = Storage::open(layout.clone())?;
    let startup_recovery = recovery::recover_all(&storage)?;
    if !startup_recovery.is_empty() {
        diagnostics::append_event(&storage, "startup_recovery", None)?;
    }
    let socket = layout.socket();
    if socket.exists() {
        let metadata = fs::symlink_metadata(&socket)?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_socket() {
            return Err(CoreError::new("PATH_REJECTED", "daemon socket path is unsafe"));
        }
        fs::remove_file(&socket)?;
    }
    let listener = UnixListener::bind(&socket)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let state = ServerState {
        storage,
        instance_id: format!("daemon_{}", Uuid::new_v4().simple()),
        shutdown: Arc::new(AtomicBool::new(false)),
    };
    diagnostics::append_event(&state.storage, "daemon_started", Some(&state.instance_id))?;
    let active_connections = Arc::new(AtomicUsize::new(0));
    while !state.shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                if active_connections
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                        (count < MAX_CONTROL_CONNECTIONS).then_some(count + 1)
                    })
                    .is_err()
                {
                    drop(stream);
                    continue;
                }
                let state = state.clone();
                let active_connections = Arc::clone(&active_connections);
                thread::spawn(move || {
                    let _ = handle_connection(stream, state);
                    active_connections.fetch_sub(1, Ordering::SeqCst);
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(error.into()),
        }
    }
    diagnostics::append_event(&state.storage, "daemon_stopped", Some(&state.instance_id))?;
    drop(listener);
    fs::remove_file(&socket)?;
    Ok(())
}

fn handle_connection(stream: UnixStream, state: ServerState) -> Result<(), CoreError> {
    let credentials = getsockopt(&stream, PeerCredentials)
        .map_err(|_| CoreError::new("PERMISSION_DENIED", "local peer identity is unavailable"))?;
    if credentials.uid() != Uid::current().as_raw() {
        return Err(CoreError::new("PERMISSION_DENIED", "local peer identity is not authorized"));
    }
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    let mut handshaken = false;
    loop {
        let Some(line) = read_control_line(&mut reader)? else {
            return Ok(());
        };
        let request: RpcRequest = match serde_json::from_str::<RpcRequest>(line.trim_end()) {
            Ok(request) if request.jsonrpc == "2.0" && !request.id.is_empty() => request,
            _ => {
                write_response(
                    &mut writer,
                    error_response(
                        "invalid",
                        CoreError::new("INVALID_REQUEST", "invalid JSON-RPC request"),
                    ),
                )?;
                continue;
            }
        };
        if !handshaken && !matches!(request.request, ControlRequest::SystemHandshake(_)) {
            write_response(
                &mut writer,
                error_response(
                    &request.id,
                    CoreError::new(
                        "PERMISSION_DENIED",
                        "system.handshake must be the first request",
                    ),
                ),
            )?;
            continue;
        }
        let response = match dispatch(&state, &request.request) {
            Ok(response) => {
                if matches!(request.request, ControlRequest::SystemHandshake(_)) {
                    handshaken = true;
                }
                success_response(&request.id, response)
            }
            Err(error) => error_response(&request.id, error),
        };
        write_response(&mut writer, response)?;
    }
}

fn read_control_line(reader: &mut impl BufRead) -> Result<Option<String>, CoreError> {
    let mut line = String::new();
    let count = reader.take((MAX_CONTROL_MESSAGE_BYTES + 1) as u64).read_line(&mut line)?;
    if count == 0 {
        return Ok(None);
    }
    if count > MAX_CONTROL_MESSAGE_BYTES {
        return Err(CoreError::new("INVALID_REQUEST", "control message exceeds negotiated limit"));
    }
    Ok(Some(line))
}

fn dispatch(state: &ServerState, request: &ControlRequest) -> Result<ControlResponse, CoreError> {
    match request {
        ControlRequest::SystemHandshake(handshake) => {
            if handshake.protocol_major != PROTOCOL_MAJOR {
                return Err(CoreError::new(
                    "PROTOCOL_INCOMPATIBLE",
                    "protocol major versions are incompatible",
                ));
            }
            let granted_capabilities = handshake
                .requested_capabilities
                .iter()
                .filter(|capability| CAPABILITIES.contains(&capability.as_str()))
                .cloned()
                .collect();
            Ok(ControlResponse::Handshake(HandshakeResponse {
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
                daemon_instance_id: state.instance_id.clone(),
                granted_capabilities,
                max_control_message_bytes: MAX_CONTROL_MESSAGE_BYTES,
            }))
        }
        ControlRequest::SystemHealth => Ok(ControlResponse::Health(HealthResponse {
            healthy: true,
            database_integrity: state.storage.integrity()?,
            schema_version: state.storage.schema_version()?,
            daemon_instance_id: state.instance_id.clone(),
        })),
        ControlRequest::SystemStatus => {
            let (accounts, projects, launches, recovery_incidents) = state.storage.counts()?;
            let active_launches = state
                .storage
                .list_launches()?
                .into_iter()
                .filter(|launch| !launch.state.terminal())
                .count() as u64;
            Ok(ControlResponse::Status(StatusResponse {
                daemon_instance_id: state.instance_id.clone(),
                accounts,
                projects,
                launches,
                recovery_incidents,
                active_launches,
            }))
        }
        ControlRequest::SystemShutdown { operation_id } => {
            validate_operation(operation_id)?;
            state.shutdown.store(true, Ordering::SeqCst);
            Ok(ControlResponse::Acknowledged)
        }
        ControlRequest::AccountList => {
            Ok(ControlResponse::Accounts(state.storage.list_accounts()?))
        }
        ControlRequest::AccountImport { source_path, display_name, operation_id } => idempotent(
            state,
            operation_id,
            "account.import",
            json!({"source_path":source_path,"display_name":display_name}),
            || {
                let account = credential::import_account(
                    &state.storage,
                    &PathBuf::from(source_path),
                    display_name,
                )?;
                let _ = diagnostics::append_event(
                    &state.storage,
                    "account_imported",
                    Some(&account.account_id),
                );
                Ok(ControlResponse::Account(account))
            },
        ),
        ControlRequest::ProjectList => {
            Ok(ControlResponse::Projects(state.storage.list_projects()?))
        }
        ControlRequest::ProjectRegister { source_path, name, operation_id } => idempotent(
            state,
            operation_id,
            "project.register",
            json!({"source_path":source_path,"name":name}),
            || {
                let project =
                    service::register_project(&state.storage, &PathBuf::from(source_path), name)?;
                let _ = diagnostics::append_event(
                    &state.storage,
                    "project_registered",
                    Some(&project.project_id),
                );
                Ok(ControlResponse::Project(project))
            },
        ),
        ControlRequest::ProjectArchive { project_id, operation_id } => idempotent(
            state,
            operation_id,
            "project.archive",
            json!({"project_id":project_id}),
            || Ok(ControlResponse::Project(service::archive_project(&state.storage, project_id)?)),
        ),
        ControlRequest::LaunchStart { account_id, project_id, operation_id } => idempotent(
            state,
            operation_id,
            "launch.start",
            json!({"account_id":account_id,"project_id":project_id}),
            || {
                let transaction = service::prepare_launch(&state.storage, account_id, project_id)?;
                let executable = service::resolve_executable_for_runner()?;
                if let Err(error) = terminal::start_managed_runner(
                    &state.storage,
                    &transaction.transaction_id,
                    &executable,
                ) {
                    state.storage.fail(&transaction.transaction_id, error.code, &error.message)?;
                    return Err(error);
                }
                let _ = diagnostics::append_event(
                    &state.storage,
                    "launch_started",
                    Some(&transaction.launch_id),
                );
                let launch = state
                    .storage
                    .list_launches()?
                    .into_iter()
                    .find(|launch| launch.launch_id == transaction.launch_id)
                    .ok_or_else(|| {
                        CoreError::new("INTERNAL_ERROR", "Launch view is unavailable")
                    })?;
                Ok(ControlResponse::Launch(launch))
            },
        ),
        ControlRequest::LaunchList => Ok(ControlResponse::Launches(state.storage.list_launches()?)),
        ControlRequest::RecoveryScan { operation_id } => {
            idempotent(state, operation_id, "recovery.scan", json!({}), || {
                Ok(ControlResponse::Recovery(recovery::recover_all(&state.storage)?))
            })
        }
        ControlRequest::RecoveryIncidentList { include_resolved } => {
            Ok(ControlResponse::RecoveryIncidents(state.storage.list_incidents(*include_resolved)?))
        }
        ControlRequest::RecoveryIncidentResolve { incident_id, action, operation_id } => {
            idempotent(
                state,
                operation_id,
                "recovery.incident.resolve",
                json!({"incident_id":incident_id,"action":action}),
                || {
                    Ok(ControlResponse::RecoveryIncident(incident::resolve(
                        &state.storage,
                        incident_id,
                        action,
                    )?))
                },
            )
        }
        ControlRequest::TerminalList { project_id } => {
            validate_id(project_id)?;
            Ok(ControlResponse::Terminals(state.storage.list_terminals(project_id)?))
        }
        ControlRequest::TerminalCreate { project_id, name, operation_id } => idempotent(
            state,
            operation_id,
            "terminal.create",
            json!({"project_id":project_id,"name":name}),
            || {
                Ok(ControlResponse::Terminal(terminal::create_auxiliary(
                    &state.storage,
                    project_id,
                    name,
                )?))
            },
        ),
        ControlRequest::TerminalHistory { terminal_id } => {
            let bytes = terminal::history_bootstrap(&state.storage, terminal_id)?;
            Ok(ControlResponse::TerminalHistory {
                terminal_id: terminal_id.clone(),
                bytes,
                truncated: false,
            })
        }
        ControlRequest::TerminalClose { terminal_id, operation_id } => idempotent(
            state,
            operation_id,
            "terminal.close",
            json!({"terminal_id":terminal_id}),
            || Ok(ControlResponse::Terminal(terminal::close(&state.storage, terminal_id)?)),
        ),
        ControlRequest::ThreadRefresh { project_id, operation_id } => idempotent(
            state,
            operation_id,
            "thread.refresh",
            json!({"project_id":project_id}),
            || Ok(ControlResponse::Threads(session::refresh(&state.storage, project_id)?)),
        ),
        ControlRequest::ThreadList { project_id } => {
            validate_id(project_id)?;
            state.storage.project(project_id)?;
            Ok(ControlResponse::Threads(state.storage.list_thread_indexes(project_id)?))
        }
        ControlRequest::UsageProbe { account_id } => {
            Ok(ControlResponse::UsageProbe(usage::probe_capabilities(&state.storage, account_id)?))
        }
        ControlRequest::UsageRead { account_id } => {
            validate_id(account_id)?;
            Ok(ControlResponse::Usage(state.storage.latest_usage(account_id)?))
        }
        ControlRequest::UsageRefresh { account_id, operation_id } => idempotent(
            state,
            operation_id,
            "usage.refresh",
            json!({"account_id":account_id}),
            || Ok(ControlResponse::Usage(Some(usage::refresh_usage(&state.storage, account_id)?))),
        ),
        ControlRequest::UsageRefreshBatch { account_ids, operation_id } => idempotent(
            state,
            operation_id,
            "usage.refresh_batch",
            json!({"account_ids":account_ids}),
            || Ok(ControlResponse::UsageBatch(usage::refresh_batch(&state.storage, account_ids)?)),
        ),
        ControlRequest::DiagnosticsExport { operation_id } => {
            idempotent(state, operation_id, "diagnostics.export", json!({}), || {
                Ok(ControlResponse::Diagnostics(diagnostics::export(&state.storage)?))
            })
        }
    }
}

fn idempotent(
    state: &ServerState,
    operation_id: &str,
    method: &str,
    semantic_request: serde_json::Value,
    execute: impl FnOnce() -> Result<ControlResponse, CoreError>,
) -> Result<ControlResponse, CoreError> {
    validate_operation(operation_id)?;
    let request_hash = hex_sha256(&serde_json::to_vec(&semantic_request)?);
    match state.storage.claim_operation(operation_id, method, &request_hash)? {
        OperationClaim::Completed(response) => serde_json::from_str(&response)
            .map_err(|_| CoreError::new("STORAGE_FAILURE", "stored operation result is invalid")),
        OperationClaim::InProgress => Err(CoreError::new(
            "RECOVERY_REQUIRED",
            "operation is incomplete and requires recovery",
        )),
        OperationClaim::New => {
            let response = execute()?;
            state.storage.complete_operation(operation_id, &serde_json::to_string(&response)?)?;
            Ok(response)
        }
    }
}

fn run_managed_runner(transaction_id: &str) -> Result<(), CoreError> {
    validate_id(transaction_id)?;
    let storage = Storage::open(Layout::discover()?)?;
    let exit_code = service::run_managed_launch(&storage, transaction_id)?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

fn validate_operation(operation_id: &str) -> Result<(), CoreError> {
    validate_id(operation_id)?;
    if !operation_id.starts_with("operation_") {
        return Err(CoreError::new("INVALID_REQUEST", "operation identifier is invalid"));
    }
    Ok(())
}

fn success_response(id: &str, result: ControlResponse) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0".to_owned(),
        id: id.to_owned(),
        body: RpcResponseBody::Result { result },
    }
}

fn error_response(id: &str, error: CoreError) -> RpcResponse {
    let (retryable, user_action_required) = match error.code {
        "ACCOUNT_IN_USE" | "PROJECT_IN_USE" | "LOCKED" => (true, false),
        "CREDENTIAL_CONFLICT" | "RECOVERY_REQUIRED" | "PROCESS_IDENTITY_UNCONFIRMED" => {
            (false, true)
        }
        _ => (false, false),
    };
    RpcResponse {
        jsonrpc: "2.0".to_owned(),
        id: id.to_owned(),
        body: RpcResponseBody::Error {
            error: RpcError {
                code: -32041,
                message: "The requested operation cannot proceed safely.".to_owned(),
                data: RpcErrorData {
                    error_code: error.code.to_owned(),
                    retryable,
                    user_action_required,
                    correlation_id: format!("corr_{}", Uuid::new_v4().simple()),
                    safe_details: json!({}),
                },
            },
        },
    }
}

fn write_response(writer: &mut UnixStream, response: RpcResponse) -> Result<(), CoreError> {
    serde_json::to_writer(&mut *writer, &response)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use muxlane_protocol::HandshakeRequest;

    use super::*;

    #[test]
    fn handshake_rejects_incompatible_major_and_negotiates_capabilities() {
        let temp = tempfile::tempdir().unwrap();
        let state = ServerState {
            storage: Storage::open(Layout::initialize(temp.path().join("muxlane")).unwrap())
                .unwrap(),
            instance_id: "daemon_fixture".to_owned(),
            shutdown: Arc::new(AtomicBool::new(false)),
        };
        let incompatible = ControlRequest::SystemHandshake(HandshakeRequest {
            protocol_major: 99,
            protocol_minor: 0,
            client_name: "test".to_owned(),
            client_version: "1".to_owned(),
            requested_capabilities: vec![],
        });
        assert_eq!(dispatch(&state, &incompatible).unwrap_err().code, "PROTOCOL_INCOMPATIBLE");
        let compatible = ControlRequest::SystemHandshake(HandshakeRequest {
            protocol_major: 1,
            protocol_minor: 99,
            client_name: "test".to_owned(),
            client_version: "1".to_owned(),
            requested_capabilities: vec!["core.read.v1".to_owned(), "future.v9".to_owned()],
        });
        let ControlResponse::Handshake(response) = dispatch(&state, &compatible).unwrap() else {
            panic!("expected handshake")
        };
        assert_eq!(response.protocol_minor, PROTOCOL_MINOR);
        assert_eq!(response.granted_capabilities, ["core.read.v1"]);
    }

    #[test]
    fn control_line_reader_rejects_before_unbounded_allocation() {
        let oversized = vec![b'a'; MAX_CONTROL_MESSAGE_BYTES + 1];
        let error = read_control_line(&mut Cursor::new(oversized)).unwrap_err();
        assert_eq!(error.code, "INVALID_REQUEST");
        assert!(read_control_line(&mut Cursor::new(b"{}\n")).unwrap().is_some());
    }
}
