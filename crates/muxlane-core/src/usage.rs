use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{Condvar, Mutex, OnceLock, mpsc},
    thread,
    time::Duration,
};

use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    CoreError, CoreResult,
    credential::{checkout_query_home, cleanup_query_home},
    layout::{hex_sha256, validate_id},
    lock::ManagedLock,
    model::{CapabilityProbe, UsageRefreshResult, UsageSnapshot, UsageWindow},
    storage::{Storage, now},
};

const QUERY_TIMEOUT: Duration = Duration::from_secs(10);
const CACHE_SECONDS: i64 = 300;
pub const MAX_CONCURRENT_USAGE_QUERIES: usize = 4;

static QUERY_LIMIT: OnceLock<(Mutex<usize>, Condvar)> = OnceLock::new();

struct QueryPermit;

impl QueryPermit {
    fn acquire() -> CoreResult<Self> {
        let (mutex, ready) = QUERY_LIMIT.get_or_init(|| (Mutex::new(0), Condvar::new()));
        let active = mutex
            .lock()
            .map_err(|_| CoreError::new("INTERNAL_ERROR", "Usage limiter is unavailable"))?;
        let mut active =
            ready
                .wait_while(active, |count| *count >= MAX_CONCURRENT_USAGE_QUERIES)
                .map_err(|_| CoreError::new("INTERNAL_ERROR", "Usage limiter is unavailable"))?;
        *active += 1;
        Ok(Self)
    }
}

impl Drop for QueryPermit {
    fn drop(&mut self) {
        if let Some((mutex, ready)) = QUERY_LIMIT.get()
            && let Ok(mut active) = mutex.lock()
        {
            *active = active.saturating_sub(1);
            ready.notify_one();
        }
    }
}

pub fn probe_capabilities(storage: &Storage, account_id: &str) -> CoreResult<CapabilityProbe> {
    validate_id(account_id)?;
    storage.account(account_id)?;
    let query_home = storage.layout().query_home(account_id)?;
    let schema_directory = query_home.join(format!("schema-probe-{}", Uuid::new_v4().simple()));
    fs::create_dir(&schema_directory)?;
    let output = Command::new("codex")
        .args([
            "app-server",
            "generate-json-schema",
            "--experimental",
            "--out",
            schema_directory.to_str().ok_or_else(|| {
                CoreError::new("PATH_REJECTED", "Query Home encoding is unsupported")
            })?,
        ])
        .env("CODEX_HOME", &query_home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| {
            CoreError::new("CODEX_UNAVAILABLE", "Codex capability probe could not start")
        })?;
    if !output.success() {
        let _ = fs::remove_dir_all(&schema_directory);
        return Err(CoreError::new("CAPABILITY_UNAVAILABLE", "Codex schema generation failed"));
    }
    let client_request = fs::read(schema_directory.join("ClientRequest.json"))?;
    let fingerprint = hex_sha256(&client_request);
    let schema_text = String::from_utf8_lossy(&client_request);
    let token_usage_read = schema_text.contains("account/tokenUsage/read");
    let reset_credits = directory_contains(&schema_directory, "rateLimitResetCredits")?;
    let probe = CapabilityProbe {
        codex_version: codex_version()?,
        schema_fingerprint: fingerprint,
        account_read: schema_text.contains("account/read"),
        rate_limits_read: schema_text.contains("account/rateLimits/read"),
        token_usage_read,
        reset_credits,
    };
    fs::remove_dir_all(&schema_directory)?;
    Ok(probe)
}

pub fn refresh_usage(storage: &Storage, account_id: &str) -> CoreResult<UsageSnapshot> {
    let _permit = QueryPermit::acquire()?;
    let probe = probe_capabilities(storage, account_id)?;
    if !probe.account_read || !probe.rate_limits_read {
        return Err(CoreError::new(
            "CAPABILITY_UNAVAILABLE",
            "installed Codex schema lacks required account capabilities",
        ));
    }
    let _lock =
        ManagedLock::try_acquire(&storage.layout().account_lock(account_id)?, "ACCOUNT_IN_USE")?;
    checkout_query_home(storage.layout(), account_id)?;
    let result = query_app_server(storage, account_id, &probe);
    let cleanup = cleanup_query_home(storage.layout(), account_id);
    match (result, cleanup) {
        (Ok(snapshot), Ok(())) => {
            storage.cache_usage(&snapshot)?;
            storage.update_account_metadata(
                account_id,
                None,
                snapshot.plan_type.as_deref(),
                &snapshot.login_status,
                None,
            )?;
            Ok(snapshot)
        }
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

pub fn refresh_batch(
    storage: &Storage,
    account_ids: &[String],
) -> CoreResult<Vec<UsageRefreshResult>> {
    if account_ids.is_empty() || account_ids.len() > 128 {
        return Err(CoreError::new("INVALID_REQUEST", "Usage batch must contain 1..=128 accounts"));
    }
    let mut unique = std::collections::BTreeSet::new();
    for account_id in account_ids {
        validate_id(account_id)?;
        storage.account(account_id)?;
        if !unique.insert(account_id.clone()) {
            return Err(CoreError::new(
                "INVALID_REQUEST",
                "Usage batch contains a duplicate account",
            ));
        }
    }
    let (sender, receiver) = mpsc::channel();
    for account_id in unique {
        let storage = storage.clone();
        let sender = sender.clone();
        thread::spawn(move || {
            let result = match refresh_usage(&storage, &account_id) {
                Ok(snapshot) => {
                    UsageRefreshResult { account_id, snapshot: Some(snapshot), error_code: None }
                }
                Err(error) => UsageRefreshResult {
                    account_id,
                    snapshot: None,
                    error_code: Some(error.code.to_owned()),
                },
            };
            let _ = sender.send(result);
        });
    }
    drop(sender);
    let mut results: Vec<_> = receiver.into_iter().collect();
    results.sort_by(|left, right| left.account_id.cmp(&right.account_id));
    Ok(results)
}

fn query_app_server(
    storage: &Storage,
    account_id: &str,
    probe: &CapabilityProbe,
) -> CoreResult<UsageSnapshot> {
    query_app_server_with_executable(storage, account_id, probe, Path::new("codex"))
}

fn query_app_server_with_executable(
    storage: &Storage,
    account_id: &str,
    probe: &CapabilityProbe,
    executable: &Path,
) -> CoreResult<UsageSnapshot> {
    let query_home = storage.layout().query_home(account_id)?;
    let mut child = Command::new(executable)
        .args(["app-server", "--stdio"])
        .env("CODEX_HOME", &query_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| CoreError::new("CODEX_UNAVAILABLE", "Codex App Server could not start"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| CoreError::new("CODEX_UNAVAILABLE", "App Server input is unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CoreError::new("CODEX_UNAVAILABLE", "App Server output is unavailable"))?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let query_result = (|| -> CoreResult<UsageSnapshot> {
        send(
            &mut stdin,
            json!({"method":"initialize","id":0,"params":{"clientInfo":{"name":"muxlane","title":"Muxlane","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}}),
        )?;
        let _ = response_for(&receiver, 0)?;
        send(&mut stdin, json!({"method":"initialized","params":{}}))?;
        send(&mut stdin, json!({"method":"account/read","id":1,"params":{"refreshToken":false}}))?;
        send(&mut stdin, json!({"method":"account/rateLimits/read","id":2,"params":null}))?;
        if probe.token_usage_read {
            send(&mut stdin, json!({"method":"account/tokenUsage/read","id":3,"params":null}))?;
        }
        let account = response_for(&receiver, 1)?;
        let rate_limits = response_for(&receiver, 2)?;
        let token_usage =
            if probe.token_usage_read { Some(response_for(&receiver, 3)?) } else { None };
        normalize_usage(account_id, probe, &account, &rate_limits, token_usage.as_ref())
    })();
    let _ = child.kill();
    let _ = child.wait();
    query_result
}

fn send(stdin: &mut impl Write, value: Value) -> CoreResult<()> {
    serde_json::to_writer(&mut *stdin, &value)?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    Ok(())
}

fn response_for(receiver: &mpsc::Receiver<String>, id: u64) -> CoreResult<Value> {
    loop {
        let line = receiver
            .recv_timeout(QUERY_TIMEOUT)
            .map_err(|_| CoreError::new("CODEX_UNAVAILABLE", "App Server response timed out"))?;
        let value: Value = serde_json::from_str(&line)
            .map_err(|_| CoreError::new("CODEX_UNAVAILABLE", "App Server returned invalid JSON"))?;
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            if value.get("error").is_some() {
                return Err(CoreError::new("CODEX_UNAVAILABLE", "App Server query failed"));
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

pub fn normalize_usage(
    account_id: &str,
    probe: &CapabilityProbe,
    account: &Value,
    rate_limits: &Value,
    token_usage: Option<&Value>,
) -> CoreResult<UsageSnapshot> {
    let account_value = account.get("account");
    let account_type = account_value
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let plan_type = account_value
        .and_then(|value| value.get("planType"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let login_status = if account_value.is_some_and(|value| !value.is_null()) {
        "authenticated"
    } else {
        "login_required"
    };
    let mut windows = Vec::new();
    collect_windows(rate_limits, &mut windows);
    windows.sort_by_key(|window| window.duration_minutes);
    windows.dedup_by(|left, right| {
        left.duration_minutes == right.duration_minutes && left.resets_at == right.resets_at
    });
    let lifetime_tokens = token_usage
        .and_then(|value| value.pointer("/summary/lifetimeTokens"))
        .and_then(Value::as_u64);
    let reset_credit_available = find_key(rate_limits, "availableCount").and_then(Value::as_u64);
    let captured_at = now();
    Ok(UsageSnapshot {
        usage_snapshot_id: format!("usage_{}", Uuid::new_v4().simple()),
        account_id: account_id.to_owned(),
        codex_version: probe.codex_version.clone(),
        capability_fingerprint: probe.schema_fingerprint.clone(),
        account_type,
        plan_type,
        login_status: login_status.to_owned(),
        windows,
        lifetime_tokens,
        reset_credit_available,
        captured_at,
        expires_at: captured_at + CACHE_SECONDS,
        error_code: None,
    })
}

fn collect_windows(value: &Value, output: &mut Vec<UsageWindow>) {
    match value {
        Value::Object(map) => {
            if map.contains_key("usedPercent") && map.contains_key("windowDurationMins") {
                output.push(UsageWindow {
                    duration_minutes: map.get("windowDurationMins").and_then(Value::as_u64),
                    used_percent: map.get("usedPercent").and_then(Value::as_u64),
                    resets_at: map.get("resetsAt").and_then(Value::as_i64),
                });
            }
            for child in map.values() {
                collect_windows(child, output);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_windows(child, output);
            }
        }
        _ => {}
    }
}

fn find_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            map.get(key).or_else(|| map.values().find_map(|value| find_key(value, key)))
        }
        Value::Array(values) => values.iter().find_map(|value| find_key(value, key)),
        _ => None,
    }
}

fn codex_version() -> CoreResult<String> {
    let output = Command::new("codex")
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| CoreError::new("CODEX_UNAVAILABLE", "Codex CLI is unavailable"))?;
    if !output.status.success() {
        return Err(CoreError::new("CODEX_UNAVAILABLE", "Codex version probe failed"));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| CoreError::new("CODEX_UNAVAILABLE", "Codex version is invalid"))?;
    Ok(text.trim().to_owned())
}

fn directory_contains(root: &std::path::Path, needle: &str) -> CoreResult<bool> {
    let mut stack = vec![root.to_owned()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.file_type()?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file()
                && fs::read_to_string(entry.path()).unwrap_or_default().contains(needle)
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    use tempfile::tempdir;

    #[test]
    fn normalizes_semantic_usage_windows_without_persisting_raw_response() {
        let probe = CapabilityProbe {
            codex_version: "codex-cli fixture".to_owned(),
            schema_fingerprint: "fingerprint".to_owned(),
            account_read: true,
            rate_limits_read: true,
            token_usage_read: true,
            reset_credits: true,
        };
        let account =
            json!({"account":{"type":"chatgpt","email":"private@example.test","planType":"plus"}});
        let rates = json!({"rateLimits":{"secondary":{"windowDurationMins":10080,"usedPercent":7,"resetsAt":42},"primary":{"windowDurationMins":300,"usedPercent":12,"resetsAt":11}},"rateLimitResetCredits":{"availableCount":2}});
        let tokens = json!({"summary":{"lifetimeTokens":1234}});
        let snapshot =
            normalize_usage("account_fixture", &probe, &account, &rates, Some(&tokens)).unwrap();
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].duration_minutes, Some(300));
        assert_eq!(snapshot.windows[1].duration_minutes, Some(10080));
        assert_eq!(snapshot.lifetime_tokens, Some(1234));
        assert_eq!(snapshot.reset_credit_available, Some(2));
        assert!(!serde_json::to_string(&snapshot).unwrap().contains("private@example.test"));
    }

    #[test]
    fn fake_app_server_covers_handshake_capability_mapping_and_query_home() {
        let temp = tempdir().unwrap();
        let layout = crate::layout::Layout::initialize(temp.path().join("runtime")).unwrap();
        layout.ensure_account("account_fixture").unwrap();
        let storage = Storage::open(layout).unwrap();
        let fake = temp.path().join("fake-codex");
        fs::write(
            &fake,
            r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"id":0'*) echo '{"id":0,"result":{}}' ;;
    *'"id":1'*) echo '{"id":1,"result":{"account":{"type":"chatgpt","planType":"fixture"}}}' ;;
    *'"id":2'*) echo '{"id":2,"result":{"primary":{"windowDurationMins":300,"usedPercent":8,"resetsAt":10},"secondary":{"windowDurationMins":10080,"usedPercent":9,"resetsAt":20},"rateLimitResetCredits":{"availableCount":3}}}' ;;
    *'"id":3'*) echo '{"id":3,"result":{"summary":{"lifetimeTokens":44}}}' ;;
  esac
done
"#,
        ).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let probe = CapabilityProbe {
            codex_version: "fake-codex 1".to_owned(),
            schema_fingerprint: "fixture".to_owned(),
            account_read: true,
            rate_limits_read: true,
            token_usage_read: true,
            reset_credits: true,
        };
        let snapshot =
            query_app_server_with_executable(&storage, "account_fixture", &probe, &fake).unwrap();
        assert_eq!(
            snapshot.windows.iter().map(|window| window.duration_minutes).collect::<Vec<_>>(),
            vec![Some(300), Some(10080)]
        );
        assert_eq!(snapshot.reset_credit_available, Some(3));
        assert_eq!(snapshot.lifetime_tokens, Some(44));
        assert_eq!(snapshot.login_status, "authenticated");
    }
}
