#!/usr/bin/env bash
set -euo pipefail

command_name=${1:-}
test_root=${2:-}
bin_dir=/opt/muxlane/bin

case "$test_root" in
  /var/tmp/muxlane-e2e/terminate-*) ;;
  *) printf '%s\n' '{"status":"error","error_code":"UNSAFE_TEST_ROOT"}'; exit 64 ;;
esac

export PATH="$bin_dir:/usr/bin:/bin"
export MUXLANE_DATA_DIR="$test_root"

wait_for_socket() {
  for _ in $(seq 1 200); do
    if [[ -S "$test_root/run/muxlaned.sock" ]] && muxlane status >/dev/null 2>&1; then
      return
    fi
    sleep 0.05
  done
  printf '%s\n' '{"status":"error","error_code":"DAEMON_START_TIMEOUT"}'
  exit 70
}

case "$command_name" in
  prepare)
    install -d -m 0700 "$test_root"
    project_source="/var/tmp/muxlane-project-${test_root##*-}"
    install -d -m 0700 "$project_source"
    export MUXLANE_TEST_CODEX_MODE=wait
    nohup muxlaned serve >"$test_root/daemon-before-terminate.log" 2>&1 &
    daemon_pid=$!
    wait_for_socket
    account_id=$(muxlane account import /var/tmp/muxlane-e2e/fixture-auth.json TerminateFixture | jq -r .result.data.account_id)
    project_id=$(muxlane project register "$project_source" TerminateProject | jq -r .result.data.project_id)
    muxlane launch start "$account_id" "$project_id" >/dev/null
    for _ in $(seq 1 200); do
      state=$(sqlite3 "$test_root/muxlane.db" 'select state from launch_transactions order by created_at desc limit 1')
      [[ "$state" == running ]] && break
      sleep 0.05
    done
    [[ "$state" == running ]]
    vault_hash=$(sha256sum "$test_root/accounts/$account_id/auth.json" | cut -d' ' -f1)
    printf '%s\n' "$account_id" >"$test_root/account.id"
    printf '%s\n' "$project_id" >"$test_root/project.id"
    printf '%s\n' "$vault_hash" >"$test_root/vault-before.sha256"
    printf '{"scenario":"before_terminate","boot_id":"%s","state":"%s","daemon_pid":%s}\n' "$(cat /proc/sys/kernel/random/boot_id)" "$state" "$daemon_pid"
    ;;
  start-paused-recovery)
    export MUXLANE_TEST_MODE=1
    export MUXLANE_RECOVERY_TEST_PAUSE_AFTER_JOURNAL_MS=30000
    nohup muxlaned serve >"$test_root/daemon-paused-recovery.log" 2>&1 &
    daemon_pid=$!
    printf '%s\n' "$daemon_pid" >"$test_root/recovery-daemon.pid"
    for _ in $(seq 1 300); do
      running=$(sqlite3 "$test_root/muxlane.db" "select count(*) from recovery_runs where status='running'")
      [[ "$running" -gt 0 ]] && break
      sleep 0.05
    done
    [[ "$running" -gt 0 ]]
    printf '{"scenario":"recovery_journaled","boot_id":"%s","running_recovery_runs":%s,"daemon_pid":%s}\n' "$(cat /proc/sys/kernel/random/boot_id)" "$running" "$daemon_pid"
    ;;
  kill-paused-recovery)
    daemon_pid=$(cat "$test_root/recovery-daemon.pid")
    kill -9 "$daemon_pid"
    for _ in $(seq 1 100); do
      kill -0 "$daemon_pid" 2>/dev/null || break
      sleep 0.02
    done
    printf '{"scenario":"recovery_process_killed","daemon_pid":%s}\n' "$daemon_pid"
    ;;
  finish)
    export MUXLANE_TEST_CODEX_MODE=exit
    nohup muxlaned serve >"$test_root/daemon-final.log" 2>&1 &
    daemon_pid=$!
    wait_for_socket
    account_id=$(cat "$test_root/account.id")
    project_id=$(cat "$test_root/project.id")
    state=$(sqlite3 "$test_root/muxlane.db" 'select state from launch_transactions order by created_at asc limit 1')
    repeated=$(muxlane recover | jq '.result.data | length')
    incomplete=$(sqlite3 "$test_root/muxlane.db" "select count(*) from recovery_runs where status='running'")
    completed=$(sqlite3 "$test_root/muxlane.db" "select count(*) from recovery_runs where status='completed'")
    runtime_auth=absent
    [[ -e "$test_root/projects/$project_id/codex-home/auth.json" ]] && runtime_auth=present
    vault_hash=$(sha256sum "$test_root/accounts/$account_id/auth.json" | cut -d' ' -f1)
    vault_stable=no
    [[ "$vault_hash" == "$(cat "$test_root/vault-before.sha256")" ]] && vault_stable=yes
    open_incidents=$(sqlite3 "$test_root/muxlane.db" "select count(*) from recovery_incidents where status='open'")
    first_transaction=$(sqlite3 "$test_root/muxlane.db" 'select transaction_id from launch_transactions order by created_at asc limit 1')
    muxlane launch start "$account_id" "$project_id" >/dev/null
    for _ in $(seq 1 200); do
      latest=$(sqlite3 "$test_root/muxlane.db" 'select state from launch_transactions order by created_at desc limit 1')
      [[ "$latest" == finished ]] && break
      sleep 0.05
    done
    [[ "$latest" == finished ]]
    socket_mode=$(stat -c '%a' "$test_root/run/muxlaned.sock")
    root_mode=$(stat -c '%a' "$test_root")
    printf '{"scenario":"post_restart_idempotent_recovery","boot_id":"%s","first_transaction":"%s","state":"%s","repeat_results":%s,"incomplete_runs":%s,"completed_runs":%s,"runtime_auth":"%s","vault_stable":"%s","open_incidents":%s,"new_launch_state":"%s","socket_mode":"%s","root_mode":"%s"}\n' "$(cat /proc/sys/kernel/random/boot_id)" "$first_transaction" "$state" "$repeated" "$incomplete" "$completed" "$runtime_auth" "$vault_stable" "$open_incidents" "$latest" "$socket_mode" "$root_mode"
    muxlane daemon stop >/dev/null
    wait "$daemon_pid"
    ;;
  *) printf '%s\n' '{"status":"error","error_code":"INVALID_STAGE"}'; exit 64 ;;
esac
