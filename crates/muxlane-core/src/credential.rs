use std::{io::Read, path::Path};

use uuid::Uuid;

use crate::{
    CoreError, CoreResult,
    layout::{
        Layout, atomic_write_private, create_private_dir, file_hash, open_private_regular,
        read_source_json, read_valid_json, remove_private_file,
    },
    model::{Account, LaunchTransaction, TransactionState},
    storage::{Storage, now},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialDisposition {
    Unchanged,
    RuntimeCommitted,
    NewerVaultPreserved,
    Conflict,
}

pub fn import_account(storage: &Storage, source: &Path, display_name: &str) -> CoreResult<Account> {
    if display_name.trim().is_empty()
        || display_name.len() > 120
        || display_name.chars().any(char::is_control)
    {
        return Err(CoreError::new("INVALID_REQUEST", "account display name is invalid"));
    }
    let (bytes, hash) = read_source_json(source)?;
    let account_id = format!("account_{}", Uuid::new_v4().simple());
    storage.layout().ensure_account(&account_id)?;
    atomic_write_private(&storage.layout().vault_auth(&account_id)?, &bytes, false)?;
    let timestamp = now();
    let account = Account {
        account_id: account_id.clone(),
        display_name: display_name.trim().to_owned(),
        masked_email: None,
        plan_type: None,
        login_status: "unknown".to_owned(),
        credential_hash: hash,
        occupied: false,
        created_at: timestamp,
        updated_at: timestamp,
    };
    if let Err(error) =
        storage.insert_account(&account, &format!("accounts/{account_id}/auth.json"))
    {
        let _ = remove_private_file(&storage.layout().vault_auth(&account_id)?);
        return Err(error);
    }
    Ok(account)
}

pub fn checkout(storage: &Storage, transaction_id: &str) -> CoreResult<()> {
    let transaction = storage.transaction(transaction_id)?;
    if transaction.state != TransactionState::Preparing {
        return Err(CoreError::new("INVALID_STATE", "credential checkout is not allowed"));
    }
    let vault = storage.layout().vault_auth(&transaction.account_id)?;
    let runtime = storage.layout().codex_home(&transaction.project_id)?.join("auth.json");
    if runtime.exists() {
        return Err(CoreError::new(
            "RECOVERY_REQUIRED",
            "project runtime already contains an active credential",
        ));
    }
    let (bytes, vault_hash) = read_valid_json(&vault)?;
    atomic_write_private(&runtime, &bytes, false)?;
    let runtime_hash = file_hash(&runtime)?;
    if runtime_hash != vault_hash {
        return Err(CoreError::new("STORAGE_FAILURE", "credential checkout verification failed"));
    }
    storage.record_checkout(transaction_id, &vault_hash, &runtime_hash)
}

pub fn commit_and_clean(
    storage: &Storage,
    transaction_id: &str,
) -> CoreResult<CredentialDisposition> {
    let transaction = storage.transaction(transaction_id)?;
    if !matches!(
        transaction.state,
        TransactionState::CheckedOut
            | TransactionState::CodexExited
            | TransactionState::CommittingAuth
    ) {
        return Err(CoreError::new("INVALID_STATE", "credential commit is not allowed"));
    }
    if transaction.state != TransactionState::CommittingAuth {
        storage.transition(transaction_id, TransactionState::CommittingAuth)?;
    }
    commit_from_hash_matrix(storage, &storage.transaction(transaction_id)?)
}

fn commit_from_hash_matrix(
    storage: &Storage,
    transaction: &LaunchTransaction,
) -> CoreResult<CredentialDisposition> {
    let baseline = transaction
        .vault_hash_before_checkout
        .as_deref()
        .ok_or_else(|| CoreError::new("INVALID_STATE", "checkout hash evidence is missing"))?;
    let vault_path = storage.layout().vault_auth(&transaction.account_id)?;
    let runtime_path = storage.layout().codex_home(&transaction.project_id)?.join("auth.json");
    let vault = read_valid_json(&vault_path);
    let runtime = read_valid_json(&runtime_path);
    let (vault_bytes, vault_hash) = match vault {
        Ok(value) => value,
        Err(_) => {
            let evidence =
                preserve_runtime(storage.layout(), transaction, &runtime_path, "vault-invalid")?;
            storage.record_backup(&transaction.transaction_id, &evidence)?;
            storage.fail(
                &transaction.transaction_id,
                "INVALID_CREDENTIAL",
                "Vault credential validation failed; Runtime evidence was isolated",
            )?;
            return Err(CoreError::new("INVALID_CREDENTIAL", "Vault credential is invalid"));
        }
    };
    let (runtime_bytes, runtime_hash) = match runtime {
        Ok(value) => value,
        Err(_) => {
            let evidence =
                preserve_runtime(storage.layout(), transaction, &runtime_path, "runtime-invalid")?;
            storage.record_backup(&transaction.transaction_id, &evidence)?;
            storage.fail(
                &transaction.transaction_id,
                "INVALID_CREDENTIAL",
                "Runtime credential validation failed and was isolated",
            )?;
            return Err(CoreError::new("INVALID_CREDENTIAL", "Runtime credential is invalid"));
        }
    };
    storage.record_recovery_hashes(
        &transaction.transaction_id,
        Some(&vault_hash),
        Some(&runtime_hash),
    )?;

    let disposition = if runtime_hash == baseline {
        if vault_hash == baseline {
            CredentialDisposition::Unchanged
        } else {
            CredentialDisposition::NewerVaultPreserved
        }
    } else if vault_hash == runtime_hash {
        CredentialDisposition::RuntimeCommitted
    } else if vault_hash == baseline {
        let evidence =
            preserve_bytes(storage.layout(), transaction, "vault-before-commit", &vault_bytes)?;
        storage.record_backup(&transaction.transaction_id, &evidence)?;
        atomic_write_private(&vault_path, &runtime_bytes, true)?;
        if file_hash(&vault_path)? != runtime_hash {
            return Err(CoreError::new("STORAGE_FAILURE", "credential commit verification failed"));
        }
        CredentialDisposition::RuntimeCommitted
    } else {
        let evidence =
            preserve_runtime(storage.layout(), transaction, &runtime_path, "credential-conflict")?;
        storage.record_backup(&transaction.transaction_id, &evidence)?;
        storage.transition(&transaction.transaction_id, TransactionState::CredentialConflict)?;
        let incident = storage.create_incident(
            &transaction.transaction_id,
            "credential_conflict",
            Some(&evidence),
        )?;
        storage.record_recovery_attempt(
            &incident,
            &transaction.transaction_id,
            "vault_and_runtime_changed",
            TransactionState::CredentialConflict,
            Some("CREDENTIAL_CONFLICT"),
        )?;
        storage.finish_launch(&transaction.transaction_id, "credential_conflict")?;
        return Ok(CredentialDisposition::Conflict);
    };

    storage.transition(&transaction.transaction_id, TransactionState::AuthCommitted)?;
    remove_private_file(&runtime_path)?;
    storage.transition(&transaction.transaction_id, TransactionState::Cleaned)?;
    Ok(disposition)
}

fn preserve_runtime(
    layout: &Layout,
    transaction: &LaunchTransaction,
    runtime_path: &Path,
    label: &str,
) -> CoreResult<String> {
    let mut file = open_private_regular(runtime_path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let relative = preserve_bytes(layout, transaction, label, &bytes)?;
    remove_private_file(runtime_path)?;
    Ok(relative)
}

fn preserve_bytes(
    layout: &Layout,
    transaction: &LaunchTransaction,
    label: &str,
    bytes: &[u8],
) -> CoreResult<String> {
    let directory = layout.recovery().join(&transaction.transaction_id);
    create_private_dir(&directory)?;
    let name = format!("{label}-{}.json", Uuid::new_v4().simple());
    atomic_write_private(&directory.join(&name), bytes, false)?;
    Ok(format!("recovery/{}/{name}", transaction.transaction_id))
}

pub fn runtime_auth_exists(layout: &Layout, project_id: &str) -> CoreResult<bool> {
    Ok(layout.codex_home(project_id)?.join("auth.json").exists())
}

pub fn cleanup_query_home(layout: &Layout, account_id: &str) -> CoreResult<()> {
    let auth = layout.query_home(account_id)?.join("auth.json");
    remove_private_file(&auth)
}

pub fn recover_preparing(
    storage: &Storage,
    transaction_id: &str,
) -> CoreResult<CredentialDisposition> {
    let transaction = storage.transaction(transaction_id)?;
    if transaction.state != TransactionState::Preparing {
        return Err(CoreError::new("INVALID_STATE", "preparing recovery is not allowed"));
    }
    let runtime = storage.layout().codex_home(&transaction.project_id)?.join("auth.json");
    if !runtime.exists() {
        storage.transition(transaction_id, TransactionState::Recovered)?;
        storage.finish_launch(transaction_id, "recovered_before_checkout")?;
        return Ok(CredentialDisposition::Unchanged);
    }
    let vault = storage.layout().vault_auth(&transaction.account_id)?;
    let (_, vault_hash) = read_valid_json(&vault)?;
    match read_valid_json(&runtime) {
        Ok((_, runtime_hash)) if runtime_hash == vault_hash => {
            remove_private_file(&runtime)?;
            storage.transition(transaction_id, TransactionState::Recovered)?;
            storage.finish_launch(transaction_id, "recovered_checkout_boundary")?;
            Ok(CredentialDisposition::Unchanged)
        }
        _ => {
            let evidence =
                preserve_runtime(storage.layout(), &transaction, &runtime, "preparing-ambiguous")?;
            storage.record_backup(transaction_id, &evidence)?;
            storage.fail(
                transaction_id,
                "RECOVERY_REQUIRED",
                "ambiguous credential checkout was isolated",
            )?;
            Err(CoreError::new("RECOVERY_REQUIRED", "credential checkout boundary is ambiguous"))
        }
    }
}

pub fn checkout_query_home(layout: &Layout, account_id: &str) -> CoreResult<()> {
    let (bytes, _) = read_valid_json(&layout.vault_auth(account_id)?)?;
    let query_home = layout.query_home(account_id)?;
    create_private_dir(&query_home)?;
    let auth = query_home.join("auth.json");
    if auth.exists() {
        remove_private_file(&auth)?;
    }
    atomic_write_private(&auth, &bytes, false)
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use tempfile::tempdir;

    use crate::{
        model::LaunchTransaction,
        storage::{Storage, now},
    };

    use super::*;

    fn fixture() -> (tempfile::TempDir, Storage, Account, crate::model::Project, LaunchTransaction)
    {
        let temp = tempdir().unwrap();
        let source = temp.path().join("fixture-auth.json");
        fs::write(&source, br#"{"fixture":"alpha"}"#).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        let layout = Layout::initialize(temp.path().join("muxlane")).unwrap();
        let storage = Storage::open(layout.clone()).unwrap();
        let account = import_account(&storage, &source, "Fixture").unwrap();
        let project =
            crate::service::register_project(&storage, temp.path(), "fixture-project").unwrap();
        let timestamp = now();
        let transaction = LaunchTransaction {
            transaction_id: format!("transaction_{}", Uuid::new_v4().simple()),
            launch_id: format!("launch_{}", Uuid::new_v4().simple()),
            project_id: project.project_id.clone(),
            account_id: account.account_id.clone(),
            state: TransactionState::Preparing,
            runner_pid: None,
            codex_pid: None,
            boot_id: None,
            runner_start_ticks: None,
            codex_start_ticks: None,
            runner_identity: None,
            codex_identity: None,
            vault_hash_before_checkout: None,
            runtime_hash_at_checkout: None,
            runtime_hash_at_recovery: None,
            vault_hash_at_recovery: None,
            credential_backup_reference: None,
            last_error_code: None,
            last_error_message_redacted: None,
            recovery_attempts: 0,
            created_at: timestamp,
            updated_at: timestamp,
        };
        storage.create_launch(&transaction).unwrap();
        (temp, storage, account, project, transaction)
    }

    #[test]
    fn checkout_and_commit_refreshed_runtime_atomically() {
        let (_temp, storage, account, project, transaction) = fixture();
        checkout(&storage, &transaction.transaction_id).unwrap();
        let runtime = storage.layout().codex_home(&project.project_id).unwrap().join("auth.json");
        atomic_write_private(&runtime, br#"{"fixture":"refreshed"}"#, true).unwrap();
        assert_eq!(
            commit_and_clean(&storage, &transaction.transaction_id).unwrap(),
            CredentialDisposition::RuntimeCommitted
        );
        assert!(!runtime.exists());
        assert_eq!(
            read_valid_json(&storage.layout().vault_auth(&account.account_id).unwrap()).unwrap().0,
            br#"{"fixture":"refreshed"}"#
        );
    }

    #[test]
    fn simultaneous_vault_and_runtime_change_preserves_both_and_conflicts() {
        let (_temp, storage, account, project, transaction) = fixture();
        checkout(&storage, &transaction.transaction_id).unwrap();
        let runtime = storage.layout().codex_home(&project.project_id).unwrap().join("auth.json");
        let vault = storage.layout().vault_auth(&account.account_id).unwrap();
        atomic_write_private(&runtime, br#"{"fixture":"runtime-new"}"#, true).unwrap();
        atomic_write_private(&vault, br#"{"fixture":"vault-new"}"#, true).unwrap();
        assert_eq!(
            commit_and_clean(&storage, &transaction.transaction_id).unwrap(),
            CredentialDisposition::Conflict
        );
        assert!(!runtime.exists());
        assert_eq!(read_valid_json(&vault).unwrap().0, br#"{"fixture":"vault-new"}"#);
        let final_transaction = storage.transaction(&transaction.transaction_id).unwrap();
        assert_eq!(final_transaction.state, TransactionState::CredentialConflict);
        let evidence =
            storage.layout().root().join(final_transaction.credential_backup_reference.unwrap());
        assert_eq!(read_valid_json(&evidence).unwrap().0, br#"{"fixture":"runtime-new"}"#);
    }
}
