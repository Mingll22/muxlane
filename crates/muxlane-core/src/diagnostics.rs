use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    CoreResult,
    layout::atomic_write_private,
    storage::{SCHEMA_VERSION, Storage, now},
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DiagnosticReceipt {
    pub export_id: String,
    pub relative_path: String,
    pub created_at: i64,
}

pub fn export(storage: &Storage) -> CoreResult<DiagnosticReceipt> {
    let (accounts, projects, launches, incidents) = storage.counts()?;
    let export_id = format!("diagnostics_{}", Uuid::new_v4().simple());
    let file_name = format!("{export_id}.json");
    let value = json!({
        "schema": 1,
        "created_at": now(),
        "muxlane_version": env!("CARGO_PKG_VERSION"),
        "database_schema_version": storage.schema_version()?,
        "expected_schema_version": SCHEMA_VERSION,
        "database_integrity": storage.integrity()?,
        "counts": {
            "accounts": accounts,
            "projects": projects,
            "launches": launches,
            "recovery_incidents": incidents
        },
        "privacy": {
            "credentials_included": false,
            "prompts_included": false,
            "terminal_content_included": false,
            "source_paths_included": false,
            "telemetry_uploaded": false
        }
    });
    atomic_write_private(
        &storage.layout().diagnostics().join(&file_name),
        &serde_json::to_vec_pretty(&value)?,
        false,
    )?;
    Ok(DiagnosticReceipt {
        export_id,
        relative_path: format!("diagnostics/{file_name}"),
        created_at: now(),
    })
}

pub fn append_event(
    storage: &Storage,
    event_type: &str,
    resource_id: Option<&str>,
) -> CoreResult<()> {
    let value = json!({
        "event_id": format!("event_{}", Uuid::new_v4().simple()),
        "event_type": safe_atom(event_type),
        "resource_id": resource_id.map(safe_atom),
        "created_at": now()
    });
    let path = storage.layout().logs().join("daemon.jsonl");
    let mut previous = if path.exists() { fs::read(&path)? } else { Vec::new() };
    previous.extend(serde_json::to_vec(&value)?);
    previous.push(b'\n');
    if previous.len() > 2 * 1024 * 1024 {
        let start = previous.len() - 1024 * 1024;
        previous = previous[start..].to_vec();
    }
    atomic_write_private(&path, &previous, path.exists())
}

fn safe_atom(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
        .take(128)
        .collect()
}

pub fn receipt_path(storage: &Storage, receipt: &DiagnosticReceipt) -> PathBuf {
    storage.layout().root().join(&receipt.relative_path)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::layout::Layout;

    use super::*;

    #[test]
    fn diagnostic_export_excludes_secrets_paths_prompts_and_terminal_content() {
        let temp = tempdir().unwrap();
        let storage =
            Storage::open(Layout::initialize(temp.path().join("muxlane")).unwrap()).unwrap();
        let receipt = export(&storage).unwrap();
        let text = fs::read_to_string(receipt_path(&storage, &receipt)).unwrap();
        assert!(text.contains("\"credentials_included\": false"));
        assert!(!text.contains(temp.path().to_string_lossy().as_ref()));
        assert!(!text.contains("Authorization"));
        assert!(!text.contains("auth.json"));
    }
}
