export type TransactionState =
  | 'preparing'
  | 'checked_out'
  | 'running'
  | 'codex_exited'
  | 'committing_auth'
  | 'auth_committed'
  | 'cleaned'
  | 'finished'
  | 'recovered'
  | 'credential_conflict'
  | 'failed';

export type Account = {
  account_id: string;
  display_name: string;
  masked_email: string | null;
  plan_type: string | null;
  login_status: string;
  occupied: boolean;
  created_at: number;
  updated_at: number;
};

export type Project = {
  project_id: string;
  name: string;
  canonical_windows_path: string | null;
  canonical_wsl_path: string;
  runtime_relative_path: string;
  tmux_session_name: string;
  active: boolean;
  archived_at: number | null;
  created_at: number;
  updated_at: number;
};

export type Launch = {
  launch_id: string;
  transaction_id: string;
  project_id: string;
  account_id: string;
  state: TransactionState;
  outcome: string | null;
  started_at: number;
  exited_at: number | null;
};

export type RecoveryIncident = {
  incident_id: string;
  transaction_id: string;
  project_id: string;
  account_id: string;
  kind: string;
  status: string;
  evidence_relative_path: string | null;
  resolution_action: string | null;
  resolution_summary: string | null;
  created_at: number;
  updated_at: number;
  resolved_at: number | null;
};

export type TerminalRecord = {
  terminal_id: string;
  project_id: string;
  kind: string;
  display_name: string;
  tmux_window_identity: string;
  lifecycle_status: string;
  created_at: number;
  closed_at: number | null;
};

export type ThreadIndex = {
  thread_id: string;
  project_id: string;
  source_relative_path: string;
  source_modified_at: number;
  codex_version: string | null;
  status: string;
  indexed_at: number;
};

export type UsageWindow = {
  duration_minutes: number | null;
  used_percent: number | null;
  resets_at: number | null;
};

export type UsageSnapshot = {
  usage_snapshot_id: string;
  account_id: string;
  codex_version: string;
  account_type: string | null;
  plan_type: string | null;
  login_status: string;
  windows: UsageWindow[];
  lifetime_tokens: number | null;
  reset_credit_available: number | null;
  captured_at: number;
  expires_at: number;
  error_code: string | null;
};

export type UsageRefreshResult = {
  account_id: string;
  snapshot: UsageSnapshot | null;
  error_code: string | null;
};

export type ProjectSettings = {
  project_id: string;
  runtime: string;
  default_account_id: string | null;
  default_model: string;
  reasoning: string;
  updated_at: number;
};

export type TerminalPresetTemplate = { name: string; kind: string };
export type CommandPresetTemplate = {
  name: string;
  description: string;
  terminal_kind: string;
  working_directory: string;
  command: string;
};

export type ProjectTemplate = {
  template_id: string;
  name: string;
  description: string;
  default_model: string;
  reasoning: string;
  terminal_presets: TerminalPresetTemplate[];
  command_presets: CommandPresetTemplate[];
  created_at: number;
  updated_at: number;
};

export type CommandPreset = CommandPresetTemplate & {
  preset_id: string;
  project_id: string;
  created_at: number;
  updated_at: number;
};

export type InputHistory = {
  history_id: string;
  project_id: string;
  terminal_id: string | null;
  thread_id: string | null;
  kind: 'shell' | 'prompt';
  input_text: string;
  created_at: number;
};

export type WorkspaceEntry = {
  relative_path: string;
  name: string;
  kind: 'directory' | 'file';
  size: number;
  modified_at: number | null;
};

export type WorkspacePreview = {
  relative_path: string;
  content: string;
  line_count: number;
  truncated: boolean;
  modified_at: number | null;
};

export type EnvironmentCheck = {
  key: 'windows' | 'wsl' | 'codex' | 'tmux' | 'muxlaned';
  status: 'ready' | 'unavailable' | 'unsupported';
  version: string | null;
  suggestion: string | null;
};

export type Handshake = {
  protocol_major: number;
  protocol_minor: number;
  daemon_version: string;
  daemon_instance_id: string;
  granted_capabilities: string[];
  max_control_message_bytes: number;
};

export type ControlRequest = { method: string; params?: Record<string, unknown> };
export type ControlResponse = { kind: string; data?: unknown };
