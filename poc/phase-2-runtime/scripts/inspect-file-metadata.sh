#!/usr/bin/env bash
# Non-production metadata-only file inspection for the Phase 2A POC harness.
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=lib/poc-safety.sh
source "$SCRIPT_DIR/lib/poc-safety.sh"

usage() {
  cat <<'EOF'
Usage: inspect-file-metadata.sh --poc-root <absolute-linux-path> --file <absolute-path> [--dry-run]

Inspect a regular file within the explicit POC root. The command prints only a
placeholder path, type, permissions, owner, size, mtime, and SHA-256. It never
prints file contents and rejects every symbolic-link component.
EOF
}

raw_root=''
raw_file=''
dry_run=false

while (($# > 0)); do
  case "$1" in
    --poc-root)
      (($# >= 2)) || die '--poc-root requires a value'
      raw_root="$2"
      shift 2
      ;;
    --file)
      (($# >= 2)) || die '--file requires a value'
      raw_file="$2"
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
FILE_PATH="$(validate_file_within_poc_root "$raw_file" "$POC_ROOT")" || exit $?

if [[ "$dry_run" == true ]]; then
  printf 'DRY RUN: would inspect metadata for %s\n' "$(relative_poc_path "$FILE_PATH" "$POC_ROOT")"
  exit 0
fi

printf 'file=%s\n' "$(relative_poc_path "$FILE_PATH" "$POC_ROOT")"
printf 'type=%s\n' "$(stat --format='%F' -- "$FILE_PATH")"
printf 'mode=%s\n' "$(stat --format='%a' -- "$FILE_PATH")"
printf 'owner=current-user\n'
printf 'size_bytes=%s\n' "$(stat --format='%s' -- "$FILE_PATH")"
printf 'mtime=%s\n' "$(stat --format='%y' -- "$FILE_PATH")"
printf 'sha256=%s\n' "$(sha256sum -- "$FILE_PATH" | awk '{ print $1 }')"
