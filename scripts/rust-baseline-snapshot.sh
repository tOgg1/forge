#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out_dir="${1:-build/rust-baseline/latest}"
mode="${2:-}"

mkdir -p "$out_dir"

(
  cd old/go
  env -u GOROOT -u GOTOOLDIR go run ./cmd/forge --help > "$repo_root/$out_dir/forge-help.txt"
  env -u GOROOT -u GOTOOLDIR go run ./cmd/fmail --help > "$repo_root/$out_dir/fmail-help.txt"
  env -u GOROOT -u GOTOOLDIR go run ./cmd/fmail-tui --help > "$repo_root/$out_dir/fmail-tui-help.txt"
  env -u GOROOT -u GOTOOLDIR go run ./cmd/schema-fingerprint --out-dir "$repo_root/$out_dir" >/dev/null
)

find old/go/internal/db/migrations -type f -name '*.sql' | sort > "$out_dir/db-migrations.txt"
find old/go/cmd old/go/internal -type f -name '*.go' -print0 | xargs -0 wc -l > "$out_dir/go-loc-summary.txt"
shasum -a 256 old/go/proto/forged/v1/forged.proto > "$out_dir/proto-forged-sha256.txt"
date -u +"%Y-%m-%dT%H:%M:%SZ" > "$out_dir/generated-at.txt"

if [[ "$mode" == "--check" ]]; then
  diff -u docs/forge-mail/help/fmail-help.txt "$out_dir/fmail-help.txt"
  diff -u docs/forge-mail/help/fmail-tui-help.txt "$out_dir/fmail-tui-help.txt"
  diff -u old/go/internal/parity/testdata/schema/schema-fingerprint.sha256 "$out_dir/schema-fingerprint.sha256"
fi

echo "baseline snapshot written to $out_dir"
