#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestSchemaFingerprintBaseline$' -count=1

cd "$repo_root/rust"
cargo test -p forge-db --test transaction_parity_test
cargo test -p forge-db --test go_db_compat_read_test
cargo test -p forge-cli --test migrate_go_oracle_fixture_test
