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

impl TransactionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::CheckedOut => "checked_out",
            Self::Running => "running",
            Self::CodexExited => "codex_exited",
            Self::CommittingAuth => "committing_auth",
            Self::AuthCommitted => "auth_committed",
            Self::Cleaned => "cleaned",
            Self::Finished => "finished",
            Self::Recovered => "recovered",
            Self::CredentialConflict => "credential_conflict",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "preparing" => Self::Preparing,
            "checked_out" => Self::CheckedOut,
            "running" => Self::Running,
            "codex_exited" => Self::CodexExited,
            "committing_auth" => Self::CommittingAuth,
            "auth_committed" => Self::AuthCommitted,
            "cleaned" => Self::Cleaned,
            "finished" => Self::Finished,
            "recovered" => Self::Recovered,
            "credential_conflict" => Self::CredentialConflict,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    pub const fn terminal(self) -> bool {
        matches!(self, Self::Finished | Self::Recovered | Self::CredentialConflict | Self::Failed)
    }

    pub const fn permits(self, next: Self) -> bool {
        if self as u8 == next as u8 {
            return true;
        }
        matches!(
            (self, next),
            (Self::Preparing, Self::CheckedOut | Self::Recovered | Self::Failed)
                | (
                    Self::CheckedOut,
                    Self::Running | Self::CommittingAuth | Self::Failed | Self::CredentialConflict
                )
                | (Self::Running, Self::CodexExited | Self::Failed)
                | (Self::CodexExited, Self::CommittingAuth | Self::Failed)
                | (
                    Self::CommittingAuth,
                    Self::AuthCommitted | Self::CredentialConflict | Self::Failed
                )
                | (Self::AuthCommitted, Self::Cleaned | Self::Failed)
                | (Self::Cleaned, Self::Finished | Self::Recovered)
        )
    }
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
pub struct RecoveryAttempt {
    pub attempt_id: String,
    pub incident_id: String,
    pub transaction_id: String,
    pub action: String,
    pub classification: String,
    pub result_state: String,
    pub error_code: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub boot_id: String,
    pub start_ticks: u64,
    pub identity: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LaunchTransaction {
    pub transaction_id: String,
    pub launch_id: String,
    pub project_id: String,
    pub account_id: String,
    pub state: TransactionState,
    pub runner_pid: Option<u32>,
    pub codex_pid: Option<u32>,
    pub boot_id: Option<String>,
    pub runner_start_ticks: Option<u64>,
    pub codex_start_ticks: Option<u64>,
    pub runner_identity: Option<String>,
    pub codex_identity: Option<String>,
    pub vault_hash_before_checkout: Option<String>,
    pub runtime_hash_at_checkout: Option<String>,
    pub runtime_hash_at_recovery: Option<String>,
    pub vault_hash_at_recovery: Option<String>,
    pub credential_backup_reference: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_message_redacted: Option<String>,
    pub recovery_attempts: u32,
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
pub struct ProjectSettings {
    pub project_id: String,
    pub runtime: String,
    pub default_account_id: Option<String>,
    pub default_model: String,
    pub reasoning: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TerminalPresetTemplate {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CommandPresetTemplate {
    pub name: String,
    pub description: String,
    pub terminal_kind: String,
    pub working_directory: String,
    pub command: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProjectTemplate {
    pub template_id: String,
    pub name: String,
    pub description: String,
    pub default_model: String,
    pub reasoning: String,
    pub terminal_presets: Vec<TerminalPresetTemplate>,
    pub command_presets: Vec<CommandPresetTemplate>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CommandPreset {
    pub preset_id: String,
    pub project_id: String,
    pub name: String,
    pub description: String,
    pub terminal_kind: String,
    pub working_directory: String,
    pub command: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct InputHistory {
    pub history_id: String,
    pub project_id: String,
    pub terminal_id: Option<String>,
    pub thread_id: Option<String>,
    pub kind: String,
    pub input_text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceEntry {
    pub relative_path: String,
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub modified_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspacePreview {
    pub relative_path: String,
    pub content: String,
    pub line_count: usize,
    pub truncated: bool,
    pub modified_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceLocation {
    pub relative_path: String,
    pub canonical_wsl_path: String,
    pub canonical_windows_path: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::TransactionState::*;

    #[test]
    fn terminal_states_are_immutable() {
        for state in [Finished, Recovered, CredentialConflict, Failed] {
            assert!(state.permits(state));
            assert!(!state.permits(Preparing));
        }
    }

    #[test]
    fn rejects_shortcut_transitions() {
        assert!(!Running.permits(Finished));
        assert!(!CheckedOut.permits(Finished));
        assert!(!CredentialConflict.permits(AuthCommitted));
        assert!(!Failed.permits(Recovered));
    }
}
