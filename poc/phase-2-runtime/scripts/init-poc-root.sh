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

POC_ROOT="$(validate_poc_root "$raw_root")" || exit $?

if [[ -e "$POC_ROOT" ]]; then
  [[ -d "$POC_ROOT" && ! -L "$POC_ROOT" ]] || die 'the existing POC root is unsafe'
  [[ "$(stat --format='%u' -- "$POC_ROOT")" == "$(id --user)" ]] || die 'the existing POC root is not owned by the current user'
  [[ "$(stat --format='%a' -- "$POC_ROOT")" == '700' ]] || die 'the existing POC root must have mode 0700'
  [[ -z "$(find -P "$POC_ROOT" -mindepth 1 -print -quit)" ]] || die 'the existing POC root is not empty; refusing to modify it'
fi

if [[ "$dry_run" == true ]]; then
  printf 'DRY RUN: would initialize POC root: <POC_ROOT>\n'
  printf 'DRY RUN: would create directories with mode 0700:\n'
  printf '  <POC_ROOT>\n'
  for relative in "${POC_REQUIRED_DIRECTORIES[@]}"; do
    printf '  <POC_ROOT>/%s\n' "$relative"
  done
  exit 0
fi

umask 077

create_poc_root() {
  [[ ! -e "$POC_ROOT" && ! -L "$POC_ROOT" ]] || die 'the POC root appeared while initializing'
  mkdir --parents --mode=0700 -- "$POC_ROOT"
  [[ -d "$POC_ROOT" && ! -L "$POC_ROOT" ]] || die 'the POC root is unsafe after creation'
  [[ "$(stat --format='%u' -- "$POC_ROOT")" == "$(id --user)" ]] || die 'the created POC root is not owned by the current user'
  [[ "$(stat --format='%a' -- "$POC_ROOT")" == '700' ]] || die 'the created POC root does not have mode 0700'
}

create_required_directory() {
  local target="$1"
  local relative="$2"

  [[ ! -e "$target" && ! -L "$target" ]] || die 'a POC directory appeared while initializing the layout'
  assert_no_symlink_components "$target"
  printf 'Creating POC directory: <POC_ROOT>/%s\n' "$relative"
  mkdir --mode=0700 -- "$target"
  [[ -d "$target" && ! -L "$target" ]] || die 'a symbolic link appeared while creating the POC layout'
  [[ "$(stat --format='%u' -- "$target")" == "$(id --user)" ]] || die 'a created POC directory has an unexpected owner'
  [[ "$(stat --format='%a' -- "$target")" == '700' ]] || die 'a created POC directory does not have mode 0700'
}

create_poc_root
for relative in "${POC_REQUIRED_DIRECTORIES[@]}"; do
  create_required_directory "$POC_ROOT/$relative" "$relative"
done

verify_poc_layout "$POC_ROOT"
printf 'PASS: initialized a Phase 2A POC root; no credentials were created or copied.\n'
