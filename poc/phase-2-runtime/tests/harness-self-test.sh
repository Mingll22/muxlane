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
expect_success bash "$INIT" --poc-root "$test_root"
expect_success bash "$INIT" --poc-root "$test_root" --dry-run
expect_success bash "$VERIFY" --poc-root "$test_root"
expect_success bash "$VERIFY" --poc-root "$test_root" --dry-run

expect_failure bash "$INIT" --poc-root /
expect_failure bash "$INIT" --poc-root "$HOME"
expect_failure bash "$INIT" --poc-root "$HOME/.codex/muxlane-phase-2a"
expect_failure bash "$INIT" --poc-root "$REPO_ROOT"
expect_failure bash "$INIT" --poc-root "$REPO_ROOT/poc/phase-2-runtime/unsafe"
expect_failure bash "$INIT" --poc-root /mnt/c/muxlane-phase-2a

printf 'UNIQUE_SYNTHETIC_NON_CREDENTIAL_CONTENT\n' >"$test_root/synthetic fixture.txt"
metadata="$(bash "$INSPECT" --poc-root "$test_root" --file "$test_root/synthetic fixture.txt")"
[[ "$metadata" == *'sha256='* ]]
[[ "$metadata" != *'UNIQUE_SYNTHETIC_NON_CREDENTIAL_CONTENT'* ]]
expect_success bash "$INSPECT" --poc-root "$test_root" --file "$test_root/synthetic fixture.txt" --dry-run

ln --symbolic "$test_root/synthetic fixture.txt" "$test_root/projects/project-a/codex-home/auth.json"
expect_failure bash "$INSPECT" --poc-root "$test_root" --file "$test_root/projects/project-a/codex-home/auth.json"
expect_failure bash "$VERIFY" --poc-root "$test_root"
rm -- "$test_root/projects/project-a/codex-home/auth.json"

chmod 0755 -- "$test_root"
expect_failure bash "$VERIFY" --poc-root "$test_root"
chmod 0700 -- "$test_root"
expect_success bash "$VERIFY" --poc-root "$test_root"

expect_success bash "$PROBE" --poc-root "$test_root" --dry-run
expect_success bash "$PROBE" --poc-root "$test_root"
[[ -f "$test_root/evidence/environment-probe.txt" ]]
[[ "$(stat --format='%a' -- "$test_root/evidence/environment-probe.txt")" == '600' ]]

printf 'PASS: Phase 2A POC harness self-test completed.\n'
