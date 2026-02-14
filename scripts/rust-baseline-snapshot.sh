#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out_dir="${1:-build/rust-baseline/latest}"
mode="${2:-}"

if [[ "$out_dir" = /* ]]; then
  out_dir_abs="$out_dir"
else
  out_dir_abs="$repo_root/$out_dir"
fi

mkdir -p "$out_dir_abs"

(
  cd old/go
  env -u GOROOT -u GOTOOLDIR go run ./cmd/forge --help > "$out_dir_abs/forge-help.txt"
  env -u GOROOT -u GOTOOLDIR go run ./cmd/fmail --help > "$out_dir_abs/fmail-help.txt"
  env -u GOROOT -u GOTOOLDIR go run ./cmd/fmail-tui --help > "$out_dir_abs/fmail-tui-help.txt"
  env -u GOROOT -u GOTOOLDIR go run ./cmd/schema-fingerprint --out-dir "$out_dir_abs" >/dev/null
)

find old/go/internal/db/migrations -type f -name '*.sql' | sort > "$out_dir_abs/db-migrations.txt"
find old/go/cmd old/go/internal -type f -name '*.go' -print0 | xargs -0 wc -l > "$out_dir_abs/go-loc-summary.txt"
shasum -a 256 old/go/proto/forged/v1/forged.proto > "$out_dir_abs/proto-forged-sha256.txt"
date -u +"%Y-%m-%dT%H:%M:%SZ" > "$out_dir_abs/generated-at.txt"

if [[ "$mode" == "--check" ]]; then
  diff -u docs/forge-mail/help/fmail-help.txt "$out_dir_abs/fmail-help.txt"
  diff -u docs/forge-mail/help/fmail-tui-help.txt "$out_dir_abs/fmail-tui-help.txt"
  diff -u old/go/internal/parity/testdata/schema/schema-fingerprint.sha256 "$out_dir_abs/schema-fingerprint.sha256"
fi

echo "baseline snapshot written to $out_dir_abs"
