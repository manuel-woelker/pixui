#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_BRANCH="${PIXUI_RELEASE_BRANCH:-main}"
WAIT_ATTEMPTS="${PIXUI_RELEASE_WAIT_ATTEMPTS:-30}"
WAIT_SECONDS="${PIXUI_RELEASE_WAIT_SECONDS:-10}"
DRY_RUN=0
MODE="publish"
TARGET_VERSION=""

RELEASE_CRATES=(
  "crates/base:pixui-base"
  "crates/pal:pixui-pal"
)

usage() {
  cat <<'EOF'
Usage:
  ./scripts/release.sh release <version>
  ./scripts/release.sh prepare <version>
  ./scripts/release.sh publish [--dry-run]
  ./scripts/release.sh [--dry-run]

Options:
  --dry-run    Run all pre-release checks and packaging validation without
               publishing crates, creating tags, or pushing anything.
  --help       Show this help.
EOF
}

log() {
  printf '==> %s\n' "$*"
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

require_tool() {
  local tool="$1"
  command -v "$tool" >/dev/null 2>&1 || fail "required tool not found: $tool"
}

manifest_value() {
  local manifest_path="$1"
  local key="$2"
  sed -nE "s/^${key}[[:space:]]*=[[:space:]]*\"([^\"]+)\"$/\\1/p" "$manifest_path" | head -n1
}

crate_version() {
  local crate_dir="$1"
  manifest_value "$ROOT_DIR/$crate_dir/Cargo.toml" "version"
}

release_crate_dir() {
  local release_crate="$1"
  printf '%s\n' "${release_crate%%:*}"
}

release_package_name() {
  local release_crate="$1"
  printf '%s\n' "${release_crate##*:}"
}

assert_release_crates_cover_all_crates() {
  local release_crate
  local crate_dir
  local manifest_path
  local actual_crate_dir
  local missing=()
  local extra=()
  declare -A configured_crates=()
  declare -A actual_crates=()

  for release_crate in "${RELEASE_CRATES[@]}"; do
    crate_dir="$(release_crate_dir "$release_crate")"
    configured_crates["$crate_dir"]=1
  done

  while IFS= read -r manifest_path; do
    actual_crate_dir="${manifest_path%/Cargo.toml}"
    actual_crates["$actual_crate_dir"]=1
  done < <(find "$ROOT_DIR/crates" -mindepth 2 -maxdepth 2 -name Cargo.toml | sort)

  for actual_crate_dir in "${!actual_crates[@]}"; do
    if [[ -z "${configured_crates[$actual_crate_dir]:-}" ]]; then
      missing+=("${actual_crate_dir#"$ROOT_DIR/"}")
    fi
  done

  for crate_dir in "${!configured_crates[@]}"; do
    if [[ -z "${actual_crates["$ROOT_DIR/$crate_dir"]:-}" ]]; then
      extra+=("$crate_dir")
    fi
  done

  if [[ "${#missing[@]}" -gt 0 || "${#extra[@]}" -gt 0 ]]; then
    if [[ "${#missing[@]}" -gt 0 ]]; then
      printf 'error: RELEASE_CRATES is missing workspace crates:\n' >&2
      printf '  %s\n' "${missing[@]}" >&2
    fi

    if [[ "${#extra[@]}" -gt 0 ]]; then
      printf 'error: RELEASE_CRATES references unknown crate entries:\n' >&2
      printf '  %s\n' "${extra[@]}" >&2
    fi

    exit 1
  fi
}

release_version() {
  crate_version "crates/base"
}

validate_version() {
  local version="$1"
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || fail "invalid version: $version"
}

current_branch() {
  git -C "$ROOT_DIR" symbolic-ref --quiet --short HEAD || true
}

assert_clean_worktree() {
  if [[ -n "$(git -C "$ROOT_DIR" status --short)" ]]; then
    fail "git worktree is not clean"
  fi
}

assert_release_branch() {
  local branch
  branch="$(current_branch)"
  [[ -n "$branch" ]] || fail "release requires a checked out branch, not a detached HEAD"
  [[ "$branch" == "$RELEASE_BRANCH" ]] || fail "release must run from branch '$RELEASE_BRANCH' (current: '$branch')"
}

assert_publish_auth() {
  if [[ -n "${CARGO_REGISTRY_TOKEN:-}" ]]; then
    return
  fi

  if [[ -f "${CARGO_HOME:-$HOME/.cargo}/credentials.toml" ]] || [[ -f "${CARGO_HOME:-$HOME/.cargo}/credentials" ]]; then
    return
  fi

  fail "crates.io credentials not found; set CARGO_REGISTRY_TOKEN or run cargo login"
}

assert_tag_absent() {
  local tag_name="$1"
  if git -C "$ROOT_DIR" rev-parse -q --verify "refs/tags/$tag_name" >/dev/null 2>&1; then
    fail "tag already exists locally: $tag_name"
  fi
}

crate_version_exists_on_crates_io() {
  local package_name="$1"
  local version="$2"
  local url="https://crates.io/api/v1/crates/$package_name/$version"
  curl --fail --silent --show-error "$url" >/dev/null 2>&1
}

assert_release_version_available() {
  local version="$1"
  local release_crate
  local package_name

  for release_crate in "${RELEASE_CRATES[@]}"; do
    package_name="$(release_package_name "$release_crate")"
    if crate_version_exists_on_crates_io "$package_name" "$version"; then
      fail "$package_name $version already exists on crates.io; bump the shared version before publishing"
    fi
  done
}

assert_shared_version() {
  local expected_version="$1"
  local release_crate
  local crate_dir

  for release_crate in "${RELEASE_CRATES[@]}"; do
    crate_dir="$(release_crate_dir "$release_crate")"
    local actual_version
    actual_version="$(crate_version "$crate_dir")"
    [[ -n "$actual_version" ]] || fail "missing version in $crate_dir/Cargo.toml"
    [[ "$actual_version" == "$expected_version" ]] || fail "$crate_dir/Cargo.toml uses version $actual_version, expected $expected_version"
  done
}

assert_internal_dependency_versions() {
  local expected_version="$1"
  local release_crate
  local crate_dir
  local manifest_path

  for release_crate in "${RELEASE_CRATES[@]}"; do
    crate_dir="$(release_crate_dir "$release_crate")"
    manifest_path="$ROOT_DIR/$crate_dir/Cargo.toml"

    while IFS= read -r dependency_line; do
      local dependency_name
      dependency_name="$(printf '%s\n' "$dependency_line" | sed -nE 's/^([a-z0-9-]+)[[:space:]]*=.*/\1/p')"
      local dependency_version
      dependency_version="$(printf '%s\n' "$dependency_line" | sed -nE 's/.*version[[:space:]]*=[[:space:]]*\"([^\"]+)\".*/\1/p')"

      [[ -n "$dependency_version" ]] || fail "$manifest_path is missing a version for internal dependency $dependency_name"
      [[ "$dependency_version" == "$expected_version" ]] || fail "$manifest_path pins $dependency_name to $dependency_version, expected $expected_version"
    done < <(grep -E '^[a-z0-9-]+[[:space:]]*=.*path[[:space:]]*=' "$manifest_path" || true)
  done
}

update_manifest_version() {
  local manifest_path="$1"
  local version="$2"

  sed -E -i \
    -e "0,/^version[[:space:]]*=[[:space:]]*\"[^\"]+\"$/s//version = \"$version\"/" \
    -e "/path[[:space:]]*=/ s/version[[:space:]]*=[[:space:]]*\"[^\"]+\"/version = \"$version\"/g" \
    "$manifest_path"
}

prepare_release_version() {
  local version="$1"
  local print_manual_next_step="${2:-1}"
  local release_crate
  local crate_dir
  local manifest_path
  local current_version

  validate_version "$version"
  current_version="$(release_version)"
  [[ "$current_version" != "$version" ]] || fail "workspace already uses version $version"

  require_tool git
  require_tool sed
  assert_clean_worktree

  for release_crate in "${RELEASE_CRATES[@]}"; do
    crate_dir="$(release_crate_dir "$release_crate")"
    manifest_path="$ROOT_DIR/$crate_dir/Cargo.toml"
    log "updating $manifest_path to version $version"
    update_manifest_version "$manifest_path" "$version"
  done

  cargo update --workspace --offline

  assert_shared_version "$version"
  assert_internal_dependency_versions "$version"

  log "prepared shared release version $version"
  if [[ "$print_manual_next_step" -eq 1 ]]; then
    log "review the manifest changes, commit them, then run ./scripts/release.sh publish"
  fi
}

run_repo_checks() {
  log "running repository checks"
  nao check
}

wait_for_crate_version() {
  local package_name="$1"
  local version="$2"
  local attempt
  local url="https://crates.io/api/v1/crates/$package_name/$version"

  for ((attempt = 1; attempt <= WAIT_ATTEMPTS; attempt += 1)); do
    if curl --fail --silent --show-error "$url" >/dev/null; then
      return
    fi

    sleep "$WAIT_SECONDS"
  done

  fail "timed out waiting for $package_name $version to become available on crates.io"
}

package_release_artifacts() {
  local release_crate
  local package_name

  rm -rf "$ROOT_DIR/dist"
  mkdir -p "$ROOT_DIR/dist"

  for release_crate in "${RELEASE_CRATES[@]}"; do
    package_name="$(release_package_name "$release_crate")"
    cargo package --locked -p "$package_name"
    cp "$ROOT_DIR/target/package/${package_name}-$(release_version).crate" "$ROOT_DIR/dist/"
  done
}

publish_crates() {
  local version="$1"
  local release_crate
  local package_name

  for release_crate in "${RELEASE_CRATES[@]}"; do
    package_name="$(release_package_name "$release_crate")"

    if [[ "$DRY_RUN" -eq 1 ]]; then
      log "dry-run: cargo publish --locked --dry-run -p $package_name"
      cargo publish --locked --dry-run -p "$package_name"
      continue
    fi

    log "publishing $package_name $version"
    cargo publish --locked -p "$package_name"
    wait_for_crate_version "$package_name" "$version"
  done
}

create_and_push_tag() {
  local version="$1"
  local tag_name="v$version"

  [[ "$DRY_RUN" -eq 1 ]] && return

  assert_tag_absent "$tag_name"
  git -C "$ROOT_DIR" tag "$tag_name"
  git -C "$ROOT_DIR" push origin "$tag_name"
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      release|prepare|publish)
        MODE="$1"
        shift
        ;;
      --dry-run)
        DRY_RUN=1
        shift
        ;;
      --help|-h)
        usage
        exit 0
        ;;
      *)
        if [[ -z "$TARGET_VERSION" ]]; then
          TARGET_VERSION="$1"
          shift
        else
          fail "unexpected argument: $1"
        fi
        ;;
    esac
  done
}

main() {
  parse_args "$@"

  require_tool cargo
  require_tool curl
  require_tool git
  assert_release_crates_cover_all_crates

  case "$MODE" in
    prepare)
      [[ -n "$TARGET_VERSION" ]] || fail "prepare requires a version"
      prepare_release_version "$TARGET_VERSION"
      ;;
    publish)
      [[ -z "$TARGET_VERSION" ]] || fail "publish does not take a version"
      local version
      version="$(release_version)"
      assert_release_branch
      assert_clean_worktree
      assert_shared_version "$version"
      assert_internal_dependency_versions "$version"
      assert_release_version_available "$version"
      run_repo_checks
      package_release_artifacts
      [[ "$DRY_RUN" -eq 1 ]] || assert_publish_auth
      publish_crates "$version"
      create_and_push_tag "$version"
      ;;
    release)
      [[ -n "$TARGET_VERSION" ]] || fail "release requires a version"
      prepare_release_version "$TARGET_VERSION" 0
      run_repo_checks
      [[ "$DRY_RUN" -eq 1 ]] || assert_publish_auth
      publish_crates "$TARGET_VERSION"
      create_and_push_tag "$TARGET_VERSION"
      ;;
    *)
      fail "unsupported mode: $MODE"
      ;;
  esac
}

main "$@"
