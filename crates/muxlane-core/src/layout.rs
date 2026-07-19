use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

use nix::unistd::Uid;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{CoreError, CoreResult};

pub const MAX_CREDENTIAL_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    pub fn discover_root() -> CoreResult<PathBuf> {
        let root = if let Some(value) = env::var_os("MUXLANE_DATA_DIR") {
            PathBuf::from(value)
        } else if let Some(value) = env::var_os("XDG_DATA_HOME") {
            PathBuf::from(value).join("muxlane")
        } else {
            let home = env::var_os("HOME")
                .ok_or_else(|| CoreError::new("PATH_REJECTED", "HOME is unavailable"))?;
            PathBuf::from(home).join(".local/share/muxlane")
        };
        if !root.is_absolute()
            || root == Path::new("/")
            || root.starts_with("/mnt")
            || root.components().any(|part| matches!(part, Component::ParentDir))
        {
            return Err(CoreError::new("PATH_REJECTED", "runtime root is not allowed"));
        }
        Ok(root)
    }

    pub fn discover() -> CoreResult<Self> {
        Self::initialize(Self::discover_root()?)
    }

    pub fn initialize(root: impl Into<PathBuf>) -> CoreResult<Self> {
        let root = root.into();
        if !root.is_absolute()
            || root == Path::new("/")
            || root.starts_with("/mnt")
            || root.components().any(|part| matches!(part, Component::ParentDir))
        {
            return Err(CoreError::new("PATH_REJECTED", "runtime root is not allowed"));
        }
        ensure_no_symlink_components(&root, true)?;
        create_private_dir(&root)?;
        let layout = Self { root };
        for path in [
            layout.accounts(),
            layout.projects(),
            layout.locks().join("accounts"),
            layout.locks().join("projects"),
            layout.transactions(),
            layout.recovery(),
            layout.logs(),
            layout.diagnostics(),
            layout.run(),
        ] {
            create_private_dir(&path)?;
        }
        Ok(layout)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn database(&self) -> PathBuf {
        self.root.join("muxlane.db")
    }

    pub fn accounts(&self) -> PathBuf {
        self.root.join("accounts")
    }

    pub fn projects(&self) -> PathBuf {
        self.root.join("projects")
    }

    pub fn locks(&self) -> PathBuf {
        self.root.join("locks")
    }

    pub fn transactions(&self) -> PathBuf {
        self.root.join("transactions")
    }

    pub fn recovery(&self) -> PathBuf {
        self.root.join("recovery")
    }

    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn diagnostics(&self) -> PathBuf {
        self.root.join("diagnostics")
    }

    pub fn run(&self) -> PathBuf {
        self.root.join("run")
    }

    pub fn socket(&self) -> PathBuf {
        self.run().join("muxlaned.sock")
    }

    pub fn daemon_lock(&self) -> PathBuf {
        self.run().join("daemon.lock")
    }

    pub fn account_dir(&self, account_id: &str) -> CoreResult<PathBuf> {
        validate_id(account_id)?;
        Ok(self.accounts().join(account_id))
    }

    pub fn vault_auth(&self, account_id: &str) -> CoreResult<PathBuf> {
        Ok(self.account_dir(account_id)?.join("auth.json"))
    }

    pub fn query_home(&self, account_id: &str) -> CoreResult<PathBuf> {
        Ok(self.account_dir(account_id)?.join("query-home"))
    }

    pub fn project_dir(&self, project_id: &str) -> CoreResult<PathBuf> {
        validate_id(project_id)?;
        Ok(self.projects().join(project_id))
    }

    pub fn codex_home(&self, project_id: &str) -> CoreResult<PathBuf> {
        Ok(self.project_dir(project_id)?.join("codex-home"))
    }

    pub fn account_lock(&self, account_id: &str) -> CoreResult<PathBuf> {
        validate_id(account_id)?;
        Ok(self.locks().join("accounts").join(format!("{account_id}.lock")))
    }

    pub fn project_lock(&self, project_id: &str) -> CoreResult<PathBuf> {
        validate_id(project_id)?;
        Ok(self.locks().join("projects").join(format!("{project_id}.lock")))
    }

    pub fn ensure_account(&self, account_id: &str) -> CoreResult<PathBuf> {
        let directory = self.account_dir(account_id)?;
        create_private_dir(&directory)?;
        create_private_dir(&directory.join("query-home"))?;
        Ok(directory)
    }

    pub fn ensure_project(&self, project_id: &str) -> CoreResult<PathBuf> {
        let directory = self.project_dir(project_id)?;
        create_private_dir(&directory)?;
        create_private_dir(&directory.join("codex-home"))?;
        create_private_dir(&directory.join("logs"))?;
        Ok(directory)
    }
}

pub fn validate_id(value: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > 96
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
    {
        return Err(CoreError::new("INVALID_REQUEST", "resource identifier is invalid"));
    }
    Ok(())
}

pub fn create_private_dir(path: &Path) -> CoreResult<()> {
    if path.exists() {
        validate_private_dir(path)?;
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::new("PATH_REJECTED", "controlled directory has no parent"))?;
    if !parent.exists() {
        create_private_dir(parent)?;
    }
    ensure_no_symlink_components(parent, false)?;
    fs::create_dir(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    sync_directory(parent)?;
    validate_private_dir(path)
}

pub fn validate_private_dir(path: &Path) -> CoreResult<()> {
    ensure_no_symlink_components(path, false)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir()
        || metadata.uid() != Uid::current().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(CoreError::new(
            "PERMISSION_DENIED",
            "controlled directory ownership or mode is unsafe",
        ));
    }
    Ok(())
}

pub fn ensure_no_symlink_components(path: &Path, allow_missing_tail: bool) -> CoreResult<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(CoreError::new("PATH_REJECTED", "symbolic links are not allowed"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && allow_missing_tail => {
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub fn open_private_regular(path: &Path) -> CoreResult<File> {
    ensure_no_symlink_components(path, false)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != Uid::current().as_raw()
        || metadata.mode() & 0o777 != 0o600
        || metadata.len() > MAX_CREDENTIAL_BYTES
    {
        return Err(CoreError::new("PERMISSION_DENIED", "credential file metadata is unsafe"));
    }
    Ok(file)
}

pub fn read_valid_json(path: &Path) -> CoreResult<(Vec<u8>, String)> {
    let mut file = open_private_regular(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
        CoreError::new("INVALID_CREDENTIAL", "credential JSON is incomplete or invalid")
    })?;
    if !value.is_object() {
        return Err(CoreError::new("INVALID_CREDENTIAL", "credential JSON must be an object"));
    }
    Ok((bytes.clone(), hex_sha256(&bytes)))
}

pub fn read_source_json(path: &Path) -> CoreResult<(Vec<u8>, String)> {
    if !path.is_absolute() {
        return Err(CoreError::new("PATH_REJECTED", "credential source must be absolute"));
    }
    ensure_no_symlink_components(path, false)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| {
            CoreError::new("PATH_REJECTED", "credential source cannot be opened safely")
        })?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != Uid::current().as_raw()
        || metadata.len() > MAX_CREDENTIAL_BYTES
    {
        return Err(CoreError::new("PATH_REJECTED", "credential source metadata is unsafe"));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
        CoreError::new("INVALID_CREDENTIAL", "credential JSON is incomplete or invalid")
    })?;
    if !parsed.is_object() {
        return Err(CoreError::new("INVALID_CREDENTIAL", "credential JSON must be an object"));
    }
    Ok((bytes.clone(), hex_sha256(&bytes)))
}

pub fn atomic_write_private(path: &Path, bytes: &[u8], overwrite: bool) -> CoreResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::new("PATH_REJECTED", "controlled file has no parent"))?;
    validate_private_dir(parent)?;
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || (!overwrite && metadata.is_file()))
    {
        return Err(CoreError::new("CONFLICT", "controlled target already exists or is unsafe"));
    }
    let temporary = parent.join(format!(".tmp-{}", Uuid::new_v4().simple()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(&temporary)?;
    let outcome = (|| -> CoreResult<()> {
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        sync_directory(parent)?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    outcome
}

pub fn remove_private_file(path: &Path) -> CoreResult<()> {
    if !path.exists() {
        return Ok(());
    }
    let _ = open_private_regular(path)?;
    fs::remove_file(path)?;
    sync_directory(path.parent().ok_or_else(|| CoreError::new("PATH_REJECTED", "invalid path"))?)
}

pub fn sync_directory(path: &Path) -> CoreResult<()> {
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_DIRECTORY | nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(path)?;
    directory.sync_all()?;
    Ok(())
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

pub fn file_hash(path: &Path) -> CoreResult<String> {
    let mut file = open_private_regular(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    file.seek(SeekFrom::Start(0))?;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex_bytes(&hasher.finalize()))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn private_atomic_write_is_durable_and_rejects_symlink_target() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("runtime");
        let layout = Layout::initialize(&root).unwrap();
        let account = layout.ensure_account("account-1").unwrap();
        let auth = account.join("auth.json");
        atomic_write_private(&auth, br#"{"fixture":"alpha"}"#, false).unwrap();
        assert_eq!(read_valid_json(&auth).unwrap().0, br#"{"fixture":"alpha"}"#);
        remove_private_file(&auth).unwrap();
        let outside = temp.path().join("outside");
        fs::write(&outside, "unchanged").unwrap();
        symlink(&outside, &auth).unwrap();
        assert_eq!(atomic_write_private(&auth, b"{}", true).unwrap_err().code, "CONFLICT");
        assert_eq!(fs::read_to_string(outside).unwrap(), "unchanged");
    }

    #[test]
    fn runtime_root_rejects_mnt_and_symlink_components() {
        assert_eq!(Layout::initialize("/mnt/c/muxlane").unwrap_err().code, "PATH_REJECTED");
        let temp = tempdir().unwrap();
        let actual = temp.path().join("actual");
        fs::create_dir(&actual).unwrap();
        fs::set_permissions(&actual, fs::Permissions::from_mode(0o700)).unwrap();
        let linked = temp.path().join("linked");
        symlink(&actual, &linked).unwrap();
        assert_eq!(Layout::initialize(linked).unwrap_err().code, "PATH_REJECTED");
    }
}
