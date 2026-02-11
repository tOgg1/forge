#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

policy_file="${1:-docs/rust-crate-boundaries.json}"
policy_file_abs="$policy_file"
if [[ "$policy_file_abs" != /* ]]; then
  policy_file_abs="$repo_root/$policy_file_abs"
fi

(
  cd "$repo_root/old/go"
  env -u GOROOT -u GOTOOLDIR go run ./cmd/rust-boundary-check --policy "$policy_file_abs"
)
