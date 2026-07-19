#![forbid(unsafe_code)]

use std::{fs, os::unix::fs::PermissionsExt};

use fs4 as _;
use muxlane_core::{
    credential::{self, CredentialDisposition},
    layout::{Layout, atomic_write_private, read_valid_json},
    model::{LaunchTransaction, TransactionState},
    recovery,
    service::{prepare_launch, register_project},
    storage::Storage,
};
use nix as _;
use rusqlite as _;
use serde as _;
use serde_json as _;
use sha2 as _;
use tempfile::TempDir;
use thiserror as _;
use uuid as _;

struct Fixture {
    _temp: TempDir,
    storage: Storage,
    transaction: LaunchTransaction,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("synthetic-auth.json");
        fs::write(&source, br#"{"fixture_version":1,"identity":"alpha"}"#).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        let project_source = temp.path().join("project");
        fs::create_dir(&project_source).unwrap();
        let storage =
            Storage::open(Layout::initialize(temp.path().join("runtime")).unwrap()).unwrap();
        let account = credential::import_account(&storage, &source, "Synthetic").unwrap();
        let project = register_project(&storage, &project_source, "Synthetic").unwrap();
        let transaction =
            prepare_launch(&storage, &account.account_id, &project.project_id).unwrap();
        Self { _temp: temp, storage, transaction }
    }

    fn runtime(&self) -> std::path::PathBuf {
        self.storage.layout().codex_home(&self.transaction.project_id).unwrap().join("auth.json")
    }

    fn vault(&self) -> std::path::PathBuf {
        self.storage.layout().vault_auth(&self.transaction.account_id).unwrap()
    }
}

#[test]
fn checkout_crash_recovers_without_losing_the_only_credential() {
    let fixture = Fixture::new();
    credential::checkout(&fixture.storage, &fixture.transaction.transaction_id).unwrap();
    let result = recovery::recover_all(&fixture.storage).unwrap();
    assert_eq!(result[0].state, TransactionState::Recovered);
    assert!(!fixture.runtime().exists());
    assert!(fixture.vault().exists());
}

#[test]
fn recovery_commits_a_runtime_only_refresh() {
    let fixture = Fixture::new();
    credential::checkout(&fixture.storage, &fixture.transaction.transaction_id).unwrap();
    atomic_write_private(
        &fixture.runtime(),
        br#"{"fixture_version":2,"identity":"runtime-refresh"}"#,
        true,
    )
    .unwrap();
    let result = recovery::recover_all(&fixture.storage).unwrap();
    assert_eq!(result[0].classification, "runtime_credential_committed");
    assert_eq!(
        read_valid_json(&fixture.vault()).unwrap().0,
        br#"{"fixture_version":2,"identity":"runtime-refresh"}"#
    );
    assert!(!fixture.runtime().exists());
}

#[test]
fn recovery_preserves_a_newer_vault_when_runtime_did_not_change() {
    let fixture = Fixture::new();
    credential::checkout(&fixture.storage, &fixture.transaction.transaction_id).unwrap();
    atomic_write_private(
        &fixture.vault(),
        br#"{"fixture_version":2,"identity":"newer-vault"}"#,
        true,
    )
    .unwrap();
    let result = recovery::recover_all(&fixture.storage).unwrap();
    assert_eq!(result[0].classification, "newer_vault_preserved");
    assert_eq!(
        read_valid_json(&fixture.vault()).unwrap().0,
        br#"{"fixture_version":2,"identity":"newer-vault"}"#
    );
    assert!(!fixture.runtime().exists());
}

#[test]
fn corrupt_runtime_is_isolated_and_never_written_to_vault() {
    let fixture = Fixture::new();
    credential::checkout(&fixture.storage, &fixture.transaction.transaction_id).unwrap();
    atomic_write_private(&fixture.runtime(), b"{incomplete", true).unwrap();
    let error = credential::commit_and_clean(&fixture.storage, &fixture.transaction.transaction_id)
        .unwrap_err();
    assert_eq!(error.code, "INVALID_CREDENTIAL");
    assert_eq!(
        read_valid_json(&fixture.vault()).unwrap().0,
        br#"{"fixture_version":1,"identity":"alpha"}"#
    );
    assert!(!fixture.runtime().exists());
    let transaction = fixture.storage.transaction(&fixture.transaction.transaction_id).unwrap();
    assert_eq!(transaction.state, TransactionState::Failed);
    assert!(transaction.credential_backup_reference.is_some());
}

#[test]
fn recovery_resumes_after_vault_commit_and_before_runtime_cleanup() {
    let fixture = Fixture::new();
    credential::checkout(&fixture.storage, &fixture.transaction.transaction_id).unwrap();
    atomic_write_private(
        &fixture.runtime(),
        br#"{"fixture_version":2,"identity":"commit-boundary"}"#,
        true,
    )
    .unwrap();
    atomic_write_private(
        &fixture.vault(),
        br#"{"fixture_version":2,"identity":"commit-boundary"}"#,
        true,
    )
    .unwrap();
    fixture
        .storage
        .transition(&fixture.transaction.transaction_id, TransactionState::CommittingAuth)
        .unwrap();
    fixture
        .storage
        .transition(&fixture.transaction.transaction_id, TransactionState::AuthCommitted)
        .unwrap();
    let result = recovery::recover_all(&fixture.storage).unwrap();
    assert_eq!(result[0].classification, "completed_post_commit_cleanup");
    assert_eq!(result[0].state, TransactionState::Recovered);
    assert!(!fixture.runtime().exists());
}

#[test]
fn recovery_recognizes_atomic_vault_replace_before_state_advance() {
    let fixture = Fixture::new();
    credential::checkout(&fixture.storage, &fixture.transaction.transaction_id).unwrap();
    atomic_write_private(&fixture.runtime(), br#"{"identity":"refreshed"}"#, true).unwrap();
    fixture
        .storage
        .transition(&fixture.transaction.transaction_id, TransactionState::CommittingAuth)
        .unwrap();
    atomic_write_private(&fixture.vault(), br#"{"identity":"refreshed"}"#, true).unwrap();

    let result = recovery::recover_all(&fixture.storage).unwrap();
    assert_eq!(result[0].classification, "runtime_credential_committed");
    assert_eq!(result[0].state, TransactionState::Recovered);
    assert_eq!(read_valid_json(&fixture.vault()).unwrap().0, br#"{"identity":"refreshed"}"#);
    assert!(!fixture.runtime().exists());
}

#[test]
fn conflict_is_terminal_and_repeated_recovery_does_not_rewrite_it() {
    let fixture = Fixture::new();
    credential::checkout(&fixture.storage, &fixture.transaction.transaction_id).unwrap();
    atomic_write_private(&fixture.runtime(), br#"{"identity":"runtime-new"}"#, true).unwrap();
    atomic_write_private(&fixture.vault(), br#"{"identity":"vault-new"}"#, true).unwrap();
    assert_eq!(
        credential::commit_and_clean(&fixture.storage, &fixture.transaction.transaction_id)
            .unwrap(),
        CredentialDisposition::Conflict
    );
    let before = fixture.storage.transaction(&fixture.transaction.transaction_id).unwrap();
    assert_eq!(before.state, TransactionState::CredentialConflict);
    assert!(recovery::recover_all(&fixture.storage).unwrap().is_empty());
    let after = fixture.storage.transaction(&fixture.transaction.transaction_id).unwrap();
    assert_eq!(after, before);
}
