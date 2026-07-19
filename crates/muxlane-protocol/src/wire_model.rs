//! Platform-neutral response DTOs used by host clients.
//!
//! Linux binds these JSON shapes directly to `muxlane-core`; non-Linux hosts
//! compile only the protocol DTOs and never link the WSL-only filesystem/lock
//! implementation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransactionState {
    Preparing,
    CheckedOut,
    Running,
    CodexExited,
    CommittingAuth,
    AuthCommitted,
    Cleaned,
    Finished,
    Recovered,
    CredentialConflict,
    Failed,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Account {
    pub account_id: String,
    pub display_name: String,
    pub masked_email: Option<String>,
    pub plan_type: Option<String>,
    pub login_status: String,
    pub credential_hash: String,
    pub occupied: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Project {
    pub project_id: String,
    pub name: String,
    pub canonical_windows_path: Option<String>,
    pub canonical_wsl_path: String,
    pub runtime_relative_path: String,
    pub tmux_session_name: String,
    pub active: bool,
    pub archived_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LaunchView {
    pub launch_id: String,
    pub transaction_id: String,
    pub project_id: String,
    pub account_id: String,
    pub state: TransactionState,
    pub outcome: Option<String>,
    pub started_at: i64,
    pub exited_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RecoveryResult {
    pub transaction_id: String,
    pub previous_state: TransactionState,
    pub state: TransactionState,
    pub classification: String,
    pub changed: bool,
    pub incident_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RecoveryIncident {
    pub incident_id: String,
    pub transaction_id: String,
    pub project_id: String,
    pub account_id: String,
    pub kind: String,
    pub status: String,
    pub evidence_relative_path: Option<String>,
    pub resolution_action: Option<String>,
    pub resolution_summary: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Terminal {
    pub terminal_id: String,
    pub project_id: String,
    pub kind: String,
    pub display_name: String,
    pub tmux_window_identity: String,
    pub lifecycle_status: String,
    pub created_at: i64,
    pub closed_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ThreadIndex {
    pub thread_id: String,
    pub project_id: String,
    pub source_relative_path: String,
    pub source_modified_at: i64,
    pub codex_version: Option<String>,
    pub status: String,
    pub indexed_at: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct UsageSnapshot {
    pub usage_snapshot_id: String,
    pub account_id: String,
    pub codex_version: String,
    pub capability_fingerprint: String,
    pub account_type: Option<String>,
    pub plan_type: Option<String>,
    pub login_status: String,
    pub windows: Vec<UsageWindow>,
    pub lifetime_tokens: Option<u64>,
    pub reset_credit_available: Option<u64>,
    pub captured_at: i64,
    pub expires_at: i64,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct UsageWindow {
    pub duration_minutes: Option<u64>,
    pub used_percent: Option<u64>,
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct UsageRefreshResult {
    pub account_id: String,
    pub snapshot: Option<UsageSnapshot>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CapabilityProbe {
    pub codex_version: String,
    pub schema_fingerprint: String,
    pub account_read: bool,
    pub rate_limits_read: bool,
    pub token_usage_read: bool,
    pub reset_credits: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DiagnosticReceipt {
    pub export_id: String,
    pub relative_path: String,
    pub created_at: i64,
}
