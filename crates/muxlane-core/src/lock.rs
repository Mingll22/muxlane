use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

use fs4::{FileExt, TryLockError};

use crate::{CoreError, CoreResult, layout::Layout};

#[derive(Debug)]
pub struct ManagedLock {
    _file: File,
}

impl ManagedLock {
    pub fn try_acquire(path: &Path, conflict_code: &'static str) -> CoreResult<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
            .open(path)?;
        match FileExt::try_lock(&file) {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => {
                Err(CoreError::new(conflict_code, "resource has an active flock owner"))
            }
            Err(TryLockError::Error(error)) => Err(CoreError::io(error)),
        }
    }
}

/// The field order documents and enforces Account -> Project acquisition.
#[derive(Debug)]
pub struct LaunchLocks {
    _account: ManagedLock,
    _project: ManagedLock,
}

impl LaunchLocks {
    pub fn try_acquire(layout: &Layout, account_id: &str, project_id: &str) -> CoreResult<Self> {
        let account =
            ManagedLock::try_acquire(&layout.account_lock(account_id)?, "ACCOUNT_IN_USE")?;
        let project =
            ManagedLock::try_acquire(&layout.project_lock(project_id)?, "PROJECT_IN_USE")?;
        Ok(Self { _account: account, _project: project })
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn flock_is_the_active_exclusion_fact_and_lock_files_can_remain() {
        let temp = tempdir().unwrap();
        let layout = Layout::initialize(temp.path().join("muxlane")).unwrap();
        let first = LaunchLocks::try_acquire(&layout, "account-a", "project-a").unwrap();
        assert_eq!(
            LaunchLocks::try_acquire(&layout, "account-a", "project-b").unwrap_err().code,
            "ACCOUNT_IN_USE"
        );
        assert!(layout.account_lock("account-a").unwrap().exists());
        drop(first);
        LaunchLocks::try_acquire(&layout, "account-a", "project-a").unwrap();
        assert!(layout.account_lock("account-a").unwrap().exists());
    }

    #[test]
    fn project_lock_rejects_a_second_account_for_same_project() {
        let temp = tempdir().unwrap();
        let layout = Layout::initialize(temp.path().join("muxlane")).unwrap();
        let _first = LaunchLocks::try_acquire(&layout, "account-a", "project-a").unwrap();
        assert_eq!(
            LaunchLocks::try_acquire(&layout, "account-b", "project-a").unwrap_err().code,
            "PROJECT_IN_USE"
        );
    }
}
