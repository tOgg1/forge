#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: scripts/rust-final-switch-checklist-hook.sh <markdown-log-path>" >&2
  exit 2
fi

out_path="$1"
event="${FORGE_SWITCH_EVENT:-}"
mode="${FORGE_SWITCH_MODE:-}"
status="${FORGE_SWITCH_STATUS:-}"
command="${FORGE_SWITCH_COMMAND:-}"
index="${FORGE_SWITCH_VERIFY_INDEX:-0}"
total="${FORGE_SWITCH_VERIFY_TOTAL:-0}"

if [[ "$event" != "verify_pass" && "$event" != "verify_fail" ]]; then
  exit 0
fi

stamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
result="PASS"
if [[ "$event" == "verify_fail" || "$status" == "failed" ]]; then
  result="FAIL"
fi

mkdir -p "$(dirname "$out_path")"
if [[ ! -f "$out_path" ]]; then
  cat <<'HEADER' >"$out_path"
# Rust Final Switch Checklist Log

| Time (UTC) | Mode | Verify Step | Result | Command |
|---|---|---|---|---|
HEADER
fi

printf '| %s | %s | %s/%s | %s | `%s` |\n' \
  "$stamp" \
  "$mode" \
  "$index" \
  "$total" \
  "$result" \
  "$command" >>"$out_path"
