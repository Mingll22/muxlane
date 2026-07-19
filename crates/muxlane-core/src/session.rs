use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde_json::Value;

use crate::{
    CoreError, CoreResult,
    layout::{ensure_no_symlink_components, validate_id},
    model::ThreadIndex,
    storage::{Storage, now},
};

const MAX_SESSION_FILES: usize = 10_000;
const MAX_SESSION_FILE_BYTES: u64 = 16 * 1024 * 1024;

pub fn refresh(storage: &Storage, project_id: &str) -> CoreResult<Vec<ThreadIndex>> {
    validate_id(project_id)?;
    let project = storage.project(project_id)?;
    let home = storage.layout().codex_home(project_id)?;
    let sessions = home.join("sessions");
    let codex_version = codex_version();
    let mut paths = Vec::new();
    if sessions.exists() {
        collect_jsonl(&sessions, &mut paths)?;
    }
    let mut indexes = Vec::new();
    for path in paths {
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.len() > MAX_SESSION_FILE_BYTES {
            continue;
        }
        ensure_no_symlink_components(&path, false)?;
        let bytes = fs::read(&path)?;
        let Some((thread_id, cwd)) = session_metadata(&bytes) else { continue };
        if cwd.as_deref().is_some_and(|cwd| cwd != project.canonical_wsl_path) {
            continue;
        }
        validate_id(&thread_id)?;
        let relative = path
            .strip_prefix(&home)
            .map_err(|_| CoreError::new("PATH_REJECTED", "session path escaped Project Runtime"))?
            .to_string_lossy()
            .into_owned();
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |value| value.as_secs() as i64);
        indexes.push(ThreadIndex {
            thread_id,
            project_id: project_id.to_owned(),
            source_relative_path: relative,
            source_modified_at: modified,
            codex_version: codex_version.clone(),
            status: "discovered".to_owned(),
            indexed_at: now(),
        });
    }
    storage.replace_thread_indexes(project_id, &indexes)?;
    storage.list_thread_indexes(project_id)
}

fn collect_jsonl(directory: &Path, output: &mut Vec<PathBuf>) -> CoreResult<()> {
    ensure_no_symlink_components(directory, false)?;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() {
            return Err(CoreError::new("PATH_REJECTED", "session tree contains a symbolic link"));
        }
        if metadata.is_dir() {
            collect_jsonl(&entry.path(), output)?;
        } else if entry.path().extension().and_then(|value| value.to_str()) == Some("jsonl") {
            if output.len() >= MAX_SESSION_FILES {
                return Err(CoreError::new("RESOURCE_LIMIT", "session index file limit exceeded"));
            }
            output.push(entry.path());
        }
    }
    Ok(())
}

fn session_metadata(bytes: &[u8]) -> Option<(String, Option<String>)> {
    bytes.split(|byte| *byte == b'\n').take(32).find_map(|line| {
        let value: Value = serde_json::from_slice(line).ok()?;
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            return None;
        }
        let payload = value.get("payload")?;
        let id = payload.get("id")?.as_str()?.to_owned();
        let cwd = payload.get("cwd").and_then(Value::as_str).map(str::to_owned);
        Some((id, cwd))
    })
}

fn codex_version() -> Option<String> {
    let output = Command::new("codex").arg("--version").stdin(Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{service::register_project, storage::Storage};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn indexes_only_project_local_session_metadata() {
        let temp = tempdir().unwrap();
        let storage =
            Storage::open(crate::layout::Layout::initialize(temp.path().join("runtime")).unwrap())
                .unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        let project = register_project(&storage, &source, "fixture").unwrap();
        let sessions =
            storage.layout().codex_home(&project.project_id).unwrap().join("sessions/2026/01");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(sessions.join("valid.jsonl"), format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"thread_abc\",\"cwd\":{:?}}}}}\nsecret body", project.canonical_wsl_path)).unwrap();
        fs::write(sessions.join("foreign.jsonl"), "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread_foreign\",\"cwd\":\"/other\"}}\n").unwrap();
        let indexes = refresh(&storage, &project.project_id).unwrap();
        assert_eq!(indexes.len(), 1);
        assert_eq!(indexes[0].thread_id, "thread_abc");
        let db = fs::read(storage.layout().database()).unwrap();
        assert!(!String::from_utf8_lossy(&db).contains("secret body"));
    }
}
