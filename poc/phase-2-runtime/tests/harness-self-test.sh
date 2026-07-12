#!/usr/bin/env bash
# Non-production self-test for the Phase 2A Runtime POC harness.
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
HARNESS_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
REPO_ROOT="$(git -C "$HARNESS_DIR" rev-parse --show-toplevel)"
INIT="$HARNESS_DIR/scripts/init-poc-root.sh"
INSPECT="$HARNESS_DIR/scripts/inspect-file-metadata.sh"
PROBE="$HARNESS_DIR/scripts/probe-environment.sh"
VERIFY="$HARNESS_DIR/scripts/verify-poc-safety.sh"

test_root=''

cleanup() {
  [[ -n "$test_root" && -d "$test_root" ]] || return 0
  rm --recursive --force -- "$test_root"
}
trap cleanup EXIT

expect_success() {
  "$@" >/dev/null
}

expect_failure() {
  if "$@" >/dev/null 2>&1; then
    printf 'expected command to fail: %q\n' "$*" >&2
    exit 1
  fi
}

expect_success bash "$INIT" --help
expect_success bash "$INSPECT" --help
expect_success bash "$PROBE" --help
expect_success bash "$VERIFY" --help
expect_failure bash "$INIT"

test_root="$(mktemp --directory '/tmp/muxlane phase 2a test.XXXXXX')"
rmdir -- "$test_root"

expect_success bash "$INIT" --poc-root "$test_root"
expect_failure bash "$INIT" --poc-root "$test_root"
expect_failure bash "$INIT" --poc-root "$test_root" --dry-run
expect_success bash "$VERIFY" --poc-root "$test_root"
expect_success bash "$VERIFY" --poc-root "$test_root" --dry-run

expect_failure bash "$INIT" --poc-root /
expect_failure bash "$INIT" --poc-root "$HOME"
fake_home="$test_root/fake-home"
mkdir -- "$fake_home"
chmod 0700 -- "$fake_home"
expect_success env HOME="$fake_home" bash "$INIT" --poc-root "$fake_home/safe-child" --dry-run
expect_failure bash "$INIT" --poc-root "$HOME/.codex/muxlane-phase-2a"
expect_failure bash "$INIT" --poc-root "$REPO_ROOT"
expect_failure bash "$INIT" --poc-root "$REPO_ROOT/poc/phase-2-runtime/unsafe"
expect_failure bash "$INIT" --poc-root /mnt/c/muxlane-phase-2a
expect_failure bash "$INIT" --poc-root /mnt/d/muxlane-phase-2a
expect_failure bash "$INIT" --poc-root /mnt/wsl/muxlane-phase-2a
expect_failure bash "$INIT" --poc-root relative-path
expect_failure bash "$INIT" --poc-root ''
expect_failure bash "$INIT" --poc-root $'/tmp/muxlane-phase-2a\nnewline'
expect_success bash "$INIT" --poc-root "$test_root/space path" --dry-run
expect_success bash "$INIT" --poc-root "$test_root/-leading-dash" --dry-run

nonempty_root="$test_root/nonempty-root"
mkdir -- "$nonempty_root"
touch -- "$nonempty_root/user-file"
chmod 0700 -- "$nonempty_root"
expect_failure bash "$INIT" --poc-root "$nonempty_root"
[[ -f "$nonempty_root/user-file" ]]

parent_link="$test_root/parent-link"
ln --symbolic "$test_root" "$parent_link"
expect_failure bash "$INIT" --poc-root "$parent_link/new-root"
rm -- "$parent_link"

root_link="$test_root/root-link"
ln --symbolic "$test_root" "$root_link"
expect_failure bash "$INIT" --poc-root "$root_link"
rm -- "$root_link"

printf 'UNIQUE_SYNTHETIC_NON_CREDENTIAL_CONTENT\n' >"$test_root/synthetic fixture.txt"
chmod 0600 -- "$test_root/synthetic fixture.txt"
metadata="$(bash "$INSPECT" --poc-root "$test_root" --file "$test_root/synthetic fixture.txt")"
[[ "$metadata" == *'sha256='* ]]
[[ "$metadata" != *'UNIQUE_SYNTHETIC_NON_CREDENTIAL_CONTENT'* ]]
[[ "$metadata" == *'owner=current-user'* ]]
expect_success bash "$INSPECT" --poc-root "$test_root" --file "$test_root/synthetic fixture.txt" --dry-run
chmod 0644 -- "$test_root/synthetic fixture.txt"
expect_failure bash "$INSPECT" --poc-root "$test_root" --file "$test_root/synthetic fixture.txt"
chmod 0600 -- "$test_root/synthetic fixture.txt"

ln --symbolic "$test_root/synthetic fixture.txt" "$test_root/projects/project-a/codex-home/auth.json"
expect_failure bash "$INSPECT" --poc-root "$test_root" --file "$test_root/projects/project-a/codex-home/auth.json"
expect_failure bash "$VERIFY" --poc-root "$test_root"
rm -- "$test_root/projects/project-a/codex-home/auth.json"

touch -- "$test_root/tmp/auth.json"
chmod 0600 -- "$test_root/tmp/auth.json"
expect_failure bash "$INSPECT" --poc-root "$test_root" --file "$test_root/tmp/auth.json"
expect_failure bash "$VERIFY" --poc-root "$test_root"
rm -- "$test_root/tmp/auth.json"

ln --symbolic "$test_root/synthetic fixture.txt" "$test_root/tmp/synthetic-link"
expect_failure bash "$INSPECT" --poc-root "$test_root" --file "$test_root/tmp/synthetic-link"
expect_failure bash "$VERIFY" --poc-root "$test_root"
rm -- "$test_root/tmp/synthetic-link"

rmdir -- "$test_root/accounts/account-a"
ln --symbolic "$test_root" "$test_root/accounts/account-a"
expect_failure bash "$VERIFY" --poc-root "$test_root"
rm -- "$test_root/accounts/account-a"
mkdir --mode=0700 -- "$test_root/accounts/account-a"

rmdir -- "$test_root/projects/project-a/codex-home"
rmdir -- "$test_root/projects/project-a"
ln --symbolic "$test_root" "$test_root/projects/project-a"
expect_failure bash "$VERIFY" --poc-root "$test_root"
rm -- "$test_root/projects/project-a"
mkdir --mode=0700 -- "$test_root/projects/project-a"
mkdir --mode=0700 -- "$test_root/projects/project-a/codex-home"

chmod 0755 -- "$test_root"
expect_failure bash "$VERIFY" --poc-root "$test_root"
chmod 0700 -- "$test_root"
expect_success bash "$VERIFY" --poc-root "$test_root"

expect_success bash "$PROBE" --poc-root "$test_root" --dry-run
probe_output="$(bash "$PROBE" --poc-root "$test_root")"
[[ "$probe_output" != *"$HOME"* ]]
[[ "$probe_output" != *"$REPO_ROOT"* ]]
[[ "$probe_output" != *'UNIQUE_SYNTHETIC_NON_CREDENTIAL_CONTENT'* ]]
[[ -f "$test_root/evidence/environment-probe.txt" ]]
[[ "$(stat --format='%a' -- "$test_root/evidence/environment-probe.txt")" == '600' ]]

printf 'PASS: Phase 2A POC harness self-test completed.\n'
