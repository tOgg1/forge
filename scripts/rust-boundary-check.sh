#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

policy_file="${1:-docs/rust-crate-boundaries.json}"

env -u GOROOT -u GOTOOLDIR go run ./cmd/rust-boundary-check --policy "$policy_file"
