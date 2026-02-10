#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: scripts/go-layout-guard.sh [--repo-root <path>] [--mode <root|mixed|legacy>]

Modes:
  root    current layout only (Go sources at repo root, legacy/old-go absent)
  mixed   transition layout (root + legacy/old-go both present)
  legacy  post-move layout (legacy/old-go present; root Go tree removed)
USAGE
}

repo_root="."
mode="root"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      shift
      repo_root="${1:-}"
      ;;
    --mode)
      shift
      mode="${1:-}"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift || true
done

if [[ -z "$repo_root" || -z "$mode" ]]; then
  echo "missing required argument value" >&2
  usage >&2
  exit 1
fi

root_required=(
  "cmd"
  "internal"
  "pkg"
  "proto"
  "go.mod"
)

legacy_required=(
  "legacy/old-go/cmd"
  "legacy/old-go/internal"
  "legacy/old-go/pkg"
  "legacy/old-go/proto"
  "legacy/old-go/go.mod"
)

missing=()
unexpected=()

require_paths() {
  local path
  for path in "$@"; do
    if [[ ! -e "$repo_root/$path" ]]; then
      missing+=("$path")
    fi
  done
}

require_absent() {
  local path
  for path in "$@"; do
    if [[ -e "$repo_root/$path" ]]; then
      unexpected+=("$path")
    fi
  done
}

case "$mode" in
  root)
    require_paths "${root_required[@]}"
    require_absent "legacy/old-go"
    ;;
  mixed)
    require_paths "${root_required[@]}"
    require_paths "${legacy_required[@]}"
    ;;
  legacy)
    require_paths "${legacy_required[@]}"
    require_absent "${root_required[@]}"
    ;;
  *)
    echo "invalid --mode: $mode (expected root|mixed|legacy)" >&2
    exit 1
    ;;
esac

if (( ${#missing[@]} > 0 )); then
  echo "go layout guard failed ($mode): missing paths" >&2
  printf '  - %s\n' "${missing[@]}" >&2
  exit 1
fi

if (( ${#unexpected[@]} > 0 )); then
  echo "go layout guard failed ($mode): unexpected paths" >&2
  printf '  - %s\n' "${unexpected[@]}" >&2
  exit 1
fi

echo "go layout guard ok ($mode)"
