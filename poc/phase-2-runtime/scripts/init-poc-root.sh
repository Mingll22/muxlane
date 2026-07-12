#!/usr/bin/env bash
# Non-production Phase 2A POC directory initializer. It never accesses ~/.codex.
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=lib/poc-safety.sh
source "$SCRIPT_DIR/lib/poc-safety.sh"

usage() {
  cat <<'EOF'
Usage: init-poc-root.sh --poc-root <absolute-linux-path> [--dry-run]

Create only the empty Phase 2A POC layout under the explicit root. The root must
be on an approved Linux-native filesystem and cannot be /, $HOME, ~/.codex, or
the repository (including its subdirectories).
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
  printf 'DRY RUN: would initialize POC root: %s\n' "$POC_ROOT"
  printf 'DRY RUN: would create directories with mode 0700:\n'
  printf '  %s\n' "$POC_ROOT"
  for relative in "${POC_REQUIRED_DIRECTORIES[@]}"; do
    printf '  %s/%s\n' "$POC_ROOT" "$relative"
  done
  exit 0
fi

umask 077

create_directory() {
  local target="$1"

  if [[ ! -d "$target" ]]; then
    printf 'Creating POC directory: %s\n' "$target"
  fi
  mkdir --parents --mode=0700 -- "$target"
  [[ ! -L "$target" ]] || die 'a symbolic link appeared while creating the POC layout'
  chmod 0700 -- "$target"
}

create_directory "$POC_ROOT"
for relative in "${POC_REQUIRED_DIRECTORIES[@]}"; do
  create_directory "$POC_ROOT/$relative"
done

verify_poc_layout "$POC_ROOT"
printf 'PASS: initialized a Phase 2A POC root; no credentials were created or copied.\n'
