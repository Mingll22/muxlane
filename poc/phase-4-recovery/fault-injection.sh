#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
daemon_bin="$repo_root/target/debug/muxlaned"
cli_bin="$repo_root/target/debug/muxlane"
fixture_bin="$repo_root/crates/muxlaned/tests/fixtures/bin"
fixture_auth="$repo_root/crates/muxlaned/tests/fixtures/auth.json"
base_root=${1:-$(mktemp -d /tmp/muxlane-phase45.XXXXXX)}

case "$base_root" in
  /tmp/muxlane-phase45.*) ;;
  *)
    printf '%s\n' '{"status":"error","error_code":"UNSAFE_TEST_ROOT"}'
    exit 64
    ;;
esac

test -x "$daemon_bin"
test -x "$cli_bin"
test -x "$fixture_bin/codex"
test -f "$fixture_auth"

daemon_pid=
test_sessions=()
cleanup() {
  if [[ -n "$daemon_pid" ]] && kill -0 "$daemon_pid" 2>/dev/null; then
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
  fi
  for session in "${test_sessions[@]}"; do
    tmux -L muxlane-runtime kill-session -t "$session" 2>/dev/null || true
  done
}
trap cleanup EXIT

select_root() {
  local name=$1
  export MUXLANE_DATA_DIR="$base_root/$name"
  mkdir -m 700 "$MUXLANE_DATA_DIR"
  export PATH="$fixture_bin:${ORIGINAL_PATH:-$PATH}"
}
ORIGINAL_PATH=$PATH

start_daemon() {
  "$daemon_bin" serve >/dev/null 2>&1 &
  daemon_pid=$!
  for _ in $(seq 1 100); do
    if [[ -S "$MUXLANE_DATA_DIR/run/muxlaned.sock" ]] && "$cli_bin" status >/dev/null 2>&1; then
      test "$(stat -c %a "$MUXLANE_DATA_DIR/run/muxlaned.sock")" = 600
      if MUXLANE_DATA_DIR="$MUXLANE_DATA_DIR" "$daemon_bin" serve >/dev/null 2>&1; then
        printf '%s\n' '{"status":"error","error_code":"DAEMON_SINGLE_INSTANCE_BROKEN"}'
        exit 74
      fi
      if ss -ltnp 2>/dev/null | grep -F "pid=$daemon_pid," >/dev/null; then
        printf '%s\n' '{"status":"error","error_code":"UNEXPECTED_TCP_LISTENER"}'
        exit 75
      fi
      return
    fi
    sleep 0.05
  done
  printf '%s\n' '{"status":"error","error_code":"DAEMON_START_TIMEOUT"}'
  exit 70
}

stop_daemon() {
  "$cli_bin" daemon stop >/dev/null
  wait "$daemon_pid"
  daemon_pid=
}

setup_resources() {
  local suffix=$1
  local account_json project_json
  account_json=$("$cli_bin" account import "$fixture_auth" "Fixture $suffix")
  ACCOUNT_ID=$(jq -r '.result.data.account_id' <<<"$account_json")
  project_json=$("$cli_bin" project register "$repo_root" "Muxlane $suffix")
  PROJECT_ID=$(jq -r '.result.data.project_id' <<<"$project_json")
}

wait_for_state() {
  local expected=$1
  for _ in $(seq 1 100); do
    local state
    state=$("$cli_bin" launch list | jq -r '.result.data[0].state')
    if [[ "$state" == "$expected" ]]; then
      return
    fi
    sleep 0.05
  done
  printf '{"status":"error","error_code":"STATE_TIMEOUT","expected":"%s"}\n' "$expected"
  exit 71
}

read_process_ids() {
  read -r RUNNER_PID CODEX_PID < <(
    python3 - "$MUXLANE_DATA_DIR/muxlane.db" <<'PY'
import sqlite3
import sys

connection = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
print(*connection.execute(
    "SELECT runner_pid, codex_pid FROM launch_transactions ORDER BY created_at DESC LIMIT 1"
).fetchone())
PY
  )
}

scenario_daemon_and_codex_kill() {
  select_root daemon-kill
  export MUXLANE_TEST_CODEX_MODE=wait
  start_daemon
  setup_resources daemon-kill
  "$cli_bin" launch start "$ACCOUNT_ID" "$PROJECT_ID" >/dev/null
  wait_for_state running
  read_process_ids
  kill -9 "$daemon_pid"
  wait "$daemon_pid" 2>/dev/null || true
  daemon_pid=
  test -r "/proc/$RUNNER_PID/stat"
  test -r "/proc/$CODEX_PID/stat"
  start_daemon
  local recovery
  recovery=$("$cli_bin" recover)
  test "$(jq -r '.result.data[0].classification' <<<"$recovery")" = active_flock
  kill -9 "$CODEX_PID"
  wait_for_state finished
  stop_daemon
  printf '%s\n' '{"scenario":"daemon_kill_then_codex_kill","status":"PASS"}'
}

scenario_runner_kill() {
  select_root runner-kill
  export MUXLANE_TEST_CODEX_MODE=wait
  start_daemon
  setup_resources runner-kill
  "$cli_bin" launch start "$ACCOUNT_ID" "$PROJECT_ID" >/dev/null
  wait_for_state running
  read_process_ids
  kill -9 "$RUNNER_PID"
  sleep 0.5
  if kill -0 "$CODEX_PID" 2>/dev/null; then
    kill -9 "$CODEX_PID"
    sleep 0.2
  fi
  local recovery
  recovery=$("$cli_bin" recover)
  test "$(jq -r '.result.data[0].state' <<<"$recovery")" = recovered
  test ! -e "$MUXLANE_DATA_DIR/projects/$PROJECT_ID/codex-home/auth.json"
  stop_daemon
  printf '%s\n' '{"scenario":"runner_kill","status":"PASS"}'
}

scenario_ctrl_c() {
  select_root ctrl-c
  export MUXLANE_TEST_CODEX_MODE=wait
  start_daemon
  setup_resources ctrl-c
  "$cli_bin" launch start "$ACCOUNT_ID" "$PROJECT_ID" >/dev/null
  wait_for_state running
  read_process_ids
  kill -INT "$CODEX_PID"
  wait_for_state finished
  test ! -e "$MUXLANE_DATA_DIR/projects/$PROJECT_ID/codex-home/auth.json"
  stop_daemon
  printf '%s\n' '{"scenario":"ctrl_c","status":"PASS"}'
}

scenario_lock_contention() {
  select_root lock-contention
  export MUXLANE_TEST_CODEX_MODE=wait
  start_daemon
  setup_resources primary
  local primary_account=$ACCOUNT_ID
  local primary_project=$PROJECT_ID
  local account_two project_two project_three
  account_two=$("$cli_bin" account import "$fixture_auth" 'Fixture secondary' | jq -r '.result.data.account_id')
  mkdir -m 700 "$base_root/source-two" "$base_root/source-three"
  project_two=$("$cli_bin" project register "$base_root/source-two" secondary | jq -r '.result.data.project_id')
  project_three=$("$cli_bin" project register "$base_root/source-three" tertiary | jq -r '.result.data.project_id')
  "$cli_bin" launch start "$primary_account" "$primary_project" >/dev/null
  test_sessions+=("muxlane-${primary_project:8:24}")
  wait_for_state running
  local error
  if error=$("$cli_bin" launch start "$primary_account" "$project_two"); then
    exit 72
  fi
  test "$(jq -r '.error_code' <<<"$error")" = ACCOUNT_IN_USE
  if error=$("$cli_bin" launch start "$account_two" "$primary_project"); then
    exit 73
  fi
  test "$(jq -r '.error_code' <<<"$error")" = PROJECT_IN_USE
  "$cli_bin" launch start "$account_two" "$project_three" >/dev/null
  test_sessions+=("muxlane-${project_three:8:24}")
  for _ in $(seq 1 100); do
    if [[ $(python3 - "$MUXLANE_DATA_DIR/muxlane.db" <<'PY'
import sqlite3
import sys

connection = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
print(connection.execute(
    "SELECT COUNT(*) FROM launch_transactions WHERE state='running'"
).fetchone()[0])
PY
) == 2 ]]; then
      break
    fi
    sleep 0.05
  done
  test "$(python3 - "$MUXLANE_DATA_DIR/muxlane.db" <<'PY'
import sqlite3
import sys

connection = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
print(connection.execute(
    "SELECT COUNT(*) FROM launch_transactions WHERE state='running'"
).fetchone()[0])
PY
)" = 2
  python3 - "$MUXLANE_DATA_DIR/muxlane.db" <<'PY' | while read -r pid; do kill -9 "$pid"; done
import sqlite3
import sys

connection = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True)
for row in connection.execute("SELECT codex_pid FROM launch_transactions WHERE state='running'"):
    print(row[0])
PY
  for _ in $(seq 1 100); do
    if [[ $("$cli_bin" status | jq -r '.result.data.active_launches') == 0 ]]; then
      break
    fi
    sleep 0.05
  done
  test "$("$cli_bin" status | jq -r '.result.data.active_launches')" = 0
  stop_daemon
  printf '%s\n' '{"scenario":"lock_contention_and_parallelism","status":"PASS"}'
}

scenario_usage_and_diagnostics() {
  select_root usage-diagnostics
  export PATH="$ORIGINAL_PATH"
  export MUXLANE_TEST_CODEX_MODE=exit
  start_daemon
  setup_resources usage
  local probe refresh_error receipt diagnostics_path
  probe=$("$cli_bin" usage probe "$ACCOUNT_ID")
  test "$(jq -r '.result.data.account_read' <<<"$probe")" = true
  test "$(jq -r '.result.data.rate_limits_read' <<<"$probe")" = true
  test "$(jq -r '.result.data.schema_fingerprint | length' <<<"$probe")" = 64
  if refresh_error=$("$cli_bin" usage refresh "$ACCOUNT_ID"); then
    exit 76
  fi
  test "$(jq -r '.error_code' <<<"$refresh_error")" = CODEX_UNAVAILABLE
  test ! -e "$MUXLANE_DATA_DIR/accounts/$ACCOUNT_ID/query-home/auth.json"
  receipt=$("$cli_bin" diagnostics export)
  diagnostics_path=$(jq -r '.result.data.relative_path' <<<"$receipt")
  test -f "$MUXLANE_DATA_DIR/$diagnostics_path"
  if grep -aER 'synthetic-only|Authorization|Bearer |private@example' \
    "$MUXLANE_DATA_DIR/muxlane.db" "$MUXLANE_DATA_DIR/logs" "$MUXLANE_DATA_DIR/diagnostics"; then
    exit 77
  fi
  stop_daemon
  printf '%s\n' '{"scenario":"usage_probe_failure_cleanup_and_diagnostics_redaction","status":"PASS"}'
}

scenario_daemon_and_codex_kill
scenario_runner_kill
scenario_ctrl_c
scenario_lock_contention
scenario_usage_and_diagnostics
printf '{"status":"PASS","evidence_root":"%s"}\n' "$base_root"
