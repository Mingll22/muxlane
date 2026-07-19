use std::{
    path::Path,
    process::{Command, Stdio},
};

use uuid::Uuid;

use crate::{
    CoreError, CoreResult,
    layout::validate_id,
    model::Terminal,
    storage::{Storage, now},
};

const TMUX_SOCKET: &str = "muxlane-runtime";
const MAX_HISTORY_BYTES: usize = 1024 * 1024;

pub fn start_managed_runner(
    storage: &Storage,
    transaction_id: &str,
    daemon_executable: &Path,
) -> CoreResult<Terminal> {
    validate_id(transaction_id)?;
    let transaction = storage.transaction(transaction_id)?;
    let project = storage.project(&transaction.project_id)?;
    let exists = tmux_status(["has-session", "-t", &project.tmux_session_name]).is_ok();
    if exists {
        verify_session_identity(&project.tmux_session_name, &project.project_id)?;
    }
    let root_env = format!("MUXLANE_DATA_DIR={}", storage.layout().root().display());
    let executable = daemon_executable.to_str().ok_or_else(|| {
        CoreError::new("PATH_REJECTED", "daemon executable encoding is unsupported")
    })?;
    let output = if exists {
        Command::new("tmux")
            .args([
                "-L",
                TMUX_SOCKET,
                "new-window",
                "-d",
                "-P",
                "-F",
                "#{window_id}",
                "-t",
                &project.tmux_session_name,
                "-n",
                "codex",
                "-e",
                &root_env,
                "--",
                executable,
                "managed-runner",
                "--transaction-id",
                transaction_id,
            ])
            .stdin(Stdio::null())
            .output()
    } else {
        Command::new("tmux")
            .args([
                "-L",
                TMUX_SOCKET,
                "new-session",
                "-d",
                "-P",
                "-F",
                "#{window_id}",
                "-s",
                &project.tmux_session_name,
                "-n",
                "codex",
                "-e",
                &root_env,
                "--",
                executable,
                "managed-runner",
                "--transaction-id",
                transaction_id,
            ])
            .stdin(Stdio::null())
            .output()
    }
    .map_err(|_| CoreError::new("CAPABILITY_UNAVAILABLE", "tmux could not be started"))?;
    if !output.status.success() {
        return Err(CoreError::new(
            "CAPABILITY_UNAVAILABLE",
            "tmux rejected managed Terminal creation",
        ));
    }
    if !exists {
        tmux_status([
            "set-option",
            "-t",
            &project.tmux_session_name,
            "@muxlane-project-id",
            &project.project_id,
        ])?;
    }
    let window_id = String::from_utf8(output.stdout)
        .map_err(|_| CoreError::new("INTERNAL_ERROR", "tmux returned invalid metadata"))?
        .trim()
        .to_owned();
    validate_window_id(&window_id)?;
    let terminal = Terminal {
        terminal_id: format!("terminal_{}", Uuid::new_v4().simple()),
        project_id: project.project_id,
        kind: "codex".to_owned(),
        display_name: "Codex".to_owned(),
        tmux_window_identity: window_id,
        lifecycle_status: "running".to_owned(),
        created_at: now(),
        closed_at: None,
    };
    let ordinal = storage.list_terminals(&terminal.project_id)?.len() as i64;
    storage.insert_terminal(&terminal, ordinal)?;
    Ok(terminal)
}

pub fn create_auxiliary(storage: &Storage, project_id: &str, name: &str) -> CoreResult<Terminal> {
    validate_id(project_id)?;
    validate_terminal_name(name)?;
    let project = storage.project(project_id)?;
    verify_session_identity(&project.tmux_session_name, project_id)?;
    let output = Command::new("tmux")
        .args([
            "-L",
            TMUX_SOCKET,
            "new-window",
            "-d",
            "-P",
            "-F",
            "#{window_id}",
            "-t",
            &project.tmux_session_name,
            "-n",
            name,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|_| CoreError::new("CAPABILITY_UNAVAILABLE", "tmux is unavailable"))?;
    if !output.status.success() {
        return Err(CoreError::new("INVALID_STATE", "tmux Terminal could not be created"));
    }
    let window_id = String::from_utf8(output.stdout)
        .map_err(|_| CoreError::new("INTERNAL_ERROR", "tmux returned invalid metadata"))?
        .trim()
        .to_owned();
    validate_window_id(&window_id)?;
    let terminal = Terminal {
        terminal_id: format!("terminal_{}", Uuid::new_v4().simple()),
        project_id: project_id.to_owned(),
        kind: "auxiliary".to_owned(),
        display_name: name.to_owned(),
        tmux_window_identity: window_id,
        lifecycle_status: "running".to_owned(),
        created_at: now(),
        closed_at: None,
    };
    let ordinal = storage.list_terminals(project_id)?.len() as i64;
    storage.insert_terminal(&terminal, ordinal)?;
    Ok(terminal)
}

pub fn history_bootstrap(storage: &Storage, terminal_id: &str) -> CoreResult<Vec<u8>> {
    validate_id(terminal_id)?;
    let terminal = storage
        .list_projects()?
        .into_iter()
        .flat_map(|project| storage.list_terminals(&project.project_id).unwrap_or_default())
        .find(|terminal| terminal.terminal_id == terminal_id)
        .ok_or_else(|| CoreError::new("NOT_FOUND", "Terminal was not found"))?;
    let project = storage.project(&terminal.project_id)?;
    verify_session_identity(&project.tmux_session_name, &project.project_id)?;
    validate_window_id(&terminal.tmux_window_identity)?;
    let target = format!("{}:{}", project.tmux_session_name, terminal.tmux_window_identity);
    let output = Command::new("tmux")
        .args(["-L", TMUX_SOCKET, "capture-pane", "-p", "-e", "-J", "-S", "-500", "-t", &target])
        .stdin(Stdio::null())
        .output()
        .map_err(|_| CoreError::new("CAPABILITY_UNAVAILABLE", "tmux history is unavailable"))?;
    if !output.status.success() {
        return Err(CoreError::new("NOT_FOUND", "Terminal history is unavailable"));
    }
    let start = output.stdout.len().saturating_sub(MAX_HISTORY_BYTES);
    Ok(output.stdout[start..].to_vec())
}

fn verify_session_identity(session: &str, project_id: &str) -> CoreResult<()> {
    let output = Command::new("tmux")
        .args(["-L", TMUX_SOCKET, "show-option", "-qv", "-t", session, "@muxlane-project-id"])
        .stdin(Stdio::null())
        .output()
        .map_err(|_| CoreError::new("CAPABILITY_UNAVAILABLE", "tmux is unavailable"))?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != project_id {
        return Err(CoreError::new("CONFLICT", "tmux session identity is not managed by Muxlane"));
    }
    Ok(())
}

fn tmux_status<const N: usize>(args: [&str; N]) -> CoreResult<()> {
    let status = Command::new("tmux")
        .arg("-L")
        .arg(TMUX_SOCKET)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| CoreError::new("CAPABILITY_UNAVAILABLE", "tmux is unavailable"))?;
    if status.success() {
        Ok(())
    } else {
        Err(CoreError::new("NOT_FOUND", "managed tmux session was not found"))
    }
}

fn validate_window_id(value: &str) -> CoreResult<()> {
    if value.strip_prefix('@').is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        Ok(())
    } else {
        Err(CoreError::new("INTERNAL_ERROR", "tmux returned an invalid window identity"))
    }
}

fn validate_terminal_name(value: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'-' | b'_'))
    {
        return Err(CoreError::new("INVALID_REQUEST", "Terminal name is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_tmux_target_and_name_injection() {
        for value in ["@1; kill-server", "1", "@-1", "@1.2"] {
            assert!(validate_window_id(value).is_err());
        }
        for value in ["bad;name", "bad\nname", "$(id)", "bad:name"] {
            assert!(validate_terminal_name(value).is_err());
        }
    }
}
