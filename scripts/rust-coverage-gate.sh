#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root/rust"

threshold_file="${1:-coverage-thresholds.txt}"

if [[ ! -f "$threshold_file" ]]; then
  echo "threshold file not found: $threshold_file" >&2
  exit 1
fi

while read -r crate threshold; do
  [[ -z "${crate:-}" ]] && continue
  [[ "$crate" =~ ^# ]] && continue
  if [[ -z "${threshold:-}" ]]; then
    echo "invalid threshold row: $crate" >&2
    exit 1
  fi

  echo "enforcing $crate >= ${threshold}% line coverage"
  cargo llvm-cov --package "$crate" --summary-only --fail-under-lines "$threshold"
done < "$threshold_file"
