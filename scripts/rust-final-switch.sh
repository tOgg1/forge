#!/usr/bin/env bash
set -euo pipefail

print_help() {
  cat <<'USAGE'
rust-final-switch.sh

One-command cutover/rollback wrapper for final Rust switch.

Usage:
  scripts/rust-final-switch.sh <cutover|rollback> [options]

Modes:
  cutover      run Rust switch command, then verification checklist commands
  rollback     run Go rollback command, then verification checklist commands

Options:
  --cutover-cmd <cmd>      command to switch to Rust ownership
                            (default: FORGE_SWITCH_CUTOVER_CMD env)
  --rollback-cmd <cmd>     command to switch back to Go ownership
                            (default: FORGE_SWITCH_ROLLBACK_CMD env)
  --verify-cmd <cmd>       verification command; can be repeated
  --hook <cmd>             lifecycle hook command; can be repeated
  --no-default-verify      disable defaults (`forge --version`, `forge doctor`)
  --log-file <path>        append timestamped log lines
  --dry-run                print actions only
  -h, --help               show this help

Hooks:
  Hook commands execute with:
    FORGE_SWITCH_EVENT: pre_switch|post_switch|verify_start|verify_pass|verify_fail
    FORGE_SWITCH_MODE: cutover|rollback
    FORGE_SWITCH_STATUS: ok|failed
    FORGE_SWITCH_COMMAND: current switch/verify command
    FORGE_SWITCH_VERIFY_INDEX: 1-based verify step index
    FORGE_SWITCH_VERIFY_TOTAL: verify step count
    FORGE_SWITCH_LOG_FILE: log file path (if set)
USAGE
}

mode=""
cutover_cmd="${FORGE_SWITCH_CUTOVER_CMD:-}"
rollback_cmd="${FORGE_SWITCH_ROLLBACK_CMD:-}"
log_file=""
dry_run=0
use_default_verify=1
verify_cmds=()
hooks=()

log_line() {
  local message="$1"
  local now
  now="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  printf '[rust-final-switch] %s %s\n' "$now" "$message"
  if [[ -n "$log_file" ]]; then
    mkdir -p "$(dirname "$log_file")"
    printf '[rust-final-switch] %s %s\n' "$now" "$message" >>"$log_file"
  fi
}

run_hooks() {
  local event="$1"
  local status="$2"
  local command="$3"
  local verify_index="${4:-0}"
  local verify_total="${5:-0}"
  local hook
  if (( ${#hooks[@]} == 0 )); then return 0; fi
  for hook in "${hooks[@]}"; do
    if (( dry_run == 1 )); then
      log_line "dry-run hook event=$event cmd=$hook"
      continue
    fi
    FORGE_SWITCH_EVENT="$event" \
      FORGE_SWITCH_MODE="$mode" \
      FORGE_SWITCH_STATUS="$status" \
      FORGE_SWITCH_COMMAND="$command" \
      FORGE_SWITCH_VERIFY_INDEX="$verify_index" \
      FORGE_SWITCH_VERIFY_TOTAL="$verify_total" \
      FORGE_SWITCH_LOG_FILE="$log_file" \
      bash -lc "$hook"
  done
}

run_command() {
  local label="$1"
  local command="$2"
  if (( dry_run == 1 )); then
    log_line "dry-run $label: $command"
    return 0
  fi
  log_line "$label: $command"
  bash -lc "$command"
}

while (($# > 0)); do
  case "$1" in
    cutover|rollback)
      if [[ -n "$mode" ]]; then
        echo "mode already set: $mode" >&2
        exit 2
      fi
      mode="$1"
      ;;
    --cutover-cmd)
      shift
      cutover_cmd="${1:-}"
      ;;
    --rollback-cmd)
      shift
      rollback_cmd="${1:-}"
      ;;
    --verify-cmd)
      shift
      verify_cmds+=("${1:-}")
      ;;
    --hook)
      shift
      hooks+=("${1:-}")
      ;;
    --no-default-verify)
      use_default_verify=0
      ;;
    --log-file)
      shift
      log_file="${1:-}"
      ;;
    --dry-run)
      dry_run=1
      ;;
    -h|--help)
      print_help
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      echo "try --help" >&2
      exit 2
      ;;
  esac
  shift || true
done

if [[ -z "$mode" ]]; then
  echo "missing required mode: cutover|rollback" >&2
  exit 2
fi

if [[ "$mode" == "cutover" && -z "$cutover_cmd" ]]; then
  echo "missing cutover command (--cutover-cmd or FORGE_SWITCH_CUTOVER_CMD)" >&2
  exit 2
fi

if [[ "$mode" == "rollback" && -z "$rollback_cmd" ]]; then
  echo "missing rollback command (--rollback-cmd or FORGE_SWITCH_ROLLBACK_CMD)" >&2
  exit 2
fi

if (( use_default_verify == 1 && ${#verify_cmds[@]} == 0 )); then
  verify_cmds=(
    "forge --version"
    "forge doctor"
  )
fi

switch_cmd="$cutover_cmd"
if [[ "$mode" == "rollback" ]]; then
  switch_cmd="$rollback_cmd"
fi

log_line "mode=$mode verify_count=${#verify_cmds[@]}"
run_hooks "pre_switch" "ok" "$switch_cmd" "0" "${#verify_cmds[@]}"
run_command "switch" "$switch_cmd"
run_hooks "post_switch" "ok" "$switch_cmd" "0" "${#verify_cmds[@]}"

if (( ${#verify_cmds[@]} == 0 )); then
  log_line "no verification commands configured"
fi

for i in "${!verify_cmds[@]}"; do
  index=$((i + 1))
  cmd="${verify_cmds[$i]}"
  run_hooks "verify_start" "ok" "$cmd" "$index" "${#verify_cmds[@]}"
  if run_command "verify[$index/${#verify_cmds[@]}]" "$cmd"; then
    run_hooks "verify_pass" "ok" "$cmd" "$index" "${#verify_cmds[@]}"
    continue
  fi
  run_hooks "verify_fail" "failed" "$cmd" "$index" "${#verify_cmds[@]}"
  log_line "verification failed at step $index"
  exit 1
done

log_line "completed mode=$mode"
