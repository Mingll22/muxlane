use crate::{
    CoreError, CoreResult,
    credential::{self, CredentialDisposition},
    layout::{file_hash, remove_private_file},
    lock::LaunchLocks,
    model::{LaunchTransaction, RecoveryResult, TransactionState},
    process::{boot_id, matches_recorded},
    storage::Storage,
};

pub fn recover_all(storage: &Storage) -> CoreResult<Vec<RecoveryResult>> {
    let current_boot = boot_id()?;
    storage
        .list_nonterminal_transactions()?
        .into_iter()
        .map(|transaction| recover_transaction(storage, &transaction, &current_boot))
        .collect()
}

pub fn recover_transaction(
    storage: &Storage,
    transaction: &LaunchTransaction,
    current_boot: &str,
) -> CoreResult<RecoveryResult> {
    let previous_state = transaction.state;
    let locks = match LaunchLocks::try_acquire(
        storage.layout(),
        &transaction.account_id,
        &transaction.project_id,
    ) {
        Ok(locks) => locks,
        Err(error) if matches!(error.code, "ACCOUNT_IN_USE" | "PROJECT_IN_USE") => {
            return Ok(RecoveryResult {
                transaction_id: transaction.transaction_id.clone(),
                previous_state,
                state: previous_state,
                classification: "active_flock".to_owned(),
                changed: false,
                incident_id: None,
            });
        }
        Err(error) => return Err(error),
    };

    if transaction.state == TransactionState::Running
        && (matches_recorded(transaction, true, current_boot)?
            || matches_recorded(transaction, false, current_boot)?)
    {
        drop(locks);
        storage.fail(
            &transaction.transaction_id,
            "PROCESS_IDENTITY_UNCONFIRMED",
            "recorded process exists without the required active flock owner",
        )?;
        let incident =
            storage.create_incident(&transaction.transaction_id, "process_without_lock", None)?;
        storage.record_recovery_attempt(
            &incident,
            &transaction.transaction_id,
            "process_without_lock",
            TransactionState::Failed,
            Some("PROCESS_IDENTITY_UNCONFIRMED"),
        )?;
        return Ok(RecoveryResult {
            transaction_id: transaction.transaction_id.clone(),
            previous_state,
            state: TransactionState::Failed,
            classification: "process_without_lock".to_owned(),
            changed: true,
            incident_id: Some(incident),
        });
    }

    let classification = match transaction.state {
        TransactionState::Preparing => {
            match credential::recover_preparing(storage, &transaction.transaction_id) {
                Ok(_) => "checkout_boundary_cleaned",
                Err(error) => {
                    let incident = storage.create_incident(
                        &transaction.transaction_id,
                        "checkout_ambiguous",
                        None,
                    )?;
                    storage.record_recovery_attempt(
                        &incident,
                        &transaction.transaction_id,
                        "checkout_ambiguous",
                        TransactionState::Failed,
                        Some(error.code),
                    )?;
                    return Ok(RecoveryResult {
                        transaction_id: transaction.transaction_id.clone(),
                        previous_state,
                        state: TransactionState::Failed,
                        classification: "checkout_ambiguous".to_owned(),
                        changed: true,
                        incident_id: Some(incident),
                    });
                }
            }
        }
        TransactionState::CheckedOut => finish_commit_recovery(storage, transaction)?,
        TransactionState::Running => {
            storage.transition(&transaction.transaction_id, TransactionState::CodexExited)?;
            finish_commit_recovery(storage, transaction)?
        }
        TransactionState::CodexExited | TransactionState::CommittingAuth => {
            finish_commit_recovery(storage, transaction)?
        }
        TransactionState::AuthCommitted => {
            recover_after_vault_commit(storage, transaction)?;
            "completed_post_commit_cleanup"
        }
        TransactionState::Cleaned => {
            storage.transition(&transaction.transaction_id, TransactionState::Recovered)?;
            storage.finish_launch(&transaction.transaction_id, "recovered")?;
            "completed_cleaned_transaction"
        }
        state if state.terminal() => "already_terminal",
        _ => return Err(CoreError::new("INVALID_STATE", "recovery state is unsupported")),
    };
    drop(locks);
    let final_transaction = storage.transaction(&transaction.transaction_id)?;
    let incident_id = if final_transaction.state == TransactionState::CredentialConflict {
        Some(storage.create_incident(
            &transaction.transaction_id,
            "credential_conflict",
            final_transaction.credential_backup_reference.as_deref(),
        )?)
    } else {
        None
    };
    Ok(RecoveryResult {
        transaction_id: transaction.transaction_id.clone(),
        previous_state,
        state: final_transaction.state,
        classification: classification.to_owned(),
        changed: final_transaction.state != previous_state,
        incident_id,
    })
}

fn finish_commit_recovery(
    storage: &Storage,
    transaction: &LaunchTransaction,
) -> CoreResult<&'static str> {
    let disposition = credential::commit_and_clean(storage, &transaction.transaction_id)?;
    if disposition == CredentialDisposition::Conflict {
        return Ok("credential_conflict");
    }
    storage.transition(&transaction.transaction_id, TransactionState::Recovered)?;
    storage.finish_launch(&transaction.transaction_id, "recovered")?;
    Ok(match disposition {
        CredentialDisposition::Unchanged => "credential_unchanged",
        CredentialDisposition::RuntimeCommitted => "runtime_credential_committed",
        CredentialDisposition::NewerVaultPreserved => "newer_vault_preserved",
        CredentialDisposition::Conflict => unreachable!(),
    })
}

fn recover_after_vault_commit(
    storage: &Storage,
    transaction: &LaunchTransaction,
) -> CoreResult<()> {
    let runtime = storage.layout().codex_home(&transaction.project_id)?.join("auth.json");
    let vault = storage.layout().vault_auth(&transaction.account_id)?;
    if !runtime.exists() {
        return Err(CoreError::new("RECOVERY_REQUIRED", "post-commit Runtime evidence is missing"));
    }
    if file_hash(&runtime)? != file_hash(&vault)? {
        return Err(CoreError::new(
            "CREDENTIAL_CONFLICT",
            "post-commit credential hashes do not match",
        ));
    }
    remove_private_file(&runtime)?;
    storage.transition(&transaction.transaction_id, TransactionState::Cleaned)?;
    storage.transition(&transaction.transaction_id, TransactionState::Recovered)?;
    storage.finish_launch(&transaction.transaction_id, "recovered")
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use tempfile::tempdir;
    use uuid::Uuid;

    use crate::{
        credential::import_account,
        service::{prepare_launch, register_project},
    };

    use super::*;

    fn prepared() -> (tempfile::TempDir, Storage, LaunchTransaction) {
        let temp = tempdir().unwrap();
        let source = temp.path().join("auth.json");
        fs::write(&source, br#"{"fixture":"alpha"}"#).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        let layout = crate::layout::Layout::initialize(temp.path().join("runtime")).unwrap();
        let storage = Storage::open(layout).unwrap();
        let account = import_account(&storage, &source, "fixture").unwrap();
        let project_dir = temp.path().join("project");
        fs::create_dir(&project_dir).unwrap();
        let project = register_project(&storage, &project_dir, "fixture").unwrap();
        let transaction =
            prepare_launch(&storage, &account.account_id, &project.project_id).unwrap();
        (temp, storage, transaction)
    }

    #[test]
    fn repeated_recovery_is_idempotent() {
        let (_temp, storage, transaction) = prepared();
        let first = recover_all(&storage).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].state, TransactionState::Recovered);
        assert!(recover_all(&storage).unwrap().is_empty());
        assert_eq!(
            storage.transaction(&transaction.transaction_id).unwrap().state,
            TransactionState::Recovered
        );
    }

    #[test]
    fn boot_change_and_stale_pid_do_not_reuse_process_identity() {
        let (_temp, storage, transaction) = prepared();
        credential::checkout(&storage, &transaction.transaction_id).unwrap();
        let current = crate::process::inspect_process(std::process::id()).unwrap();
        let fake_codex = crate::model::ProcessIdentity {
            pid: current.pid,
            boot_id: current.boot_id.clone(),
            start_ticks: current.start_ticks.saturating_add(1),
            identity: current.identity.clone(),
        };
        storage.record_processes(&transaction.transaction_id, &current, &fake_codex).unwrap();
        let transaction = storage.transaction(&transaction.transaction_id).unwrap();
        assert!(!matches_recorded(&transaction, false, &current.boot_id).unwrap());
        assert!(
            !matches_recorded(&transaction, true, &format!("different-{}", Uuid::new_v4()))
                .unwrap()
        );
    }
}
