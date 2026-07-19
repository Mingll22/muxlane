use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    CoreError, CoreResult,
    layout::{Layout, ensure_no_symlink_components},
    model::{
        Account, LaunchTransaction, LaunchView, Project, RecoveryAttempt, RecoveryIncident,
        Terminal, ThreadIndex, TransactionState, UsageSnapshot,
    },
};

pub const SCHEMA_VERSION: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationClaim {
    New,
    InProgress,
    Completed(String),
}

#[derive(Debug, Clone)]
pub struct Storage {
    layout: Layout,
}

impl Storage {
    pub fn open(layout: Layout) -> CoreResult<Self> {
        let storage = Self { layout };
        let mut connection = storage.connect()?;
        storage.migrate(&mut connection)?;
        Ok(storage)
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn connect(&self) -> CoreResult<Connection> {
        let path = self.layout.database();
        if path.exists() {
            ensure_no_symlink_components(&path, false)?;
        }
        let connection = Connection::open(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(connection)
    }

    fn migrate(&self, connection: &mut Connection) -> CoreResult<()> {
        let current: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if current > SCHEMA_VERSION {
            return Err(CoreError::new(
                "MIGRATION_REQUIRED",
                "database schema is newer than this daemon",
            ));
        }
        if current < 1 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                r#"
                CREATE TABLE accounts (
                    account_id TEXT PRIMARY KEY,
                    display_name TEXT NOT NULL,
                    masked_email TEXT,
                    plan_type TEXT,
                    login_status TEXT NOT NULL,
                    credential_hash TEXT NOT NULL,
                    vault_relative_path TEXT NOT NULL UNIQUE,
                    archived_at INTEGER,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE projects (
                    project_id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    canonical_windows_path TEXT,
                    canonical_wsl_path TEXT NOT NULL UNIQUE,
                    runtime_relative_path TEXT NOT NULL UNIQUE,
                    tmux_session_name TEXT NOT NULL UNIQUE,
                    archived_at INTEGER,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE launches (
                    launch_id TEXT PRIMARY KEY,
                    transaction_id TEXT NOT NULL UNIQUE,
                    project_id TEXT NOT NULL REFERENCES projects(project_id),
                    account_id TEXT NOT NULL REFERENCES accounts(account_id),
                    outcome TEXT,
                    started_at INTEGER NOT NULL,
                    exited_at INTEGER
                );
                CREATE TABLE launch_transactions (
                    transaction_id TEXT PRIMARY KEY REFERENCES launches(transaction_id),
                    launch_id TEXT NOT NULL UNIQUE REFERENCES launches(launch_id),
                    project_id TEXT NOT NULL REFERENCES projects(project_id),
                    account_id TEXT NOT NULL REFERENCES accounts(account_id),
                    state TEXT NOT NULL,
                    runner_pid INTEGER,
                    codex_pid INTEGER,
                    boot_id TEXT,
                    runner_start_ticks INTEGER,
                    codex_start_ticks INTEGER,
                    runner_identity TEXT,
                    codex_identity TEXT,
                    vault_hash_before_checkout TEXT,
                    runtime_hash_at_checkout TEXT,
                    runtime_hash_at_recovery TEXT,
                    vault_hash_at_recovery TEXT,
                    credential_backup_reference TEXT,
                    last_error_code TEXT,
                    last_error_message_redacted TEXT,
                    recovery_attempts INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    schema_version INTEGER NOT NULL DEFAULT 1
                );
                CREATE TABLE recovery_incidents (
                    incident_id TEXT PRIMARY KEY,
                    transaction_id TEXT NOT NULL REFERENCES launch_transactions(transaction_id),
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    evidence_relative_path TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE recovery_attempts (
                    attempt_id TEXT PRIMARY KEY,
                    incident_id TEXT NOT NULL REFERENCES recovery_incidents(incident_id),
                    transaction_id TEXT NOT NULL REFERENCES launch_transactions(transaction_id),
                    classification TEXT NOT NULL,
                    result_state TEXT NOT NULL,
                    error_code TEXT,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE terminals (
                    terminal_id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL REFERENCES projects(project_id),
                    kind TEXT NOT NULL,
                    display_name TEXT NOT NULL,
                    tmux_window_identity TEXT NOT NULL,
                    lifecycle_status TEXT NOT NULL,
                    ordinal INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    closed_at INTEGER,
                    UNIQUE(project_id, tmux_window_identity)
                );
                CREATE TABLE usage_snapshots (
                    usage_snapshot_id TEXT PRIMARY KEY,
                    account_id TEXT NOT NULL REFERENCES accounts(account_id),
                    codex_version TEXT NOT NULL,
                    capability_fingerprint TEXT NOT NULL,
                    snapshot_json TEXT NOT NULL,
                    captured_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL,
                    error_code TEXT
                );
                CREATE TABLE daemon_events (
                    event_id TEXT PRIMARY KEY,
                    event_type TEXT NOT NULL,
                    resource_id TEXT,
                    safe_details_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE operations (
                    operation_id TEXT PRIMARY KEY,
                    method TEXT NOT NULL,
                    request_hash TEXT NOT NULL,
                    response_json TEXT,
                    created_at INTEGER NOT NULL
                );
                PRAGMA user_version = 1;
                "#,
            )?;
            transaction.commit()?;
        }
        let current: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if current < 2 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                r#"
                CREATE UNIQUE INDEX active_launch_per_project
                    ON launch_transactions(project_id)
                    WHERE state NOT IN ('finished', 'recovered', 'credential_conflict', 'failed');
                CREATE UNIQUE INDEX active_launch_per_account
                    ON launch_transactions(account_id)
                    WHERE state NOT IN ('finished', 'recovered', 'credential_conflict', 'failed');
                CREATE INDEX recovery_incidents_open
                    ON recovery_incidents(status, transaction_id);
                CREATE INDEX usage_snapshots_account_time
                    ON usage_snapshots(account_id, captured_at DESC);
                PRAGMA user_version = 2;
                "#,
            )?;
            transaction.commit()?;
        }
        let current: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if current < 3 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                r#"
                ALTER TABLE recovery_incidents ADD COLUMN resolution_action TEXT;
                ALTER TABLE recovery_incidents ADD COLUMN resolution_summary TEXT;
                ALTER TABLE recovery_incidents ADD COLUMN resolved_at INTEGER;
                ALTER TABLE recovery_attempts ADD COLUMN action TEXT NOT NULL DEFAULT 'automatic_recovery';
                ALTER TABLE recovery_attempts ADD COLUMN completed_at INTEGER;
                CREATE TABLE thread_indexes (
                    project_id TEXT NOT NULL REFERENCES projects(project_id),
                    thread_id TEXT NOT NULL,
                    source_relative_path TEXT NOT NULL,
                    source_modified_at INTEGER NOT NULL,
                    codex_version TEXT,
                    status TEXT NOT NULL,
                    indexed_at INTEGER NOT NULL,
                    PRIMARY KEY(project_id, thread_id),
                    UNIQUE(project_id, source_relative_path)
                );
                CREATE INDEX thread_indexes_project_time
                    ON thread_indexes(project_id, source_modified_at DESC);
                CREATE TABLE recovery_runs (
                    run_id TEXT PRIMARY KEY,
                    transaction_id TEXT NOT NULL REFERENCES launch_transactions(transaction_id),
                    status TEXT NOT NULL,
                    classification TEXT,
                    result_state TEXT,
                    error_code TEXT,
                    started_at INTEGER NOT NULL,
                    completed_at INTEGER
                );
                CREATE INDEX recovery_runs_transaction_time
                    ON recovery_runs(transaction_id, started_at DESC);
                PRAGMA user_version = 3;
                "#,
            )?;
            transaction.commit()?;
        }
        let current: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if current < 4 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                r#"
                ALTER TABLE terminals RENAME TO terminals_v3;
                CREATE TABLE terminals (
                    terminal_id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL REFERENCES projects(project_id),
                    kind TEXT NOT NULL,
                    display_name TEXT NOT NULL,
                    tmux_window_identity TEXT NOT NULL,
                    lifecycle_status TEXT NOT NULL,
                    ordinal INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    closed_at INTEGER
                );
                INSERT INTO terminals(
                    terminal_id,project_id,kind,display_name,tmux_window_identity,
                    lifecycle_status,ordinal,created_at,closed_at
                ) SELECT
                    terminal_id,project_id,kind,display_name,tmux_window_identity,
                    lifecycle_status,ordinal,created_at,closed_at
                FROM terminals_v3;
                DROP TABLE terminals_v3;
                CREATE UNIQUE INDEX active_terminal_window_identity
                    ON terminals(project_id,tmux_window_identity)
                    WHERE lifecycle_status != 'closed';
                PRAGMA user_version = 4;
                "#,
            )?;
            transaction.commit()?;
        }
        let current: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if current < 5 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                r#"
                CREATE TABLE project_settings (
                    project_id TEXT PRIMARY KEY REFERENCES projects(project_id),
                    runtime TEXT NOT NULL,
                    default_account_id TEXT REFERENCES accounts(account_id),
                    default_model TEXT NOT NULL,
                    reasoning TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE project_templates (
                    template_id TEXT PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    description TEXT NOT NULL,
                    default_model TEXT NOT NULL,
                    reasoning TEXT NOT NULL,
                    terminal_presets_json TEXT NOT NULL,
                    command_presets_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE command_presets (
                    preset_id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL REFERENCES projects(project_id),
                    name TEXT NOT NULL,
                    description TEXT NOT NULL,
                    terminal_kind TEXT NOT NULL,
                    working_directory TEXT NOT NULL,
                    command_text TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    UNIQUE(project_id, name)
                );
                CREATE TABLE input_history (
                    history_id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL REFERENCES projects(project_id),
                    terminal_id TEXT REFERENCES terminals(terminal_id),
                    thread_id TEXT,
                    kind TEXT NOT NULL,
                    input_text TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE INDEX input_history_scope_time
                    ON input_history(project_id, terminal_id, thread_id, kind, created_at DESC);
                PRAGMA user_version = 5;
                "#,
            )?;
            transaction.commit()?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> CoreResult<u32> {
        Ok(self.connect()?.pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn integrity(&self) -> CoreResult<String> {
        let result: String =
            self.connect()?.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if result != "ok" {
            return Err(CoreError::new("STORAGE_FAILURE", "database integrity check failed"));
        }
        Ok(result)
    }

    pub fn insert_account(&self, account: &Account, vault_relative_path: &str) -> CoreResult<()> {
        self.connect()?.execute(
            "INSERT INTO accounts(account_id,display_name,masked_email,plan_type,login_status,credential_hash,vault_relative_path,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
            params![account.account_id, account.display_name, account.masked_email, account.plan_type, account.login_status, account.credential_hash, vault_relative_path, account.created_at, account.updated_at],
        )?;
        Ok(())
    }

    pub fn list_accounts(&self) -> CoreResult<Vec<Account>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT a.account_id,a.display_name,a.masked_email,a.plan_type,a.login_status,a.credential_hash,a.created_at,a.updated_at,EXISTS(SELECT 1 FROM launch_transactions t WHERE t.account_id=a.account_id AND t.state NOT IN ('finished','recovered','credential_conflict','failed')) FROM accounts a WHERE a.archived_at IS NULL ORDER BY a.created_at,a.account_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Account {
                account_id: row.get(0)?,
                display_name: row.get(1)?,
                masked_email: row.get(2)?,
                plan_type: row.get(3)?,
                login_status: row.get(4)?,
                credential_hash: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                occupied: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn account(&self, account_id: &str) -> CoreResult<Account> {
        self.list_accounts()?
            .into_iter()
            .find(|account| account.account_id == account_id)
            .ok_or_else(|| CoreError::new("NOT_FOUND", "account was not found"))
    }

    pub fn update_account_metadata(
        &self,
        account_id: &str,
        masked_email: Option<&str>,
        plan_type: Option<&str>,
        login_status: &str,
        credential_hash: Option<&str>,
    ) -> CoreResult<()> {
        self.connect()?.execute(
            "UPDATE accounts SET masked_email=COALESCE(?,masked_email),plan_type=COALESCE(?,plan_type),login_status=?,credential_hash=COALESCE(?,credential_hash),updated_at=? WHERE account_id=?",
            params![masked_email, plan_type, login_status, credential_hash, now(), account_id],
        )?;
        Ok(())
    }

    pub fn insert_project(&self, project: &Project) -> CoreResult<()> {
        self.connect()?.execute(
            "INSERT INTO projects(project_id,name,canonical_windows_path,canonical_wsl_path,runtime_relative_path,tmux_session_name,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)",
            params![project.project_id, project.name, project.canonical_windows_path, project.canonical_wsl_path, project.runtime_relative_path, project.tmux_session_name, project.created_at, project.updated_at],
        )?;
        Ok(())
    }

    pub fn list_projects(&self) -> CoreResult<Vec<Project>> {
        self.list_projects_with_archived(false)
    }

    pub fn list_projects_with_archived(&self, include_archived: bool) -> CoreResult<Vec<Project>> {
        let connection = self.connect()?;
        let filter = if include_archived { "" } else { "WHERE p.archived_at IS NULL" };
        let sql = format!(
            "SELECT p.project_id,p.name,p.canonical_windows_path,p.canonical_wsl_path,p.runtime_relative_path,p.tmux_session_name,p.created_at,p.updated_at,EXISTS(SELECT 1 FROM launch_transactions t WHERE t.project_id=p.project_id AND t.state NOT IN ('finished','recovered','credential_conflict','failed')),p.archived_at FROM projects p {filter} ORDER BY p.created_at,p.project_id"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], |row| {
            Ok(Project {
                project_id: row.get(0)?,
                name: row.get(1)?,
                canonical_windows_path: row.get(2)?,
                canonical_wsl_path: row.get(3)?,
                runtime_relative_path: row.get(4)?,
                tmux_session_name: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                active: row.get(8)?,
                archived_at: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn project(&self, project_id: &str) -> CoreResult<Project> {
        self.list_projects()?
            .into_iter()
            .find(|project| project.project_id == project_id)
            .ok_or_else(|| CoreError::new("NOT_FOUND", "project was not found"))
    }

    pub fn project_including_archived(&self, project_id: &str) -> CoreResult<Project> {
        self.list_projects_with_archived(true)?
            .into_iter()
            .find(|project| project.project_id == project_id)
            .ok_or_else(|| CoreError::new("NOT_FOUND", "project was not found"))
    }

    pub fn archive_project(&self, project_id: &str) -> CoreResult<Project> {
        let timestamp = now();
        let changed = self.connect()?.execute(
            "UPDATE projects SET archived_at=COALESCE(archived_at,?),updated_at=? WHERE project_id=?",
            params![timestamp, timestamp, project_id],
        )?;
        if changed == 0 {
            return Err(CoreError::new("NOT_FOUND", "project was not found"));
        }
        self.project_including_archived(project_id)
    }

    pub fn create_launch(&self, transaction: &LaunchTransaction) -> CoreResult<()> {
        let mut connection = self.connect()?;
        let database = connection.transaction()?;
        database.execute(
            "INSERT INTO launches(launch_id,transaction_id,project_id,account_id,started_at) VALUES(?,?,?,?,?)",
            params![transaction.launch_id, transaction.transaction_id, transaction.project_id, transaction.account_id, transaction.created_at],
        )?;
        database.execute(
            "INSERT INTO launch_transactions(transaction_id,launch_id,project_id,account_id,state,created_at,updated_at,schema_version) VALUES(?,?,?,?,?,?,?,1)",
            params![transaction.transaction_id, transaction.launch_id, transaction.project_id, transaction.account_id, transaction.state.as_str(), transaction.created_at, transaction.updated_at],
        )?;
        database.commit()?;
        Ok(())
    }

    pub fn transaction(&self, transaction_id: &str) -> CoreResult<LaunchTransaction> {
        self.connect()?
            .query_row(
                "SELECT transaction_id,launch_id,project_id,account_id,state,runner_pid,codex_pid,boot_id,runner_start_ticks,codex_start_ticks,runner_identity,codex_identity,vault_hash_before_checkout,runtime_hash_at_checkout,runtime_hash_at_recovery,vault_hash_at_recovery,credential_backup_reference,last_error_code,last_error_message_redacted,recovery_attempts,created_at,updated_at FROM launch_transactions WHERE transaction_id=?",
                [transaction_id],
                map_transaction,
            )
            .optional()?
            .ok_or_else(|| CoreError::new("NOT_FOUND", "launch transaction was not found"))
    }

    pub fn transaction_for_launch(&self, launch_id: &str) -> CoreResult<LaunchTransaction> {
        let transaction_id: String = self
            .connect()?
            .query_row(
                "SELECT transaction_id FROM launches WHERE launch_id=?",
                [launch_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| CoreError::new("NOT_FOUND", "launch was not found"))?;
        self.transaction(&transaction_id)
    }

    pub fn launch_view(&self, launch_id: &str) -> CoreResult<LaunchView> {
        self.list_launches()?
            .into_iter()
            .find(|launch| launch.launch_id == launch_id)
            .ok_or_else(|| CoreError::new("NOT_FOUND", "launch was not found"))
    }

    pub fn list_nonterminal_transactions(&self) -> CoreResult<Vec<LaunchTransaction>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT transaction_id,launch_id,project_id,account_id,state,runner_pid,codex_pid,boot_id,runner_start_ticks,codex_start_ticks,runner_identity,codex_identity,vault_hash_before_checkout,runtime_hash_at_checkout,runtime_hash_at_recovery,vault_hash_at_recovery,credential_backup_reference,last_error_code,last_error_message_redacted,recovery_attempts,created_at,updated_at FROM launch_transactions WHERE state NOT IN ('finished','recovered','credential_conflict','failed') ORDER BY created_at",
        )?;
        let rows = statement.query_map([], map_transaction)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_launches(&self) -> CoreResult<Vec<LaunchView>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT l.launch_id,l.transaction_id,l.project_id,l.account_id,t.state,l.outcome,l.started_at,l.exited_at FROM launches l JOIN launch_transactions t ON t.transaction_id=l.transaction_id ORDER BY l.started_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            let state: String = row.get(4)?;
            Ok(LaunchView {
                launch_id: row.get(0)?,
                transaction_id: row.get(1)?,
                project_id: row.get(2)?,
                account_id: row.get(3)?,
                state: TransactionState::parse(&state).unwrap_or(TransactionState::Failed),
                outcome: row.get(5)?,
                started_at: row.get(6)?,
                exited_at: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn transition(&self, transaction_id: &str, next: TransactionState) -> CoreResult<()> {
        let current = self.transaction(transaction_id)?.state;
        if !current.permits(next) {
            return Err(CoreError::new("INVALID_STATE", "launch state transition is not allowed"));
        }
        self.connect()?.execute(
            "UPDATE launch_transactions SET state=?,updated_at=? WHERE transaction_id=? AND state=?",
            params![next.as_str(), now(), transaction_id, current.as_str()],
        )?;
        Ok(())
    }

    pub fn record_checkout(
        &self,
        transaction_id: &str,
        vault_hash: &str,
        runtime_hash: &str,
    ) -> CoreResult<()> {
        self.connect()?.execute(
            "UPDATE launch_transactions SET vault_hash_before_checkout=?,runtime_hash_at_checkout=?,updated_at=? WHERE transaction_id=?",
            params![vault_hash, runtime_hash, now(), transaction_id],
        )?;
        self.transition(transaction_id, TransactionState::CheckedOut)
    }

    pub fn record_processes(
        &self,
        transaction_id: &str,
        runner: &crate::model::ProcessIdentity,
        codex: &crate::model::ProcessIdentity,
    ) -> CoreResult<()> {
        self.connect()?.execute(
            "UPDATE launch_transactions SET runner_pid=?,codex_pid=?,boot_id=?,runner_start_ticks=?,codex_start_ticks=?,runner_identity=?,codex_identity=?,updated_at=? WHERE transaction_id=?",
            params![runner.pid, codex.pid, runner.boot_id, runner.start_ticks as i64, codex.start_ticks as i64, runner.identity, codex.identity, now(), transaction_id],
        )?;
        self.transition(transaction_id, TransactionState::Running)
    }

    pub fn record_recovery_hashes(
        &self,
        transaction_id: &str,
        vault_hash: Option<&str>,
        runtime_hash: Option<&str>,
    ) -> CoreResult<()> {
        self.connect()?.execute(
            "UPDATE launch_transactions SET vault_hash_at_recovery=?,runtime_hash_at_recovery=?,recovery_attempts=recovery_attempts+1,updated_at=? WHERE transaction_id=?",
            params![vault_hash, runtime_hash, now(), transaction_id],
        )?;
        Ok(())
    }

    pub fn record_backup(&self, transaction_id: &str, relative: &str) -> CoreResult<()> {
        self.connect()?.execute(
            "UPDATE launch_transactions SET credential_backup_reference=?,updated_at=? WHERE transaction_id=?",
            params![relative, now(), transaction_id],
        )?;
        Ok(())
    }

    pub fn fail(&self, transaction_id: &str, code: &str, message: &str) -> CoreResult<()> {
        let current = self.transaction(transaction_id)?.state;
        if current.terminal() {
            return Ok(());
        }
        self.connect()?.execute(
            "UPDATE launch_transactions SET state='failed',last_error_code=?,last_error_message_redacted=?,updated_at=? WHERE transaction_id=?",
            params![code, message, now(), transaction_id],
        )?;
        self.finish_launch(transaction_id, "failed")?;
        self.create_incident(transaction_id, code, None)?;
        Ok(())
    }

    pub fn finish_launch(&self, transaction_id: &str, outcome: &str) -> CoreResult<()> {
        self.connect()?.execute(
            "UPDATE launches SET outcome=?,exited_at=? WHERE transaction_id=?",
            params![outcome, now(), transaction_id],
        )?;
        Ok(())
    }

    pub fn create_incident(
        &self,
        transaction_id: &str,
        kind: &str,
        evidence: Option<&str>,
    ) -> CoreResult<String> {
        let connection = self.connect()?;
        if let Some(existing) = connection
            .query_row(
                "SELECT incident_id FROM recovery_incidents WHERE transaction_id=? AND status='open'",
                [transaction_id],
                |row| row.get(0),
            )
            .optional()?
        {
            return Ok(existing);
        }
        let incident_id = format!("incident_{}", uuid::Uuid::new_v4().simple());
        connection.execute(
            "INSERT INTO recovery_incidents(incident_id,transaction_id,kind,status,evidence_relative_path,created_at,updated_at) VALUES(?, ?, ?, 'open', ?, ?, ?)",
            params![incident_id, transaction_id, kind, evidence, now(), now()],
        )?;
        Ok(incident_id)
    }

    pub fn has_open_incident(&self, project_id: &str) -> CoreResult<bool> {
        let count: i64 = self.connect()?.query_row(
            "SELECT COUNT(*) FROM recovery_incidents i JOIN launch_transactions t ON t.transaction_id=i.transaction_id WHERE t.project_id=? AND i.status='open'",
            [project_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn incident(&self, incident_id: &str) -> CoreResult<RecoveryIncident> {
        self.connect()?
            .query_row(
                "SELECT i.incident_id,i.transaction_id,t.project_id,t.account_id,i.kind,i.status,i.evidence_relative_path,i.resolution_action,i.resolution_summary,i.created_at,i.updated_at,i.resolved_at FROM recovery_incidents i JOIN launch_transactions t ON t.transaction_id=i.transaction_id WHERE i.incident_id=?",
                [incident_id],
                |row| Ok(RecoveryIncident {
                    incident_id: row.get(0)?, transaction_id: row.get(1)?,
                    project_id: row.get(2)?, account_id: row.get(3)?, kind: row.get(4)?,
                    status: row.get(5)?, evidence_relative_path: row.get(6)?,
                    resolution_action: row.get(7)?, resolution_summary: row.get(8)?,
                    created_at: row.get(9)?, updated_at: row.get(10)?, resolved_at: row.get(11)?,
                }),
            )
            .optional()?
            .ok_or_else(|| CoreError::new("NOT_FOUND", "recovery incident was not found"))
    }

    pub fn list_incidents(&self, include_resolved: bool) -> CoreResult<Vec<RecoveryIncident>> {
        let filter = if include_resolved { "" } else { "WHERE i.status='open'" };
        let sql = format!(
            "SELECT i.incident_id,i.transaction_id,t.project_id,t.account_id,i.kind,i.status,i.evidence_relative_path,i.resolution_action,i.resolution_summary,i.created_at,i.updated_at,i.resolved_at FROM recovery_incidents i JOIN launch_transactions t ON t.transaction_id=i.transaction_id {filter} ORDER BY i.created_at,i.incident_id"
        );
        let connection = self.connect()?;
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], |row| {
            Ok(RecoveryIncident {
                incident_id: row.get(0)?,
                transaction_id: row.get(1)?,
                project_id: row.get(2)?,
                account_id: row.get(3)?,
                kind: row.get(4)?,
                status: row.get(5)?,
                evidence_relative_path: row.get(6)?,
                resolution_action: row.get(7)?,
                resolution_summary: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                resolved_at: row.get(11)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn begin_resolution_attempt(&self, incident_id: &str, action: &str) -> CoreResult<String> {
        let incident = self.incident(incident_id)?;
        let attempt_id = format!("attempt_{}", uuid::Uuid::new_v4().simple());
        self.connect()?.execute(
            "INSERT INTO recovery_attempts(attempt_id,incident_id,transaction_id,classification,result_state,error_code,action,created_at) VALUES(?,?,?,'manual_resolution','running',NULL,?,?)",
            params![attempt_id, incident_id, incident.transaction_id, action, now()],
        )?;
        Ok(attempt_id)
    }

    pub fn complete_resolution_attempt(
        &self,
        attempt_id: &str,
        result_state: &str,
        error_code: Option<&str>,
    ) -> CoreResult<()> {
        self.connect()?.execute(
            "UPDATE recovery_attempts SET result_state=?,error_code=?,completed_at=? WHERE attempt_id=?",
            params![result_state, error_code, now(), attempt_id],
        )?;
        Ok(())
    }

    pub fn resolve_incident(
        &self,
        incident_id: &str,
        action: &str,
        summary: &str,
    ) -> CoreResult<RecoveryIncident> {
        let timestamp = now();
        let changed = self.connect()?.execute(
            "UPDATE recovery_incidents SET status='resolved',resolution_action=?,resolution_summary=?,resolved_at=?,updated_at=? WHERE incident_id=? AND status='open'",
            params![action, summary, timestamp, timestamp, incident_id],
        )?;
        if changed == 0 && self.incident(incident_id)?.status != "resolved" {
            return Err(CoreError::new("INVALID_STATE", "incident cannot be resolved"));
        }
        self.incident(incident_id)
    }

    pub fn list_recovery_attempts(&self, incident_id: &str) -> CoreResult<Vec<RecoveryAttempt>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT attempt_id,incident_id,transaction_id,action,classification,result_state,error_code,created_at,completed_at FROM recovery_attempts WHERE incident_id=? ORDER BY created_at,attempt_id",
        )?;
        let rows = statement.query_map([incident_id], |row| {
            Ok(RecoveryAttempt {
                attempt_id: row.get(0)?,
                incident_id: row.get(1)?,
                transaction_id: row.get(2)?,
                action: row.get(3)?,
                classification: row.get(4)?,
                result_state: row.get(5)?,
                error_code: row.get(6)?,
                created_at: row.get(7)?,
                completed_at: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn record_recovery_attempt(
        &self,
        incident_id: &str,
        transaction_id: &str,
        classification: &str,
        state: TransactionState,
        error_code: Option<&str>,
    ) -> CoreResult<()> {
        self.connect()?.execute(
            "INSERT INTO recovery_attempts(attempt_id,incident_id,transaction_id,classification,result_state,error_code,created_at) VALUES(?,?,?,?,?,?,?)",
            params![format!("attempt_{}", uuid::Uuid::new_v4().simple()), incident_id, transaction_id, classification, state.as_str(), error_code, now()],
        )?;
        Ok(())
    }

    pub fn begin_recovery_run(&self, transaction_id: &str) -> CoreResult<String> {
        let run_id = format!("recovery_run_{}", uuid::Uuid::new_v4().simple());
        self.connect()?.execute(
            "INSERT INTO recovery_runs(run_id,transaction_id,status,started_at) VALUES(?,?,'running',?)",
            params![run_id, transaction_id, now()],
        )?;
        Ok(run_id)
    }

    pub fn complete_recovery_run(
        &self,
        run_id: &str,
        classification: &str,
        state: TransactionState,
        error_code: Option<&str>,
    ) -> CoreResult<()> {
        self.connect()?.execute(
            "UPDATE recovery_runs SET status=?,classification=?,result_state=?,error_code=?,completed_at=? WHERE run_id=?",
            params![if error_code.is_some() { "failed" } else { "completed" }, classification, state.as_str(), error_code, now(), run_id],
        )?;
        Ok(())
    }

    pub fn incomplete_recovery_runs(&self) -> CoreResult<u64> {
        let count: i64 = self.connect()?.query_row(
            "SELECT COUNT(*) FROM recovery_runs WHERE status='running'",
            [],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn insert_terminal(&self, terminal: &Terminal, ordinal: i64) -> CoreResult<()> {
        self.connect()?.execute(
            "INSERT INTO terminals(terminal_id,project_id,kind,display_name,tmux_window_identity,lifecycle_status,ordinal,created_at) VALUES(?,?,?,?,?,?,?,?)",
            params![terminal.terminal_id, terminal.project_id, terminal.kind, terminal.display_name, terminal.tmux_window_identity, terminal.lifecycle_status, ordinal, terminal.created_at],
        )?;
        Ok(())
    }

    pub fn list_terminals(&self, project_id: &str) -> CoreResult<Vec<Terminal>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT terminal_id,project_id,kind,display_name,tmux_window_identity,lifecycle_status,created_at,closed_at FROM terminals WHERE project_id=? ORDER BY ordinal",
        )?;
        let rows = statement.query_map([project_id], |row| {
            Ok(Terminal {
                terminal_id: row.get(0)?,
                project_id: row.get(1)?,
                kind: row.get(2)?,
                display_name: row.get(3)?,
                tmux_window_identity: row.get(4)?,
                lifecycle_status: row.get(5)?,
                created_at: row.get(6)?,
                closed_at: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn terminal(&self, terminal_id: &str) -> CoreResult<Terminal> {
        let connection = self.connect()?;
        connection.query_row(
            "SELECT terminal_id,project_id,kind,display_name,tmux_window_identity,lifecycle_status,created_at,closed_at FROM terminals WHERE terminal_id=?",
            [terminal_id],
            |row| Ok(Terminal { terminal_id: row.get(0)?, project_id: row.get(1)?, kind: row.get(2)?, display_name: row.get(3)?, tmux_window_identity: row.get(4)?, lifecycle_status: row.get(5)?, created_at: row.get(6)?, closed_at: row.get(7)? }),
        ).optional()?.ok_or_else(|| CoreError::new("NOT_FOUND", "Terminal was not found"))
    }

    pub fn close_terminal(&self, terminal_id: &str) -> CoreResult<Terminal> {
        let timestamp = now();
        self.connect()?.execute(
            "UPDATE terminals SET lifecycle_status='closed',closed_at=COALESCE(closed_at,?) WHERE terminal_id=?",
            params![timestamp, terminal_id],
        )?;
        self.terminal(terminal_id)
    }

    pub fn close_project_terminals(&self, project_id: &str) -> CoreResult<()> {
        let timestamp = now();
        self.connect()?.execute(
            "UPDATE terminals SET lifecycle_status='closed',closed_at=COALESCE(closed_at,?) WHERE project_id=? AND lifecycle_status!='closed'",
            params![timestamp, project_id],
        )?;
        Ok(())
    }

    pub fn replace_thread_indexes(
        &self,
        project_id: &str,
        indexes: &[ThreadIndex],
    ) -> CoreResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM thread_indexes WHERE project_id=?", [project_id])?;
        for index in indexes {
            transaction.execute(
                "INSERT INTO thread_indexes(project_id,thread_id,source_relative_path,source_modified_at,codex_version,status,indexed_at) VALUES(?,?,?,?,?,?,?)",
                params![index.project_id,index.thread_id,index.source_relative_path,index.source_modified_at,index.codex_version,index.status,index.indexed_at],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn list_thread_indexes(&self, project_id: &str) -> CoreResult<Vec<ThreadIndex>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT thread_id,project_id,source_relative_path,source_modified_at,codex_version,status,indexed_at FROM thread_indexes WHERE project_id=? ORDER BY source_modified_at DESC,thread_id",
        )?;
        let rows = statement.query_map([project_id], |row| {
            Ok(ThreadIndex {
                thread_id: row.get(0)?,
                project_id: row.get(1)?,
                source_relative_path: row.get(2)?,
                source_modified_at: row.get(3)?,
                codex_version: row.get(4)?,
                status: row.get(5)?,
                indexed_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn cache_usage(&self, snapshot: &UsageSnapshot) -> CoreResult<()> {
        let encoded = serde_json::to_string(snapshot)?;
        self.connect()?.execute(
            "INSERT INTO usage_snapshots(usage_snapshot_id,account_id,codex_version,capability_fingerprint,snapshot_json,captured_at,expires_at,error_code) VALUES(?,?,?,?,?,?,?,?)",
            params![snapshot.usage_snapshot_id, snapshot.account_id, snapshot.codex_version, snapshot.capability_fingerprint, encoded, snapshot.captured_at, snapshot.expires_at, snapshot.error_code],
        )?;
        Ok(())
    }

    pub fn latest_usage(&self, account_id: &str) -> CoreResult<Option<UsageSnapshot>> {
        let encoded: Option<String> = self
            .connect()?
            .query_row(
                "SELECT snapshot_json FROM usage_snapshots WHERE account_id=? ORDER BY captured_at DESC LIMIT 1",
                [account_id],
                |row| row.get(0),
            )
            .optional()?;
        encoded.map(|value| serde_json::from_str(&value).map_err(Into::into)).transpose()
    }

    pub fn counts(&self) -> CoreResult<(u64, u64, u64, u64)> {
        let connection = self.connect()?;
        let count = |table: &str| -> CoreResult<u64> {
            if !["accounts", "projects", "launches", "recovery_incidents"].contains(&table) {
                return Err(CoreError::new("INVALID_REQUEST", "invalid count table"));
            }
            let value: i64 =
                connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))?;
            Ok(value.max(0) as u64)
        };
        Ok((
            count("accounts")?,
            count("projects")?,
            count("launches")?,
            count("recovery_incidents")?,
        ))
    }

    pub fn claim_operation(
        &self,
        operation_id: &str,
        method: &str,
        request_hash: &str,
    ) -> CoreResult<OperationClaim> {
        let connection = self.connect()?;
        let existing: Option<(String, String, Option<String>)> = connection
            .query_row(
                "SELECT method,request_hash,response_json FROM operations WHERE operation_id=?",
                [operation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((existing_method, existing_hash, response)) = existing {
            if existing_method != method || existing_hash != request_hash {
                return Err(CoreError::new(
                    "CONFLICT",
                    "operation identifier was reused with different semantics",
                ));
            }
            return Ok(response.map_or(OperationClaim::InProgress, OperationClaim::Completed));
        }
        match connection.execute(
            "INSERT INTO operations(operation_id,method,request_hash,created_at) VALUES(?,?,?,?)",
            params![operation_id, method, request_hash, now()],
        ) {
            Ok(_) => Ok(OperationClaim::New),
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                self.claim_operation(operation_id, method, request_hash)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn complete_operation(&self, operation_id: &str, response_json: &str) -> CoreResult<()> {
        let changed = self.connect()?.execute(
            "UPDATE operations SET response_json=? WHERE operation_id=? AND response_json IS NULL",
            params![response_json, operation_id],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(CoreError::new("CONFLICT", "operation completion was not unique"))
        }
    }
}

fn map_transaction(row: &rusqlite::Row<'_>) -> rusqlite::Result<LaunchTransaction> {
    let state: String = row.get(4)?;
    Ok(LaunchTransaction {
        transaction_id: row.get(0)?,
        launch_id: row.get(1)?,
        project_id: row.get(2)?,
        account_id: row.get(3)?,
        state: TransactionState::parse(&state).unwrap_or(TransactionState::Failed),
        runner_pid: row.get::<_, Option<u32>>(5)?,
        codex_pid: row.get::<_, Option<u32>>(6)?,
        boot_id: row.get(7)?,
        runner_start_ticks: row.get::<_, Option<i64>>(8)?.and_then(|value| value.try_into().ok()),
        codex_start_ticks: row.get::<_, Option<i64>>(9)?.and_then(|value| value.try_into().ok()),
        runner_identity: row.get(10)?,
        codex_identity: row.get(11)?,
        vault_hash_before_checkout: row.get(12)?,
        runtime_hash_at_checkout: row.get(13)?,
        runtime_hash_at_recovery: row.get(14)?,
        vault_hash_at_recovery: row.get(15)?,
        credential_backup_reference: row.get(16)?,
        last_error_code: row.get(17)?,
        last_error_message_redacted: row.get(18)?,
        recovery_attempts: row.get(19)?,
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
    })
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

pub fn database_exists(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn migrations_are_transactional_and_idempotent() {
        let temp = tempdir().unwrap();
        let layout = Layout::initialize(temp.path().join("muxlane")).unwrap();
        let storage = Storage::open(layout.clone()).unwrap();
        assert_eq!(storage.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(storage.integrity().unwrap(), "ok");
        assert_eq!(Storage::open(layout).unwrap().schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn newer_schema_is_rejected_without_rewrite() {
        let temp = tempdir().unwrap();
        let layout = Layout::initialize(temp.path().join("muxlane")).unwrap();
        let storage = Storage::open(layout.clone()).unwrap();
        storage.connect().unwrap().pragma_update(None, "user_version", 999).unwrap();
        assert_eq!(Storage::open(layout).unwrap_err().code, "MIGRATION_REQUIRED");
    }

    #[test]
    fn operation_ids_are_idempotent_and_semantically_bound() {
        let temp = tempdir().unwrap();
        let layout = Layout::initialize(temp.path().join("muxlane")).unwrap();
        let storage = Storage::open(layout).unwrap();
        assert_eq!(
            storage.claim_operation("operation_fixture", "test.write", "hash-a").unwrap(),
            OperationClaim::New
        );
        assert_eq!(
            storage.claim_operation("operation_fixture", "test.write", "hash-a").unwrap(),
            OperationClaim::InProgress
        );
        storage.complete_operation("operation_fixture", "{\"ok\":true}").unwrap();
        assert_eq!(
            storage.claim_operation("operation_fixture", "test.write", "hash-a").unwrap(),
            OperationClaim::Completed("{\"ok\":true}".to_owned())
        );
        assert_eq!(
            storage.claim_operation("operation_fixture", "test.write", "hash-b").unwrap_err().code,
            "CONFLICT"
        );
    }
}
