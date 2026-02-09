#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo_toml="rust/crates/forge-ftui-adapter/Cargo.toml"
cargo_lock="rust/Cargo.lock"
pin_rev="23429fac0e739635c7b8e0b995bde09401ff6ea0"
pin_url="https://github.com/Dicklesworthstone/frankentui"

rg -n "ftui = \\{ git = \\\"${pin_url}\\\", rev = \\\"${pin_rev}\\\"" "$cargo_toml" >/dev/null
rg -n "name = \\\"forge-ftui-adapter\\\"" "$cargo_lock" >/dev/null

if rg -n "name = \\\"ftui\\\"" "$cargo_lock" >/dev/null; then
  rg -n "${pin_url}\\?rev=${pin_rev}#${pin_rev}" "$cargo_lock" >/dev/null
fi

echo "rust-frankentui-pin-check: PASS"
