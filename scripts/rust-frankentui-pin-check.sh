#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo_toml="crates/forge-ftui-adapter/Cargo.toml"
cargo_lock="Cargo.lock"
pin_rev="23429fac0e739635c7b8e0b995bde09401ff6ea0"
pin_url="https://github.com/Dicklesworthstone/frankentui"

search_pattern() {
  local pattern="$1"
  local path="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -n "$pattern" "$path" >/dev/null
  else
    grep -En "$pattern" "$path" >/dev/null
  fi
}

search_pattern "ftui = \\{ git = \\\"${pin_url}\\\", rev = \\\"${pin_rev}\\\"" "$cargo_toml"
search_pattern "name = \\\"forge-ftui-adapter\\\"" "$cargo_lock"

if search_pattern "name = \\\"ftui\\\"" "$cargo_lock"; then
  search_pattern "${pin_url}\\?rev=${pin_rev}#${pin_rev}" "$cargo_lock"
fi

echo "rust-frankentui-pin-check: PASS"
