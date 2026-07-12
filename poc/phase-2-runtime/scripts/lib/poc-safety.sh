#!/usr/bin/env bash
# Non-production helpers for the Phase 2A Runtime POC harness.

readonly POC_HARNESS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
readonly REPO_ROOT="$(git -C "$POC_HARNESS_DIR" rev-parse --show-toplevel)"

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command is unavailable: $1"
}

is_within_path() {
  local child="$1"
  local parent="$2"

  [[ "$child" == "$parent" || "$child" == "$parent"/* ]]
}

assert_no_parent_traversal() {
  local raw_path="$1"
  local component
  local -a components=()

  IFS='/' read -r -a components <<<"${raw_path#/}"
  for component in "${components[@]}"; do
    [[ "$component" != '..' ]] || die 'parent traversal is not allowed in a POC path'
  done
}

assert_safe_poc_path_characters() {
  local raw_path="$1"

  if [[ "$raw_path" =~ [[:cntrl:]] ]]; then
    die 'control characters are not allowed in a POC path'
  fi
}

assert_no_symlink_components() {
  local absolute_path="$1"
  local component
  local current='/'
  local -a components=()

  IFS='/' read -r -a components <<<"${absolute_path#/}"
  for component in "${components[@]}"; do
    [[ -n "$component" ]] || continue
    current="${current%/}/$component"
    [[ ! -L "$current" ]] || die 'symbolic links are not allowed in a POC path'
  done
}

nearest_existing_parent() {
  local path="$1"

  while [[ ! -e "$path" ]]; do
    path="$(dirname -- "$path")"
  done
  [[ -d "$path" ]] || die 'POC path parent is not a directory'
  printf '%s\n' "$path"
}

filesystem_type_for_path() {
  local path="$1"

  require_command findmnt
  findmnt --noheadings --output FSTYPE --target "$path" | awk 'NF { print $1; exit }'
}

assert_linux_native_filesystem() {
  local path="$1"
  local fstype

  case "$path" in
    /mnt|/mnt/*) die 'Windows-mounted paths are not allowed for the POC root' ;;
  esac

  fstype="$(filesystem_type_for_path "$path")"
  case "$fstype" in
    ext2|ext3|ext4|xfs|btrfs|zfs|tmpfs|overlay) ;;
    '') die 'unable to determine the POC root filesystem type' ;;
    *) die "POC root filesystem is not an approved Linux-native type: $fstype" ;;
  esac
}

canonicalize_absolute_path() {
  local raw_path="$1"

  [[ -n "$raw_path" ]] || die 'a path is required'
  [[ "$raw_path" == /* ]] || die 'relative paths are not allowed'
  assert_safe_poc_path_characters "$raw_path"
  assert_no_parent_traversal "$raw_path"
  assert_no_symlink_components "$raw_path"
  require_command realpath
  realpath --canonicalize-missing -- "$raw_path"
}

validate_poc_root() {
  local raw_root="$1"
  local root
  local parent
  local home
  local codex_home
  local repo

  root="$(canonicalize_absolute_path "$raw_root")" || return 1
  [[ "$root" != '/' ]] || die 'the filesystem root is not a valid POC root'

  home="$(realpath --canonicalize-existing -- "$HOME")" || return 1
  codex_home="$(realpath --canonicalize-missing -- "$home/.codex")" || return 1
  repo="$(realpath --canonicalize-existing -- "$REPO_ROOT")" || return 1

  [[ "$root" != "$home" ]] || die 'the user HOME directory is not a valid POC root'
  ! is_within_path "$root" "$codex_home" || die 'the real global CODEX_HOME is not a valid POC root'
  ! is_within_path "$root" "$repo" || die 'the repository or a repository subdirectory is not a valid POC root'

  parent="$(nearest_existing_parent "$root")" || return 1
  assert_no_symlink_components "$parent"
  assert_linux_native_filesystem "$parent"

  if [[ -e "$root" ]]; then
    [[ -d "$root" ]] || die 'the POC root exists but is not a directory'
    [[ ! -L "$root" ]] || die 'the POC root must not be a symbolic link'
    [[ "$(stat --format='%u' -- "$root")" == "$(id --user)" ]] || die 'the existing POC root is not owned by the current user'
  fi

  printf '%s\n' "$root"
}

validate_file_within_poc_root() {
  local raw_file="$1"
  local root="$2"
  local file

  file="$(canonicalize_absolute_path "$raw_file")" || return 1
  [[ -e "$file" ]] || die 'the requested file does not exist'
  [[ ! -L "$file" ]] || die 'symbolic-link files are not allowed'
  [[ -f "$file" ]] || die 'the requested path is not a regular file'
  is_within_path "$file" "$root" || die 'the requested file is outside the explicit POC root'
  [[ "$(stat --format='%u' -- "$file")" == "$(id --user)" ]] || die 'the requested file is not owned by the current user'
  [[ "$(stat --format='%a' -- "$file")" == '600' ]] || die 'the requested file must have mode 0600'
  printf '%s\n' "$file"
}

relative_poc_path() {
  local path="$1"
  local root="$2"

  if [[ "$path" == "$root" ]]; then
    printf '<POC_ROOT>\n'
  else
    printf '<POC_ROOT>/%s\n' "${path#"$root"/}"
  fi
}

declare -ar POC_REQUIRED_DIRECTORIES=(
  accounts
  accounts/account-a
  accounts/account-b
  projects
  projects/project-a
  projects/project-a/codex-home
  projects/project-b
  projects/project-b/codex-home
  backups
  evidence
  manifests
  tmp
)

verify_poc_layout() {
  local root="$1"
  local relative
  local target
  local symlink

  [[ -d "$root" && ! -L "$root" ]] || die 'the POC root is missing or unsafe'
  [[ "$(stat --format='%u' -- "$root")" == "$(id --user)" ]] || die 'the POC root owner is not the current user'
  [[ "$(stat --format='%a' -- "$root")" == '700' ]] || die 'the POC root mode is not 0700'

  for relative in "${POC_REQUIRED_DIRECTORIES[@]}"; do
    target="$root/$relative"
    [[ -d "$target" && ! -L "$target" ]] || die "required POC directory is missing or unsafe: $relative"
    [[ "$(stat --format='%u' -- "$target")" == "$(id --user)" ]] || die "required POC directory has an unexpected owner: $relative"
    [[ "$(stat --format='%a' -- "$target")" == '700' ]] || die "required POC directory does not have mode 0700: $relative"
  done

  symlink="$(find -P "$root" -type l -print -quit)"
  [[ -z "$symlink" ]] || die 'symbolic links are not permitted anywhere in the Phase 2A POC root'

  symlink="$(find -P "$root" -name auth.json -print -quit)"
  [[ -z "$symlink" ]] || die 'Phase 2A POC roots must not contain auth.json'
}

scan_for_sensitive_markers() {
  local file="$1"
  local pattern='access_token|refresh_token|id_token|Bearer[[:space:]]|Authorization:|Cookie:|client_secret|private_key|BEGIN[[:space:]]+(RSA[[:space:]]+)?PRIVATE[[:space:]]+KEY'

  if LC_ALL=C grep --binary-files=without-match --extended-regexp --ignore-case --quiet "$pattern" "$file"; then
    return 1
  fi
  return 0
}
