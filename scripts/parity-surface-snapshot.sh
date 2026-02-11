#!/usr/bin/env bash
# parity-surface-snapshot.sh — generate Go+Rust CLI surface JSON snapshots
# and optionally run the diff gate.
#
# Usage:
#   scripts/parity-surface-snapshot.sh [--out-dir <dir>] [--check]
#
# Outputs:
#   <out-dir>/go-surface.json
#   <out-dir>/rust-surface.json
#   <out-dir>/surface-report.json
#   <out-dir>/generated-at.txt
#
# --check  exits non-zero if drift is detected.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out_dir="build/parity-surface/latest"
check=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir) out_dir="${2:-}"; shift 2 ;;
    --out-dir=*) out_dir="${1#*=}"; shift ;;
    --check) check=1; shift ;;
    -h|--help)
      sed -n '2,/^$/s/^# //p' "$0"
      exit 0
      ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

mkdir -p "$out_dir"

echo "==> Running surface gate test..."
(
  cd "$repo_root/old/go"
  env -u GOROOT -u GOTOOLDIR go test ./internal/parity \
    -run TestSurfaceGateGoVsRust \
    -count=1 \
    -v \
    -timeout 300s
) 2>&1 | tee "$out_dir/gate-output.txt"

gate_exit=${PIPESTATUS[0]}

date -u +"%Y-%m-%dT%H:%M:%SZ" > "$out_dir/generated-at.txt"

echo "==> Surface gate output: $out_dir/gate-output.txt"

if [[ "$check" -eq 1 && "$gate_exit" -ne 0 ]]; then
  echo "FAIL: surface parity drift detected" >&2
  exit "$gate_exit"
fi

exit 0
