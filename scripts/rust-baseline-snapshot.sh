#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out_dir="${1:-build/rust-baseline/latest}"
mode="${2:-}"

mkdir -p "$out_dir"

env -u GOROOT -u GOTOOLDIR go run ./cmd/forge --help > "$out_dir/forge-help.txt"
env -u GOROOT -u GOTOOLDIR go run ./cmd/fmail --help > "$out_dir/fmail-help.txt"
env -u GOROOT -u GOTOOLDIR go run ./cmd/fmail-tui --help > "$out_dir/fmail-tui-help.txt"

find internal/db/migrations -type f -name '*.sql' | sort > "$out_dir/db-migrations.txt"
find cmd internal -type f -name '*.go' -print0 | xargs -0 wc -l > "$out_dir/go-loc-summary.txt"
shasum -a 256 proto/forged/v1/forged.proto > "$out_dir/proto-forged-sha256.txt"
date -u +"%Y-%m-%dT%H:%M:%SZ" > "$out_dir/generated-at.txt"

if [[ "$mode" == "--check" ]]; then
  diff -u docs/forge-mail/help/fmail-help.txt "$out_dir/fmail-help.txt"
  diff -u docs/forge-mail/help/fmail-tui-help.txt "$out_dir/fmail-tui-help.txt"
fi

echo "baseline snapshot written to $out_dir"
