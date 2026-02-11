#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# Full daemon bring-up parity suite:
# - rforged gRPC lifecycle
# - rforge up --spawn-owner daemon tmp-repo e2e
# - multi-loop daemon ownership/targeting/bulk-stop e2e
cargo test -p forge-daemon --test rforged_binary_test
