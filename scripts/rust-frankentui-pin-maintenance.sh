#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: scripts/rust-frankentui-pin-maintenance.sh [options]

Options:
  --rev <sha>      update frankentui pin rev in adapter Cargo.toml before checks
  --check-only     skip pin mutation/update; run validation workflow only
  --skip-smoke     skip loop/fmail parity smoke checks
  -h, --help       show help

Workflow:
  1) (optional) update pin rev in rust/crates/forge-ftui-adapter/Cargo.toml
  2) cargo update -p ftui
  3) cargo check --workspace
  4) scripts/rust-frankentui-pin-check.sh
  5) scripts/rust-loop-tui-smoke.sh + scripts/rust-fmail-tui-smoke.sh
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_toml="$repo_root/rust/crates/forge-ftui-adapter/Cargo.toml"

new_rev=""
check_only=0
skip_smoke=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --rev)
      shift
      new_rev="${1:-}"
      ;;
    --check-only)
      check_only=1
      ;;
    --skip-smoke)
      skip_smoke=1
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

if [[ -n "$new_rev" && "$check_only" -eq 1 ]]; then
  echo "--rev and --check-only are mutually exclusive" >&2
  exit 1
fi

if [[ -n "$new_rev" ]]; then
  python3 - "$cargo_toml" "$new_rev" <<'PY'
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
new_rev = sys.argv[2].strip()
body = path.read_text()
pattern = r'(ftui\s*=\s*\{\s*git\s*=\s*"https://github.com/Dicklesworthstone/frankentui",\s*rev\s*=\s*")([0-9a-f]+)(")'
updated, n = re.subn(pattern, r'\1' + new_rev + r'\3', body)
if n != 1:
    raise SystemExit("failed to locate frankentui pin entry in Cargo.toml")
path.write_text(updated)
print(f"updated frankentui rev to {new_rev}")
PY
fi

cd "$repo_root/rust"

if [[ "$check_only" -eq 0 ]]; then
  cargo update -p ftui
fi

cargo check --workspace

cd "$repo_root"
scripts/rust-frankentui-pin-check.sh

if [[ "$skip_smoke" -eq 0 ]]; then
  scripts/rust-loop-tui-smoke.sh
  scripts/rust-fmail-tui-smoke.sh
fi

echo "rust-frankentui-pin-maintenance: PASS"
