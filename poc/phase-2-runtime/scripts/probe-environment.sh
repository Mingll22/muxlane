#!/usr/bin/env bash
# Non-production, metadata-only environment and Codex CLI probe for Phase 2A.
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=lib/poc-safety.sh
source "$SCRIPT_DIR/lib/poc-safety.sh"

usage() {
  cat <<'EOF'
Usage: probe-environment.sh --poc-root <absolute-linux-path> [--dry-run]

Write a local, mode-0600 evidence report below the explicit Phase 2A POC root.
The probe only runs version/help commands, an isolated CODEX_HOME version/help
check, and an explicitly exposed disposable Schema export. It never starts
interactive Codex or logs in/out, and it never intentionally targets ~/.codex
or prints credential file contents. Without strace or equivalent evidence, an
isolated probe does not prove that the CLI avoided the global Home.
EOF
}

raw_root=''
dry_run=false

while (($# > 0)); do
  case "$1" in
    --poc-root)
      (($# >= 2)) || die '--poc-root requires a value'
      raw_root="$2"
      shift 2
      ;;
    --dry-run)
      dry_run=true
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *) die "unknown argument: $1" ;;
  esac
done

POC_ROOT="$(validate_poc_root "$raw_root")" || exit $?
verify_poc_layout "$POC_ROOT"

if [[ "$dry_run" == true ]]; then
  printf 'DRY RUN: would write non-sensitive local evidence below <POC_ROOT>/evidence/.\n'
  exit 0
fi

umask 077
EVIDENCE_FILE="$POC_ROOT/evidence/environment-probe.txt"
HELP_CAPTURE=''
APP_SERVER_HELP_CAPTURE=''
DISPOSABLE_CODEX_HOME=''
SCHEMA_OUTPUT_DIRECTORY=''
app_server_schema_export_available=false
codex_probe_failed=false
LAST_COMMAND_EXIT_CODE=0

cleanup() {
  if [[ -n "$DISPOSABLE_CODEX_HOME" && -d "$DISPOSABLE_CODEX_HOME" ]]; then
    rm --recursive --force -- "$DISPOSABLE_CODEX_HOME"
  fi
  if [[ -n "$HELP_CAPTURE" && -f "$HELP_CAPTURE" ]]; then
    rm --force -- "$HELP_CAPTURE"
  fi
  if [[ -n "$APP_SERVER_HELP_CAPTURE" && -f "$APP_SERVER_HELP_CAPTURE" ]]; then
    rm --force -- "$APP_SERVER_HELP_CAPTURE"
  fi
  if [[ -n "$SCHEMA_OUTPUT_DIRECTORY" && -d "$SCHEMA_OUTPUT_DIRECTORY" ]]; then
    rm --recursive --force -- "$SCHEMA_OUTPUT_DIRECTORY"
  fi
}
trap cleanup EXIT

record_command() {
  local exit_code

  {
    printf '\n$'
    printf ' %q' "$@"
    printf '\n'
  } >>"$EVIDENCE_FILE"

  if "$@" >>"$EVIDENCE_FILE" 2>&1; then
    exit_code=0
  else
    exit_code=$?
  fi
  LAST_COMMAND_EXIT_CODE="$exit_code"
  printf '[exit_code=%s]\n' "$exit_code" >>"$EVIDENCE_FILE"
  return 0
}

record_text() {
  printf '%s\n' "$1" >>"$EVIDENCE_FILE"
}

record_isolated_codex_command() {
  record_command env "CODEX_HOME=$DISPOSABLE_CODEX_HOME" codex "$@"
}

record_tree_metadata() {
  local directory="$1"

  find -P "$directory" -mindepth 1 -printf '%P|type=%y|mode=%m|owner=%u|size=%s|mtime=%TY-%Tm-%TdT%TH:%TM:%TS%Tz\n' | sort
}

: >"$EVIDENCE_FILE"
chmod 0600 -- "$EVIDENCE_FILE"

{
  printf 'Muxlane Phase 2A local environment probe\n'
  printf 'Generated at: '
  date --iso-8601=seconds
  printf 'Repository path: %s\n' "$REPO_ROOT"
  printf 'POC root: %s\n' "$POC_ROOT"
  printf 'Shell: %s\n' "${SHELL:-<unset>}"
  printf 'HOME: %s\n' "$HOME"
  printf 'XDG_CONFIG_HOME: %s\n' "${XDG_CONFIG_HOME:-<unset>}"
  printf 'XDG_DATA_HOME: %s\n' "${XDG_DATA_HOME:-<unset>}"
  printf 'XDG_STATE_HOME: %s\n' "${XDG_STATE_HOME:-<unset>}"
  printf 'XDG_CACHE_HOME: %s\n' "${XDG_CACHE_HOME:-<unset>}"
  printf 'Default umask: '
  umask
  printf '\n[Operating system and filesystem]\n'
} >>"$EVIDENCE_FILE"

record_command uname --kernel-name --kernel-release --machine
record_command sh -c 'grep -qiE "microsoft|wsl" /proc/sys/kernel/osrelease /proc/version 2>/dev/null'
record_command lsb_release --description
record_command findmnt --noheadings --output TARGET,SOURCE,FSTYPE --target "$REPO_ROOT"
record_command findmnt --noheadings --output TARGET,SOURCE,FSTYPE --target "$POC_ROOT"

if command -v wsl.exe >/dev/null 2>&1; then
  record_command wsl.exe --version
else
  record_text 'wsl.exe --version: not available from this environment'
fi

record_text ''
record_text '[Toolchain]'
record_command rustc --version
record_command cargo --version
record_command node --version
record_command pnpm --version

record_text ''
record_text '[Codex CLI]'
if command -v codex >/dev/null 2>&1; then
  CODEX_BINARY="$(command -v codex)"
  record_text "command -v codex: $CODEX_BINARY"
  record_command realpath --canonicalize-existing -- "$CODEX_BINARY"
  DISPOSABLE_CODEX_HOME="$(mktemp --directory "$POC_ROOT/tmp/codex-home-probe.XXXXXX")"
  chmod 0700 -- "$DISPOSABLE_CODEX_HOME"
  record_text ''
  record_text '[Disposable CODEX_HOME version/help probe]'
  record_text 'Before:'
  record_tree_metadata "$DISPOSABLE_CODEX_HOME" >>"$EVIDENCE_FILE"
  record_isolated_codex_command --version
  [[ "$LAST_COMMAND_EXIT_CODE" == 0 ]] || codex_probe_failed=true

  HELP_CAPTURE="$(mktemp "$POC_ROOT/evidence/.codex-help.XXXXXX")"
  if env "CODEX_HOME=$DISPOSABLE_CODEX_HOME" codex --help >"$HELP_CAPTURE" 2>&1; then
    help_exit_code=0
  else
    help_exit_code=$?
  fi
  {
    printf '\n$ codex --help\n'
    sed -n '1,400p' "$HELP_CAPTURE"
    printf '[exit_code=%s]\n' "$help_exit_code"
  } >>"$EVIDENCE_FILE"
  [[ "$help_exit_code" == 0 ]] || codex_probe_failed=true

  for subcommand in resume login debug; do
    if awk -v command_name="$subcommand" '$1 == command_name { found = 1 } END { exit !found }' "$HELP_CAPTURE"; then
      record_isolated_codex_command "$subcommand" --help
    else
      record_text "codex $subcommand --help: not exposed by observed top-level help"
    fi
  done

  if awk '$1 == "app-server" { found = 1 } END { exit !found }' "$HELP_CAPTURE"; then
    APP_SERVER_HELP_CAPTURE="$(mktemp "$POC_ROOT/evidence/.codex-app-server-help.XXXXXX")"
    if env "CODEX_HOME=$DISPOSABLE_CODEX_HOME" codex app-server --help >"$APP_SERVER_HELP_CAPTURE" 2>&1; then
      app_server_help_exit_code=0
    else
      app_server_help_exit_code=$?
    fi
    {
      printf '\n$ codex app-server --help\n'
      sed -n '1,400p' "$APP_SERVER_HELP_CAPTURE"
      printf '[exit_code=%s]\n' "$app_server_help_exit_code"
    } >>"$EVIDENCE_FILE"

    if awk '$1 == "generate-json-schema" { found = 1 } END { exit !found }' "$APP_SERVER_HELP_CAPTURE"; then
      record_isolated_codex_command app-server generate-json-schema --help
      app_server_schema_export_available=true
    else
      record_text 'codex app-server generate-json-schema --help: not exposed by observed app-server help'
    fi
  else
    record_text 'codex app-server --help: not exposed by observed top-level help'
  fi

  if command -v npm >/dev/null 2>&1; then
    npm_global_root="$(npm root --global 2>/dev/null || true)"
    if [[ -n "$npm_global_root" && -d "$npm_global_root/@openai/codex" ]]; then
      record_text "Codex installation source (inferred): npm global package @openai/codex at $npm_global_root"
    else
      record_text 'Codex installation source: not safely determined from npm global package metadata'
    fi
  else
    record_text 'Codex installation source: not safely determined because npm is unavailable'
  fi

  record_text 'After:'
  record_tree_metadata "$DISPOSABLE_CODEX_HOME" >>"$EVIDENCE_FILE"

  if [[ "$app_server_schema_export_available" == true ]]; then
    SCHEMA_OUTPUT_DIRECTORY="$(mktemp --directory "$POC_ROOT/tmp/app-server-schema.XXXXXX")"
    chmod 0700 -- "$SCHEMA_OUTPUT_DIRECTORY"
    record_text ''
    record_text '[Disposable App Server schema export probe]'
    record_isolated_codex_command app-server generate-json-schema --out "$SCHEMA_OUTPUT_DIRECTORY"
    record_text 'Generated schema metadata:'
    record_tree_metadata "$SCHEMA_OUTPUT_DIRECTORY" >>"$EVIDENCE_FILE"
    [[ "$LAST_COMMAND_EXIT_CODE" == 0 ]] || codex_probe_failed=true
  fi

  if command -v strace >/dev/null 2>&1; then
    TRACE_FILE="$POC_ROOT/evidence/codex-home-version.strace"
    if strace --follow-forks --quiet --trace=file --output="$TRACE_FILE" env "CODEX_HOME=$DISPOSABLE_CODEX_HOME" codex --version >/dev/null 2>&1; then
      chmod 0600 -- "$TRACE_FILE"
      if grep --quiet '/\.codex\(/\|"\|$\)' "$TRACE_FILE"; then
        record_text 'Global CODEX_HOME access during traced codex --version: observed; inspect protected trace evidence.'
      else
        record_text 'Global CODEX_HOME access during traced codex --version: no .codex file path observed.'
      fi
    else
      record_text 'Global CODEX_HOME access during traced codex --version: not verified because strace failed.'
    fi
  else
    record_text 'Global CODEX_HOME access during codex --version: not verified because strace is unavailable.'
  fi
else
  record_text 'codex executable: unavailable; Codex CLI capability probe is blocked.'
  codex_probe_failed=true
fi

if ! scan_for_sensitive_markers "$EVIDENCE_FILE"; then
  printf 'error: sensitive marker detected in local evidence; protected evidence was retained for local inspection.\n' >&2
  exit 1
fi

if [[ -f "${TRACE_FILE:-}" ]] && ! scan_for_sensitive_markers "$TRACE_FILE"; then
  printf 'error: sensitive marker detected in protected trace evidence; it was retained for local inspection.\n' >&2
  exit 1
fi

if [[ "$codex_probe_failed" == true ]]; then
  printf 'error: a required Codex CLI probe failed; inspect protected local evidence.\n' >&2
  exit 1
fi

printf 'PASS: wrote local, mode-0600 probe evidence below <POC_ROOT>/evidence/.\n'
