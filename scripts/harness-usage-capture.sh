#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
STATE_ROOT=${TMPDIR:-/tmp}
STATE_DIR="$STATE_ROOT/harness-usage-capture-$(printf "%s" "$REPO_ROOT" | shasum -a 256 | awk '{print $1}')"
PID_FILE="$STATE_DIR/usage-capture.pid"
LOG_FILE="$STATE_DIR/usage-capture.log"

DEFAULT_OUT_DIR="$REPO_ROOT/.forge/usage-capture"
DEFAULT_INTERVAL=300
DEFAULT_DAYS=7
DEFAULT_PROFILES="oc1,oc2,oc3,cc1,cc2,cc3,codex1,codex2,codex3,pi"

usage() {
  cat <<'USAGE'
Usage: scripts/harness-usage-capture.sh <run-once|start|stop|status|tail> [options]

Commands:
  run-once   Capture one snapshot now
  start      Run background capture loop
  stop       Stop background capture loop
  status     Show loop status and files
  tail       Tail capture log

Options:
  --out-dir PATH      Output dir (default: .forge/usage-capture)
  --interval SECONDS  Loop interval for start (default: 300)
  --days N            Window for opencode stats (default: 7)
  --profiles CSV      Comma-separated profiles
                      (default: oc1,oc2,oc3,cc1,cc2,cc3,codex1,codex2,codex3,pi)

Outputs:
  <out-dir>/latest.json
  <out-dir>/snapshots.jsonl

Notes:
  - OpenCode: direct CLI stats.
  - Claude: reads ~/.claude-*/stats-cache.json (local cache).
  - Codex: reads auth status + latest token usage from ~/.codex-*/log/codex-tui.log.
  - Pi: usage capture not yet supported by native CLI.
USAGE
}

err() {
  echo "error: $*" >&2
}

ensure_state_dir() {
  mkdir -p "$STATE_DIR"
}

is_running() {
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid=$(cat "$PID_FILE")
    if kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
  fi
  return 1
}

json_quote() {
  jq -Rn --arg v "$1" '$v'
}

now_iso() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

trim() {
  local s="$1"
  s="${s#"${s%%[![:space:]]*}"}"
  s="${s%"${s##*[![:space:]]}"}"
  printf '%s' "$s"
}

parse_int_field() {
  local text="$1"
  local field="$2"
  printf '%s\n' "$text" \
    | sed -n "s/.*[[:space:]]${field}[[:space:]]*\\([0-9][0-9,]*\\)[[:space:]]*|.*/\\1/p" \
    | head -n 1 \
    | tr -d ','
}

parse_any_field() {
  local text="$1"
  local field="$2"
  printf '%s\n' "$text" \
    | sed -n "s/.*[[:space:]]${field}[[:space:]]*\\([^|]*\\)[[:space:]]*|.*/\\1/p" \
    | head -n 1
}

collect_opencode() {
  local profile="$1"
  local days="$2"
  local n="${profile#oc}"
  local home="$HOME/.opencode-$n"
  local status="ok"
  local err_msg=""
  local raw=""

  if [[ ! -d "$home" ]]; then
    jq -n \
      --arg profile "$profile" \
      --arg harness "opencode" \
      --arg status "missing-home" \
      --arg home "$home" \
      '{profile:$profile,harness:$harness,status:$status,home:$home}'
    return 0
  fi

  if ! raw="$(OPENCODE_MESSAGE_QUEUE_MODE=hold XDG_DATA_HOME="$home" OPENCODE_CONFIG_DIR="$home" opencode stats --days "$days" 2>&1)"; then
    status="error"
    err_msg="$raw"
  fi

  local sessions=""
  local messages=""
  local total_cost=""
  local input_tokens=""
  local output_tokens=""
  local cache_read=""
  local cache_write=""

  if [[ "$status" == "ok" ]]; then
    sessions=$(parse_int_field "$raw" "Sessions")
    messages=$(parse_int_field "$raw" "Messages")
    total_cost=$(trim "$(parse_any_field "$raw" "Total Cost")")
    input_tokens=$(trim "$(parse_any_field "$raw" "Input")")
    output_tokens=$(trim "$(parse_any_field "$raw" "Output")")
    cache_read=$(trim "$(parse_any_field "$raw" "Cache Read")")
    cache_write=$(trim "$(parse_any_field "$raw" "Cache Write")")
  fi

  jq -n \
    --arg profile "$profile" \
    --arg harness "opencode" \
    --arg status "$status" \
    --arg home "$home" \
    --arg error "$err_msg" \
    --arg sessions "$sessions" \
    --arg messages "$messages" \
    --arg total_cost "$total_cost" \
    --arg input_tokens "$input_tokens" \
    --arg output_tokens "$output_tokens" \
    --arg cache_read "$cache_read" \
    --arg cache_write "$cache_write" \
    '{
      profile:$profile,
      harness:$harness,
      status:$status,
      home:$home,
      error:(if $error=="" then null else $error end),
      stats:{
        sessions:(if $sessions=="" then null else ($sessions|tonumber) end),
        messages:(if $messages=="" then null else ($messages|tonumber) end),
        total_cost:$total_cost,
        input:$input_tokens,
        output:$output_tokens,
        cache_read:$cache_read,
        cache_write:$cache_write
      }
    }'
}

collect_claude() {
  local profile="$1"
  local n="${profile#cc}"
  local home="$HOME/.claude-$n"
  local cache="$home/stats-cache.json"

  if [[ ! -d "$home" ]]; then
    jq -n \
      --arg profile "$profile" \
      --arg harness "claude" \
      --arg status "missing-home" \
      --arg home "$home" \
      '{profile:$profile,harness:$harness,status:$status,home:$home}'
    return 0
  fi

  if [[ ! -f "$cache" ]]; then
    jq -n \
      --arg profile "$profile" \
      --arg harness "claude" \
      --arg status "missing-cache" \
      --arg home "$home" \
      --arg cache "$cache" \
      '{profile:$profile,harness:$harness,status:$status,home:$home,cache:$cache}'
    return 0
  fi

  jq -c \
    --arg profile "$profile" \
    --arg home "$home" \
    --arg cache "$cache" \
    '
    {
      profile:$profile,
      harness:"claude",
      status:"ok",
      home:$home,
      cache:$cache,
      last_computed_date:(.lastComputedDate // null),
      last_daily_activity:(.dailyActivity[-1] // null),
      model_usage:(.modelUsage // null)
    }' "$cache" 2>/dev/null || jq -n \
      --arg profile "$profile" \
      --arg harness "claude" \
      --arg status "error" \
      --arg home "$home" \
      --arg cache "$cache" \
      '{profile:$profile,harness:$harness,status:$status,home:$home,cache:$cache}'
}

collect_codex() {
  local profile="$1"
  local n="${profile#codex}"
  local home="$HOME/.codex-$n"
  local log_path="$home/log/codex-tui.log"
  local login_status=""
  local latest_line=""

  if [[ ! -d "$home" ]]; then
    jq -n \
      --arg profile "$profile" \
      --arg harness "codex" \
      --arg status "missing-home" \
      --arg home "$home" \
      '{profile:$profile,harness:$harness,status:$status,home:$home}'
    return 0
  fi

  login_status="$(CODEX_HOME="$home" codex login status 2>&1 || true)"
  latest_line="$(rg -n "post sampling token usage" -S "$log_path" 2>/dev/null | tail -n 1 || true)"

  local total_usage_tokens=""
  local estimated_tokens=""
  local log_timestamp=""
  local thread_id=""

  if [[ -n "$latest_line" ]]; then
    total_usage_tokens="$(printf '%s\n' "$latest_line" | sed -n 's/.*total_usage_tokens=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
    estimated_tokens="$(printf '%s\n' "$latest_line" | sed -n 's/.*estimated_token_count=Some(\([0-9][0-9]*\)).*/\1/p' | head -n 1)"
    log_timestamp="$(printf '%s\n' "$latest_line" | sed -n -E 's/^[0-9]+:([0-9T:.-]+Z).*/\1/p' | head -n 1)"
    thread_id="$(printf '%s\n' "$latest_line" | sed -n -E 's/.*thread_id=([0-9a-fA-F-]+).*/\1/p' | head -n 1)"
  fi

  jq -n \
    --arg profile "$profile" \
    --arg harness "codex" \
    --arg status "ok" \
    --arg home "$home" \
    --arg log_path "$log_path" \
    --arg login_status "$login_status" \
    --arg latest_line "$latest_line" \
    --arg total_usage_tokens "$total_usage_tokens" \
    --arg estimated_tokens "$estimated_tokens" \
    --arg log_timestamp "$log_timestamp" \
    --arg thread_id "$thread_id" \
    '{
      profile:$profile,
      harness:$harness,
      status:$status,
      home:$home,
      login_status:$login_status,
      latest_usage:{
        log_path:$log_path,
        timestamp:(if $log_timestamp=="" then null else $log_timestamp end),
        thread_id:(if $thread_id=="" then null else $thread_id end),
        total_usage_tokens:(if $total_usage_tokens=="" then null else ($total_usage_tokens|tonumber) end),
        estimated_tokens:(if $estimated_tokens=="" then null else ($estimated_tokens|tonumber) end),
        raw_line:(if $latest_line=="" then null else $latest_line end)
      }
    }'
}

collect_pi() {
  local profile="$1"
  jq -n \
    --arg profile "$profile" \
    --arg harness "pi" \
    --arg status "unsupported" \
    --arg note "No stable native usage/quota command found in pi CLI yet." \
    '{profile:$profile,harness:$harness,status:$status,note:$note}'
}

collect_profile() {
  local profile="$1"
  local days="$2"
  case "$profile" in
    oc[0-9]*) collect_opencode "$profile" "$days" ;;
    cc[0-9]*) collect_claude "$profile" ;;
    codex[0-9]*) collect_codex "$profile" ;;
    pi|pi[0-9]*) collect_pi "$profile" ;;
    *)
      jq -n \
        --arg profile "$profile" \
        --arg status "unknown-profile" \
        '{profile:$profile,status:$status}'
      ;;
  esac
}

capture_once() {
  local out_dir="$1"
  local profiles_csv="$2"
  local days="$3"
  mkdir -p "$out_dir"

  local ts
  ts="$(now_iso)"

  local tmpdir
  tmpdir=$(mktemp -d)
  trap 'rm -rf "$tmpdir"' RETURN

  IFS=',' read -r -a profiles <<<"$profiles_csv"
  local idx=0
  local profile=""
  for profile in "${profiles[@]}"; do
    profile="$(trim "$profile")"
    if [[ -z "$profile" ]]; then
      continue
    fi
    collect_profile "$profile" "$days" > "$tmpdir/${idx}.json"
    idx=$((idx + 1))
  done

  if [[ "$idx" -eq 0 ]]; then
    err "no profiles specified"
    exit 2
  fi

  jq -cs \
    --arg ts "$ts" \
    '{
      timestamp:$ts,
      profiles:.
    }' "$tmpdir"/*.json > "$out_dir/latest.json"

  jq -c '.' "$out_dir/latest.json" >> "$out_dir/snapshots.jsonl"

  echo "captured: $ts"
  echo "output: $out_dir/latest.json"
}

run_daemon() {
  local out_dir="$1"
  local profiles_csv="$2"
  local interval="$3"
  local days="$4"

  while true; do
    capture_once "$out_dir" "$profiles_csv" "$days" || true
    sleep "$interval"
  done
}

cmd="${1:-}"
if [[ -z "$cmd" ]]; then
  usage
  exit 1
fi
shift || true

out_dir="$DEFAULT_OUT_DIR"
profiles_csv="$DEFAULT_PROFILES"
interval="$DEFAULT_INTERVAL"
days="$DEFAULT_DAYS"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      out_dir="${2:-}"
      shift 2
      ;;
    --profiles)
      profiles_csv="${2:-}"
      shift 2
      ;;
    --interval)
      interval="${2:-}"
      shift 2
      ;;
    --days)
      days="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      err "unknown option: $1"
      usage
      exit 2
      ;;
  esac
done

case "$cmd" in
  run-once)
    capture_once "$out_dir" "$profiles_csv" "$days"
    ;;
  start)
    ensure_state_dir
    if is_running; then
      echo "capture loop already running (pid $(cat "$PID_FILE"))"
      exit 0
    fi
    nohup "$0" _daemon --out-dir "$out_dir" --profiles "$profiles_csv" --interval "$interval" --days "$days" >>"$LOG_FILE" 2>&1 &
    echo "$!" > "$PID_FILE"
    echo "capture loop started (pid $(cat "$PID_FILE"))"
    echo "log: $LOG_FILE"
    ;;
  _daemon)
    run_daemon "$out_dir" "$profiles_csv" "$interval" "$days"
    ;;
  stop)
    if ! is_running; then
      echo "capture loop not running"
      rm -f "$PID_FILE"
      exit 0
    fi
    pid="$(cat "$PID_FILE")"
    kill "$pid" 2>/dev/null || true
    sleep 1
    if kill -0 "$pid" 2>/dev/null; then
      kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$PID_FILE"
    echo "capture loop stopped"
    ;;
  status)
    ensure_state_dir
    if is_running; then
      echo "running (pid $(cat "$PID_FILE"))"
    else
      echo "not running"
    fi
    echo "state_dir: $STATE_DIR"
    echo "log_file: $LOG_FILE"
    echo "out_dir: $out_dir"
    ;;
  tail)
    ensure_state_dir
    touch "$LOG_FILE"
    tail -f "$LOG_FILE"
    ;;
  *)
    err "unknown command: $cmd"
    usage
    exit 2
    ;;
esac
