use std::{fs, path::Path};

use sha2::{Digest, Sha256};

use crate::{
    CoreError, CoreResult,
    layout::hex_sha256,
    model::{LaunchTransaction, ProcessIdentity},
};

pub fn boot_id() -> CoreResult<String> {
    let value = fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|_| {
        CoreError::new("PROCESS_IDENTITY_UNCONFIRMED", "Linux boot identity is unavailable")
    })?;
    let value = value.trim();
    if value.len() < 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit() || byte == b'-') {
        return Err(CoreError::new(
            "PROCESS_IDENTITY_UNCONFIRMED",
            "Linux boot identity is invalid",
        ));
    }
    Ok(value.to_owned())
}

pub fn inspect_process(pid: u32) -> CoreResult<ProcessIdentity> {
    let proc_root = Path::new("/proc").join(pid.to_string());
    let stat = fs::read_to_string(proc_root.join("stat")).map_err(|_| {
        CoreError::new("PROCESS_IDENTITY_UNCONFIRMED", "process identity is unavailable")
    })?;
    let closing = stat.rfind(')').ok_or_else(|| {
        CoreError::new("PROCESS_IDENTITY_UNCONFIRMED", "process stat is malformed")
    })?;
    let fields: Vec<&str> = stat[closing + 1..].split_whitespace().collect();
    // starttime is field 22; fields begins at the original field 3.
    let start_ticks =
        fields.get(19).and_then(|value| value.parse::<u64>().ok()).ok_or_else(|| {
            CoreError::new("PROCESS_IDENTITY_UNCONFIRMED", "process start time is invalid")
        })?;
    let executable = fs::read_link(proc_root.join("exe")).map_err(|_| {
        CoreError::new("PROCESS_IDENTITY_UNCONFIRMED", "process executable is unavailable")
    })?;
    let basename = executable.file_name().and_then(|value| value.to_str()).ok_or_else(|| {
        CoreError::new("PROCESS_IDENTITY_UNCONFIRMED", "process executable is invalid")
    })?;
    let executable_bytes = fs::read(&executable).map_err(|_| {
        CoreError::new("PROCESS_IDENTITY_UNCONFIRMED", "process executable cannot be hashed")
    })?;
    let mut hasher = Sha256::new();
    hasher.update(b"muxlane-process-image-v1\0");
    hasher.update(basename.as_bytes());
    hasher.update([0]);
    hasher.update(executable_bytes);
    Ok(ProcessIdentity {
        pid,
        boot_id: boot_id()?,
        start_ticks,
        identity: hex_sha256(&hasher.finalize()),
    })
}

pub fn matches_recorded(
    transaction: &LaunchTransaction,
    runner: bool,
    current_boot_id: &str,
) -> CoreResult<bool> {
    if transaction.boot_id.as_deref() != Some(current_boot_id) {
        return Ok(false);
    }
    let (pid, ticks, identity) = if runner {
        (
            transaction.runner_pid,
            transaction.runner_start_ticks,
            transaction.runner_identity.as_deref(),
        )
    } else {
        (
            transaction.codex_pid,
            transaction.codex_start_ticks,
            transaction.codex_identity.as_deref(),
        )
    };
    let (Some(pid), Some(ticks), Some(identity)) = (pid, ticks, identity) else {
        return Ok(false);
    };
    let observed = match inspect_process(pid) {
        Ok(observed) => observed,
        Err(_) if !Path::new("/proc").join(pid.to_string()).exists() => return Ok(false),
        Err(error) => return Err(error),
    };
    Ok(observed.boot_id == current_boot_id
        && observed.start_ticks == ticks
        && observed.identity == identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_uses_boot_pid_start_ticks_and_executable_digest() {
        let observed = inspect_process(std::process::id()).unwrap();
        assert_eq!(observed.pid, std::process::id());
        assert_eq!(observed.boot_id, boot_id().unwrap());
        assert!(observed.start_ticks > 0);
        assert_eq!(observed.identity.len(), 64);
    }
}
