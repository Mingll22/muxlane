//! Read-only, Project-confined workspace navigation for the desktop client.

use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::UNIX_EPOCH,
};

use crate::{
    CoreError, CoreResult,
    layout::validate_id,
    model::{WorkspaceEntry, WorkspaceLocation, WorkspacePreview},
    storage::Storage,
};

const MAX_DIRECTORY_ENTRIES: usize = 2_000;
const MAX_SEARCH_RESULTS: usize = 300;
const MAX_SEARCH_VISITS: usize = 20_000;
const MAX_PREVIEW_BYTES: u64 = 1024 * 1024;

pub fn list(
    storage: &Storage,
    project_id: &str,
    relative_directory: &str,
) -> CoreResult<Vec<WorkspaceEntry>> {
    let (root, directory) = resolve(storage, project_id, relative_directory, true)?;
    if !directory.is_dir() {
        return Err(CoreError::new("INVALID_REQUEST", "workspace location is not a directory"));
    }
    let mut entries = Vec::new();
    for item in fs::read_dir(&directory)? {
        if entries.len() >= MAX_DIRECTORY_ENTRIES {
            break;
        }
        let item = item?;
        let metadata = item.file_type()?;
        if metadata.is_symlink() {
            continue;
        }
        let path = item.path();
        let item_metadata = fs::metadata(&path)?;
        let relative = relative_text(&root, &path)?;
        entries.push(WorkspaceEntry {
            relative_path: relative,
            name: item.file_name().to_string_lossy().into_owned(),
            kind: if metadata.is_dir() { "directory" } else { "file" }.to_owned(),
            size: if metadata.is_file() { item_metadata.len() } else { 0 },
            modified_at: modified_at(&item_metadata),
        });
    }
    entries.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(entries)
}

pub fn search(storage: &Storage, project_id: &str, query: &str) -> CoreResult<Vec<WorkspaceEntry>> {
    validate_query(query)?;
    let (root, _) = resolve(storage, project_id, "", true)?;
    let query = query.to_lowercase();
    let mut pending = vec![root.clone()];
    let mut results = Vec::new();
    let mut visits = 0usize;
    while let Some(directory) = pending.pop() {
        for item in fs::read_dir(&directory)? {
            visits += 1;
            if visits > MAX_SEARCH_VISITS || results.len() >= MAX_SEARCH_RESULTS {
                return Ok(results);
            }
            let item = item?;
            let file_type = item.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            let path = item.path();
            let name = item.file_name().to_string_lossy().into_owned();
            if file_type.is_dir() && !is_ignored_directory(&name) {
                pending.push(path.clone());
            }
            if name.to_lowercase().contains(&query) {
                let metadata = fs::metadata(&path)?;
                results.push(WorkspaceEntry {
                    relative_path: relative_text(&root, &path)?,
                    name,
                    kind: if file_type.is_dir() { "directory" } else { "file" }.to_owned(),
                    size: if file_type.is_file() { metadata.len() } else { 0 },
                    modified_at: modified_at(&metadata),
                });
            }
        }
    }
    results.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(results)
}

pub fn preview(
    storage: &Storage,
    project_id: &str,
    relative_path: &str,
) -> CoreResult<WorkspacePreview> {
    let (root, path) = resolve(storage, project_id, relative_path, false)?;
    let metadata = fs::metadata(&path)?;
    if !metadata.is_file() {
        return Err(CoreError::new("INVALID_REQUEST", "workspace preview requires a file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)?.take(MAX_PREVIEW_BYTES + 1).read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 > MAX_PREVIEW_BYTES;
    bytes.truncate(MAX_PREVIEW_BYTES as usize);
    if bytes.contains(&0) {
        return Err(CoreError::new("BINARY_FILE", "binary files cannot be previewed"));
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| CoreError::new("BINARY_FILE", "file is not valid UTF-8 text"))?;
    Ok(WorkspacePreview {
        relative_path: relative_text(&root, &path)?,
        line_count: content.lines().count(),
        content,
        truncated,
        modified_at: modified_at(&metadata),
    })
}

pub fn location(
    storage: &Storage,
    project_id: &str,
    relative_path: &str,
) -> CoreResult<WorkspaceLocation> {
    let (root, path) = resolve(storage, project_id, relative_path, false)?;
    let canonical_windows_path = Command::new("wslpath")
        .args(["-w", path.to_str().unwrap_or_default()])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned());
    Ok(WorkspaceLocation {
        relative_path: relative_text(&root, &path)?,
        canonical_wsl_path: path.to_string_lossy().into_owned(),
        canonical_windows_path,
    })
}

fn resolve(
    storage: &Storage,
    project_id: &str,
    relative_path: &str,
    allow_root: bool,
) -> CoreResult<(PathBuf, PathBuf)> {
    validate_id(project_id)?;
    validate_relative(relative_path, allow_root)?;
    let project = storage.project(project_id)?;
    let root = fs::canonicalize(&project.canonical_wsl_path)
        .map_err(|_| CoreError::new("PATH_REJECTED", "Project root is unavailable"))?;
    let candidate = if relative_path.is_empty() { root.clone() } else { root.join(relative_path) };
    reject_symlink_components(&root, &candidate)?;
    let canonical = fs::canonicalize(&candidate)
        .map_err(|_| CoreError::new("NOT_FOUND", "workspace path was not found"))?;
    if !canonical.starts_with(&root) {
        return Err(CoreError::new("PATH_REJECTED", "workspace path escaped Project root"));
    }
    Ok((root, canonical))
}

fn validate_relative(value: &str, allow_empty: bool) -> CoreResult<()> {
    if (!allow_empty && value.is_empty())
        || value.len() > 4096
        || value.contains('\0')
        || Path::new(value).is_absolute()
        || Path::new(value).components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return Err(CoreError::new("PATH_REJECTED", "workspace path is invalid"));
    }
    Ok(())
}

fn reject_symlink_components(root: &Path, candidate: &Path) -> CoreResult<()> {
    let mut current = root.to_path_buf();
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| CoreError::new("PATH_REJECTED", "workspace path escaped Project root"))?;
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(CoreError::new("PATH_REJECTED", "workspace path is invalid"));
        };
        current.push(value);
        if fs::symlink_metadata(&current)
            .map_err(|_| CoreError::new("NOT_FOUND", "workspace path was not found"))?
            .file_type()
            .is_symlink()
        {
            return Err(CoreError::new("PATH_REJECTED", "symbolic links are not traversed"));
        }
    }
    Ok(())
}

fn validate_query(query: &str) -> CoreResult<()> {
    if query.trim().is_empty() || query.len() > 160 || query.chars().any(char::is_control) {
        return Err(CoreError::new("INVALID_REQUEST", "file search query is invalid"));
    }
    Ok(())
}

fn relative_text(root: &Path, path: &Path) -> CoreResult<String> {
    path.strip_prefix(root)
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .map_err(|_| CoreError::new("PATH_REJECTED", "workspace path escaped Project root"))
}

fn modified_at(metadata: &fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
}

fn is_ignored_directory(name: &str) -> bool {
    matches!(name, ".git" | "node_modules" | "target" | ".next" | "dist" | "build")
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;

    use super::*;
    use crate::{layout::Layout, service};

    #[test]
    fn confines_navigation_and_rejects_binary_and_symlinks() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("hello.txt"), "hello\n世界\n").unwrap();
        fs::write(source.join("binary.bin"), [0, 1, 2]).unwrap();
        symlink(temp.path(), source.join("escape")).unwrap();
        let storage =
            Storage::open(Layout::initialize(temp.path().join("runtime")).unwrap()).unwrap();
        let project = service::register_project(&storage, &source, "demo").unwrap();
        assert_eq!(list(&storage, &project.project_id, "").unwrap().len(), 2);
        assert_eq!(preview(&storage, &project.project_id, "hello.txt").unwrap().line_count, 2);
        assert_eq!(
            preview(&storage, &project.project_id, "binary.bin").unwrap_err().code,
            "BINARY_FILE"
        );
        assert_eq!(
            preview(&storage, &project.project_id, "escape").unwrap_err().code,
            "PATH_REJECTED"
        );
        assert_eq!(
            preview(&storage, &project.project_id, "../outside").unwrap_err().code,
            "PATH_REJECTED"
        );
    }
}
