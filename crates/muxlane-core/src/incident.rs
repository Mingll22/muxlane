use std::{fs, path::Path};

use crate::{
    CoreError, CoreResult,
    layout::{atomic_write_private, create_private_dir, read_valid_json, remove_private_file},
    lock::LaunchLocks,
    model::RecoveryIncident,
    storage::Storage,
};

pub const KEEP_VAULT: &str = "keep_vault";
pub const USE_EVIDENCE: &str = "use_evidence";

/// Resolve an incident only after an explicit, auditable credential choice.
/// The original terminal Launch Transaction is intentionally never rewritten.
pub fn resolve(storage: &Storage, incident_id: &str, action: &str) -> CoreResult<RecoveryIncident> {
    let incident = storage.incident(incident_id)?;
    if incident.status == "resolved" {
        if incident.resolution_action.as_deref() == Some(action) {
            return Ok(incident);
        }
        return Err(CoreError::new("CONFLICT", "incident was resolved with another action"));
    }
    if !matches!(action, KEEP_VAULT | USE_EVIDENCE) {
        return Err(CoreError::new("INVALID_REQUEST", "incident resolution action is invalid"));
    }
    let _locks =
        LaunchLocks::try_acquire(storage.layout(), &incident.account_id, &incident.project_id)?;
    let attempt_id = storage.begin_resolution_attempt(incident_id, action)?;
    let result = apply_resolution(storage, &incident, action);
    match result {
        Ok(summary) => {
            let resolved = storage.resolve_incident(incident_id, action, summary)?;
            storage.complete_resolution_attempt(&attempt_id, "resolved", None)?;
            Ok(resolved)
        }
        Err(error) => {
            storage.complete_resolution_attempt(&attempt_id, "failed", Some(error.code))?;
            Err(error)
        }
    }
}

fn apply_resolution<'a>(
    storage: &Storage,
    incident: &RecoveryIncident,
    action: &'a str,
) -> CoreResult<&'a str> {
    let vault = storage.layout().vault_auth(&incident.account_id)?;
    let runtime = storage.layout().codex_home(&incident.project_id)?.join("auth.json");
    let _ = read_valid_json(&vault)?;
    if action == KEEP_VAULT {
        if runtime.exists() {
            let _ = read_valid_json(&runtime)?;
            remove_private_file(&runtime)?;
        }
        return Ok("validated Vault retained; Runtime credential cleared");
    }

    let relative = incident.evidence_relative_path.as_deref().ok_or_else(|| {
        CoreError::new("RECOVERY_REQUIRED", "incident has no credential evidence candidate")
    })?;
    let evidence = safe_evidence_path(storage, relative)?;
    let (candidate, _) = read_valid_json(&evidence)?;
    let (current, _) = read_valid_json(&vault)?;
    let backup_dir = storage.layout().recovery().join(&incident.transaction_id);
    create_private_dir(&backup_dir)?;
    let backup = backup_dir.join(format!("vault-before-resolution-{}.json", incident.incident_id));
    if !backup.exists() {
        atomic_write_private(&backup, &current, false)?;
    }
    atomic_write_private(&vault, &candidate, true)?;
    if runtime.exists() {
        remove_private_file(&runtime)?;
    }
    Ok("validated evidence selected; prior Vault preserved; Runtime credential cleared")
}

fn safe_evidence_path(storage: &Storage, relative: &str) -> CoreResult<std::path::PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(part, std::path::Component::ParentDir | std::path::Component::RootDir)
        })
        || relative.extension().and_then(|value| value.to_str()) != Some("json")
    {
        return Err(CoreError::new("PATH_REJECTED", "incident evidence path is invalid"));
    }
    let path = storage.layout().root().join(relative);
    let canonical = fs::canonicalize(&path)
        .map_err(|_| CoreError::new("NOT_FOUND", "incident evidence is unavailable"))?;
    let recovery = fs::canonicalize(storage.layout().recovery())?;
    if !canonical.starts_with(recovery) {
        return Err(CoreError::new("PATH_REJECTED", "incident evidence escaped recovery root"));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use tempfile::tempdir;

    use crate::{
        credential::{checkout, commit_and_clean, import_account},
        service::{prepare_launch, register_project},
        storage::Storage,
    };

    use super::*;

    #[test]
    fn explicit_keep_vault_is_idempotent_and_audited() {
        let temp = tempdir().unwrap();
        let auth = temp.path().join("auth.json");
        fs::write(&auth, br#"{"fixture":"one"}"#).unwrap();
        fs::set_permissions(&auth, fs::Permissions::from_mode(0o600)).unwrap();
        let storage =
            Storage::open(crate::layout::Layout::initialize(temp.path().join("runtime")).unwrap())
                .unwrap();
        let account = import_account(&storage, &auth, "fixture").unwrap();
        let source = temp.path().join("project");
        fs::create_dir(&source).unwrap();
        let project = register_project(&storage, &source, "fixture").unwrap();
        let transaction =
            prepare_launch(&storage, &account.account_id, &project.project_id).unwrap();
        checkout(&storage, &transaction.transaction_id).unwrap();
        fs::write(
            storage.layout().vault_auth(&account.account_id).unwrap(),
            br#"{"fixture":"vault-new"}"#,
        )
        .unwrap();
        fs::write(
            storage.layout().codex_home(&project.project_id).unwrap().join("auth.json"),
            br#"{"fixture":"runtime-new"}"#,
        )
        .unwrap();
        commit_and_clean(&storage, &transaction.transaction_id).unwrap();
        let incident = storage.list_incidents(false).unwrap().remove(0);
        let resolved = resolve(&storage, &incident.incident_id, KEEP_VAULT).unwrap();
        assert_eq!(resolved.status, "resolved");
        assert_eq!(resolve(&storage, &incident.incident_id, KEEP_VAULT).unwrap(), resolved);
        assert_eq!(storage.list_recovery_attempts(&incident.incident_id).unwrap().len(), 2);
    }
}
