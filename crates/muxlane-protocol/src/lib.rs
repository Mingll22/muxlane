//! Versioned, local-only control protocol for Muxlane components.
//!
//! The formal v1 boundary is intentionally separate from the retained Phase 3
//! POC frame types at the end of this file. New product clients use only the v1
//! JSON-RPC envelopes and capability-negotiated request types.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(target_os = "linux")]
use muxlane_core::{
    diagnostics::DiagnosticReceipt,
    model::{
        Account, CapabilityProbe, LaunchView, Project, RecoveryIncident, RecoveryResult, Terminal,
        ThreadIndex, UsageRefreshResult, UsageSnapshot,
    },
};

#[cfg(not(target_os = "linux"))]
mod wire_model;
#[cfg(not(target_os = "linux"))]
use wire_model::{
    Account, CapabilityProbe, DiagnosticReceipt, LaunchView, Project, RecoveryIncident,
    RecoveryResult, Terminal, ThreadIndex, UsageRefreshResult, UsageSnapshot,
};

pub const PROTOCOL_MAJOR: u16 = 1;
pub const PROTOCOL_MINOR: u16 = 0;
pub const MAX_CONTROL_MESSAGE_BYTES: usize = 128 * 1024;

pub const CAPABILITIES: &[&str] = &[
    "core.read.v1",
    "account.read.v1",
    "account.import.v1",
    "project.read.v1",
    "project.register.v1",
    "project.archive.v1",
    "launch.start.v1",
    "launch.read.v1",
    "recovery.scan.v1",
    "recovery.incident.v1",
    "terminal.read.v1",
    "terminal.create.v1",
    "terminal.history.v1",
    "terminal.close.v1",
    "terminal.data.v1",
    "thread.index.v1",
    "usage.probe.v1",
    "usage.read.v1",
    "usage.refresh.v1",
    "usage.batch.v1",
    "diagnostics.export.v1",
];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: String,
    #[serde(flatten)]
    pub request: ControlRequest,
}

impl RpcRequest {
    pub fn new(id: impl Into<String>, request: ControlRequest) -> Self {
        Self { jsonrpc: "2.0".to_owned(), id: id.into(), request }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: String,
    #[serde(flatten)]
    pub body: RpcResponseBody,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum RpcResponseBody {
    Result { result: ControlResponse },
    Error { error: RpcError },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    pub data: RpcErrorData,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RpcErrorData {
    pub error_code: String,
    pub retryable: bool,
    pub user_action_required: bool,
    pub correlation_id: String,
    pub safe_details: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "method", content = "params")]
pub enum ControlRequest {
    #[serde(rename = "system.handshake")]
    SystemHandshake(HandshakeRequest),
    #[serde(rename = "system.health")]
    SystemHealth,
    #[serde(rename = "system.status")]
    SystemStatus,
    #[serde(rename = "system.shutdown")]
    SystemShutdown { operation_id: String },
    #[serde(rename = "account.list")]
    AccountList,
    #[serde(rename = "account.import")]
    AccountImport { source_path: String, display_name: String, operation_id: String },
    #[serde(rename = "project.list")]
    ProjectList,
    #[serde(rename = "project.register")]
    ProjectRegister { source_path: String, name: String, operation_id: String },
    #[serde(rename = "project.archive")]
    ProjectArchive { project_id: String, operation_id: String },
    #[serde(rename = "launch.start")]
    LaunchStart { account_id: String, project_id: String, operation_id: String },
    #[serde(rename = "launch.list")]
    LaunchList,
    #[serde(rename = "recovery.scan")]
    RecoveryScan { operation_id: String },
    #[serde(rename = "recovery.incident.list")]
    RecoveryIncidentList { include_resolved: bool },
    #[serde(rename = "recovery.incident.resolve")]
    RecoveryIncidentResolve { incident_id: String, action: String, operation_id: String },
    #[serde(rename = "terminal.list")]
    TerminalList { project_id: String },
    #[serde(rename = "terminal.create")]
    TerminalCreate { project_id: String, name: String, operation_id: String },
    #[serde(rename = "terminal.history")]
    TerminalHistory { terminal_id: String },
    #[serde(rename = "terminal.close")]
    TerminalClose { terminal_id: String, operation_id: String },
    #[serde(rename = "thread.refresh")]
    ThreadRefresh { project_id: String, operation_id: String },
    #[serde(rename = "thread.list")]
    ThreadList { project_id: String },
    #[serde(rename = "usage.probe")]
    UsageProbe { account_id: String },
    #[serde(rename = "usage.read")]
    UsageRead { account_id: String },
    #[serde(rename = "usage.refresh")]
    UsageRefresh { account_id: String, operation_id: String },
    #[serde(rename = "usage.refresh_batch")]
    UsageRefreshBatch { account_ids: Vec<String>, operation_id: String },
    #[serde(rename = "diagnostics.export")]
    DiagnosticsExport { operation_id: String },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HandshakeRequest {
    pub protocol_major: u16,
    pub protocol_minor: u16,
    pub client_name: String,
    pub client_version: String,
    pub requested_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ControlResponse {
    Handshake(HandshakeResponse),
    Health(HealthResponse),
    Status(StatusResponse),
    Accounts(Vec<Account>),
    Account(Account),
    Projects(Vec<Project>),
    Project(Project),
    Launch(LaunchView),
    Launches(Vec<LaunchView>),
    Recovery(Vec<RecoveryResult>),
    RecoveryIncidents(Vec<RecoveryIncident>),
    RecoveryIncident(RecoveryIncident),
    Terminals(Vec<Terminal>),
    Terminal(Terminal),
    TerminalHistory { terminal_id: String, bytes: Vec<u8>, truncated: bool },
    Threads(Vec<ThreadIndex>),
    UsageProbe(CapabilityProbe),
    Usage(Option<UsageSnapshot>),
    UsageBatch(Vec<UsageRefreshResult>),
    Diagnostics(DiagnosticReceipt),
    Acknowledged,
}

pub const TERMINAL_DATA_PROTOCOL_MAJOR: u16 = 1;
pub const TERMINAL_DATA_PROTOCOL_MINOR: u16 = 0;
pub const MAX_TERMINAL_DATA_MESSAGE_BYTES: usize = 128 * 1024;

/// Formal, stdio terminal data-plane request. It carries no shell command,
/// executable, path, tmux session name, window target, or pane target.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TerminalDataRequestEnvelope {
    pub id: u64,
    pub request: TerminalDataRequest,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "method", content = "params")]
pub enum TerminalDataRequest {
    #[serde(rename = "terminal.handshake")]
    Handshake { protocol_major: u16, protocol_minor: u16, client_name: String },
    #[serde(rename = "terminal.attach")]
    Attach { terminal_id: String },
    #[serde(rename = "terminal.stream.start")]
    StartStream { stream: TerminalStream },
    #[serde(rename = "terminal.detach")]
    Detach { stream: TerminalStream },
    #[serde(rename = "terminal.switch")]
    Switch { terminal_id: String },
    #[serde(rename = "terminal.input")]
    SendInput { stream: TerminalStream, bytes: Vec<u8> },
    #[serde(rename = "terminal.resize")]
    Resize { stream: TerminalStream, columns: u16, rows: u16 },
    #[serde(rename = "terminal.close")]
    Close { terminal_id: String },
    #[serde(rename = "terminal.state")]
    ReadState,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TerminalStream {
    pub connection_id: String,
    pub attachment_id: u64,
    pub bootstrap_id: u64,
    pub project_id: String,
    pub terminal_id: String,
    pub window_id: String,
    pub pane_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalDataResponse {
    Handshake {
        protocol_major: u16,
        protocol_minor: u16,
        connection_id: String,
        tmux_version: String,
        max_message_bytes: usize,
    },
    Attached {
        stream: TerminalStream,
    },
    StreamStarted {
        stream: TerminalStream,
    },
    Detached,
    State {
        attached: Option<TerminalStream>,
    },
    Closed {
        terminal_id: String,
    },
    Acknowledged,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TerminalDataError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalDataEvent {
    History { stream: TerminalStream, sequence: u64, bytes: Vec<u8> },
    Output { stream: TerminalStream, sequence: u64, bytes: Vec<u8> },
    StreamClosed { stream: TerminalStream, sequence: u64 },
    StreamError { stream: TerminalStream, sequence: u64, code: String },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum TerminalDataFrame {
    Response { id: u64, result: TerminalDataResult },
    Event { event: TerminalDataEvent },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TerminalDataResult {
    Ok { response: TerminalDataResponse },
    Error { error: TerminalDataError },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HandshakeResponse {
    pub protocol_major: u16,
    pub protocol_minor: u16,
    pub daemon_version: String,
    pub daemon_instance_id: String,
    pub granted_capabilities: Vec<String>,
    pub max_control_message_bytes: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HealthResponse {
    pub healthy: bool,
    pub database_integrity: String,
    pub schema_version: u32,
    pub daemon_instance_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct StatusResponse {
    pub daemon_instance_id: String,
    pub accounts: u64,
    pub projects: u64,
    pub launches: u64,
    pub recovery_incidents: u64,
    pub active_launches: u64,
}

/// Stable package identifier for build and integration checks.
pub const CRATE_IDENTIFIER: &str = "muxlane-protocol";

/// A request sent over the POC's local stdio bridge.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Phase3Request {
    Probe,
    CreateSyntheticSession { project_id: String },
    ListManagedSessions,
    CreateWindow { project_id: String, name: String },
    ListWindows { project_id: String },
    Attach { project_id: String, window_id: String },
    StartStream { stream: AttachedTerminal },
    Detach { stream: AttachedTerminal },
    SendInput { stream: AttachedTerminal, bytes: Vec<u8> },
    Resize { stream: AttachedTerminal, columns: u16, rows: u16 },
    CloseWindow { project_id: String, window_id: String },
    CleanupSession { project_id: String },
    ReadState,
}

/// A request envelope permits the host to match a structured response.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Phase3RequestEnvelope {
    pub id: u64,
    pub request: Phase3Request,
}

/// A managed session. The name is generated by the gateway, never supplied as a tmux target.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ManagedSession {
    pub project_id: String,
    pub session_name: String,
    pub session_id: String,
}

/// A window discovered from a session that has already passed managed-session validation.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ManagedWindow {
    pub id: String,
    pub name: String,
    pub active: bool,
}

/// Control-plane results for the POC bridge.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Phase3Response {
    Probe { connection_id: String, tmux_version: String },
    Sessions { sessions: Vec<ManagedSession> },
    Windows { windows: Vec<ManagedWindow> },
    Attached { stream: AttachedTerminal },
    StreamStarted { stream: AttachedTerminal },
    Detached,
    State { attached: Option<AttachedTerminal> },
    Acknowledged,
}

/// The current data-plane attachment, if any.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AttachedTerminal {
    pub connection_id: String,
    pub attachment_id: u64,
    pub bootstrap_id: u64,
    pub project_id: String,
    pub window_id: String,
    pub pane_id: String,
}

/// Structured failures intentionally omit child-process stderr and terminal content.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Phase3Error {
    pub code: String,
    pub message: String,
}

/// Data-plane events; output is raw bytes to avoid conflating UTF-8 with terminal bytes.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Phase3Event {
    History { stream: AttachedTerminal, sequence: u64, bytes: Vec<u8> },
    Output { stream: AttachedTerminal, sequence: u64, bytes: Vec<u8> },
    StreamClosed { stream: AttachedTerminal, sequence: u64 },
    StreamError { stream: AttachedTerminal, sequence: u64, code: String },
    ConnectionClosed { connection_id: String },
}

/// Every stdout frame is a typed response or terminal event, never a free-form command.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum Phase3Frame {
    Response {
        id: u64,
        #[serde(flatten)]
        result: ResultFrame,
    },
    Event {
        event: Phase3Event,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResultFrame {
    Ok { response: Phase3Response },
    Error { error: Phase3Error },
}

#[cfg(test)]
mod tests {
    use super::{Phase3Frame, Phase3Request, Phase3RequestEnvelope};

    #[test]
    fn phase3_poc_frames_are_typed_and_round_trip() {
        let stream = super::AttachedTerminal {
            connection_id: "connection-7".to_owned(),
            attachment_id: 3,
            bootstrap_id: 4,
            project_id: "project-a".to_owned(),
            window_id: "@1".to_owned(),
            pane_id: "%1".to_owned(),
        };
        let request = Phase3RequestEnvelope {
            id: 7,
            request: Phase3Request::Resize { stream: stream.clone(), columns: 120, rows: 40 },
        };
        let encoded = serde_json::to_string(&request).expect("serializes test request");
        let decoded: Phase3RequestEnvelope =
            serde_json::from_str(&encoded).expect("deserializes test request");
        assert_eq!(decoded, request);

        let malformed = r#"{\"id\":1,\"request\":{\"kind\":\"execute\",\"command\":\"id\"}}"#;
        assert!(serde_json::from_str::<Phase3RequestEnvelope>(malformed).is_err());
        assert!(
            serde_json::to_string(&Phase3Frame::Event {
                event: super::Phase3Event::StreamClosed { stream, sequence: 9 },
            })
            .is_ok()
        );
    }
}
