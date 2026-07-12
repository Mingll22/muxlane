#!/usr/bin/env bash
# Non-production structural safety checks for the Phase 2A POC harness.
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=lib/poc-safety.sh
source "$SCRIPT_DIR/lib/poc-safety.sh"

usage() {
  cat <<'EOF'
Usage: verify-poc-safety.sh --poc-root <absolute-linux-path> [--dry-run]

Verify the empty Phase 2A POC structure. It rejects unsafe locations, symlinks,
incorrect ownership or permissions, and any auth.json. It does not read file
contents or perform credential checkout, commit, locking, or recovery.
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

POC_ROOT="$(validate_poc_root "$raw_root")"

if [[ "$dry_run" == true ]]; then
  printf 'DRY RUN: would verify the Phase 2A POC structure at %s\n' "$(relative_poc_path "$POC_ROOT" "$POC_ROOT")"
  exit 0
fi

verify_poc_layout "$POC_ROOT"
printf 'PASS: POC root has expected 0700 directories, no symbolic links, and no auth.json.\n'
