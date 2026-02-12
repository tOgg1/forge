#!/usr/bin/env bash
set -euo pipefail

DEFAULT_PROFILES="cc1,cc2,cc3,codex1,codex2,codex3"
DEFAULT_STARTUP_WAIT=12
DEFAULT_RENDER_WAIT=6

usage() {
  cat <<'USAGE'
Usage: scripts/harness-quota-endpoint.sh [options]

Options:
  --profiles CSV        Profiles to query (default: cc1,cc2,cc3,codex1,codex2,codex3)
  --json                Print full JSON snapshot (default)
  --text                Print compact text summary
  --out PATH            Write JSON snapshot to file
  --startup-wait SEC    Wait before slash command (default: 12)
  --render-wait SEC     Wait after slash command (default: 6)
  -h, --help            Show help

Notes:
  - Claude: pulls "Current session" and "Current week (all models)" from /usage.
    "Current session" is the rolling 5h window.
  - Codex: pulls "5h limit" and "Weekly limit" from /status.
USAGE
}

trim() {
  local s="$1"
  s="${s#"${s%%[![:space:]]*}"}"
  s="${s%"${s##*[![:space:]]}"}"
  printf '%s' "$s"
}

extract_used_pct() {
  local line="$1"
  printf '%s\n' "$line" \
    | grep -Eo '[0-9]{1,3}%used' \
    | head -n 1 \
    | grep -Eo '^[0-9]+' || true
}

extract_left_pct() {
  local line="$1"
  printf '%s\n' "$line" \
    | grep -Eo '[0-9]{1,3}%[[:space:]]*left' \
    | head -n 1 \
    | grep -Eo '^[0-9]+' || true
}

extract_reset_paren() {
  local line="$1"
  printf '%s\n' "$line" | sed -n -E 's/.*\(resets[[:space:]]*([^)]*)\).*/\1/p' | head -n 1
}

extract_reset_loose() {
  local line="$1"
  printf '%s\n' "$line" | sed -n -E 's/.*Rese(ts|s)//p' | head -n 1
}

clean_capture() {
  local in="$1"
  local out="$2"
  perl -pe 's/\e\[[0-9;?]*[A-Za-z]//g; s/\r//g; s/[^\x09\x0A\x20-\x7E]//g' "$in" > "$out"
}

capture_with_tmux() {
  local session="$1"
  local cmd="$2"
  local slash="$3"
  local startup_wait="$4"
  local render_wait="$5"
  local out_file="$6"
  local exit_slash="$7"

  tmux new-session -d -s "$session" "zsh -lc '$cmd'"
  sleep 1
  if ! tmux has-session -t "$session" 2>/dev/null; then
    return 1
  fi

  tmux pipe-pane -t "$session":0.0 -o "cat >> '$out_file'"
  sleep "$startup_wait"
  tmux send-keys -t "$session":0.0 "$slash" C-m
  if [[ "$slash" == "/usage" ]]; then
    sleep 1
    tmux send-keys -t "$session":0.0 C-m
  fi
  sleep "$render_wait"
  tmux send-keys -t "$session":0.0 "$exit_slash" C-m
  sleep 2
  tmux kill-session -t "$session" 2>/dev/null || true
  return 0
}

collect_codex() {
  local profile="$1"
  local startup_wait="$2"
  local render_wait="$3"
  local n="${profile#codex}"
  local home="$HOME/.codex-$n"

  if [[ ! -d "$home" ]]; then
    jq -n --arg p "$profile" --arg h "codex" --arg s "missing-home" --arg home "$home" \
      '{profile:$p,harness:$h,status:$s,home:$home}'
    return 0
  fi

  local raw clean session
  raw=$(mktemp)
  clean=$(mktemp)
  session="quota-${profile}-$(date +%s)-$RANDOM"

  if ! capture_with_tmux \
    "$session" \
    "CODEX_HOME=$home codex --dangerously-bypass-approvals-and-sandbox" \
    "/status" \
    "$startup_wait" \
    "$render_wait" \
    "$raw" \
    "/quit"; then
    rm -f "$raw" "$clean"
    jq -n --arg p "$profile" --arg h "codex" --arg s "spawn-failed" --arg home "$home" \
      '{profile:$p,harness:$h,status:$s,home:$home}'
    return 0
  fi

  clean_capture "$raw" "$clean"

  local account_line five_line weekly_line weekly_reset_line context_line session_line
  account_line="$(rg -m1 "Account:" -S "$clean" || true)"
  five_line="$(rg -m1 "5h limit:" -S "$clean" || true)"
  weekly_line="$(rg -m1 "Weekly limit:" -S "$clean" || true)"
  context_line="$(rg -m1 "Context window:" -S "$clean" || true)"
  session_line="$(rg -m1 "Session:" -S "$clean" || true)"

  local weekly_ln
  weekly_ln="$(rg -n -m1 "Weekly limit:" -S "$clean" | cut -d: -f1 || true)"
  if [[ -n "$weekly_ln" ]]; then
    weekly_reset_line="$(sed -n "$((weekly_ln + 1))p" "$clean" || true)"
  else
    weekly_reset_line=""
  fi

  local account context_left five_left weekly_left five_reset weekly_reset session_id
  account="$(trim "$(printf '%s\n' "$account_line" | sed -n -E 's/.*Account:[[:space:]]*(.*)/\1/p' | head -n 1)")"
  context_left="$(extract_left_pct "$context_line")"
  five_left="$(extract_left_pct "$five_line")"
  weekly_left="$(extract_left_pct "$weekly_line")"
  five_reset="$(trim "$(extract_reset_paren "$five_line")")"
  weekly_reset="$(trim "$(extract_reset_paren "$weekly_line")")"
  if [[ -z "$weekly_reset" ]]; then
    weekly_reset="$(trim "$(extract_reset_paren "$weekly_reset_line")")"
  fi
  session_id="$(trim "$(printf '%s\n' "$session_line" | sed -n -E 's/.*Session:[[:space:]]*([0-9a-fA-F-]+).*/\1/p' | head -n 1)")"

  rm -f "$raw" "$clean"

  jq -n \
    --arg p "$profile" \
    --arg h "codex" \
    --arg s "ok" \
    --arg home "$home" \
    --arg account "$account" \
    --arg session_id "$session_id" \
    --arg context_left "$context_left" \
    --arg five_left "$five_left" \
    --arg weekly_left "$weekly_left" \
    --arg five_reset "$five_reset" \
    --arg weekly_reset "$weekly_reset" \
    '{
      profile:$p,
      harness:$h,
      status:$s,
      home:$home,
      account:(if $account=="" then null else $account end),
      session_id:(if $session_id=="" then null else $session_id end),
      context_window:{remaining_pct:(if $context_left=="" then null else ($context_left|tonumber) end)},
      five_hour:{remaining_pct:(if $five_left=="" then null else ($five_left|tonumber) end),resets:(if $five_reset=="" then null else $five_reset end)},
      weekly:{remaining_pct:(if $weekly_left=="" then null else ($weekly_left|tonumber) end),resets:(if $weekly_reset=="" then null else $weekly_reset end)}
    }'
}

collect_claude() {
  local profile="$1"
  local startup_wait="$2"
  local render_wait="$3"
  local n="${profile#cc}"
  local home="$HOME/.claude-$n"

  if [[ ! -d "$home" ]]; then
    jq -n --arg p "$profile" --arg h "claude" --arg s "missing-home" --arg home "$home" \
      '{profile:$p,harness:$h,status:$s,home:$home}'
    return 0
  fi

  local raw clean session
  raw=$(mktemp)
  clean=$(mktemp)
  session="quota-${profile}-$(date +%s)-$RANDOM"

  if ! capture_with_tmux \
    "$session" \
    "CLAUDE_CONFIG_DIR=$home claude --dangerously-skip-permissions" \
    "/usage" \
    "$startup_wait" \
    "$render_wait" \
    "$raw" \
    "/exit"; then
    rm -f "$raw" "$clean"
    jq -n --arg p "$profile" --arg h "claude" --arg s "spawn-failed" --arg home "$home" \
      '{profile:$p,harness:$h,status:$s,home:$home}'
    return 0
  fi

  clean_capture "$raw" "$clean"

  local session_line week_marker week_used_line week_reset_line sonnet_marker sonnet_used_line sonnet_reset_line extra_marker extra_next_line extra_spent_line
  session_line="$(rg -m1 "Current session" -S "$clean" || true)"

  local week_ln sonnet_ln extra_ln
  week_ln="$(rg -n -m1 "Currentweek\\(allmodels\\)" -S "$clean" | cut -d: -f1 || true)"
  sonnet_ln="$(rg -n -m1 "Currentweek\\(Sonnetonly\\)" -S "$clean" | cut -d: -f1 || true)"
  extra_ln="$(rg -n -m1 "Extrausage" -S "$clean" | cut -d: -f1 || true)"

  if [[ -n "$week_ln" ]]; then
    week_marker="$(sed -n "${week_ln}p" "$clean" || true)"
    week_used_line="$(sed -n "$((week_ln + 1))p" "$clean" || true)"
    week_reset_line="$(sed -n "$((week_ln + 2))p" "$clean" || true)"
  else
    week_marker=""; week_used_line=""; week_reset_line=""
  fi

  if [[ -n "$sonnet_ln" ]]; then
    sonnet_marker="$(sed -n "${sonnet_ln}p" "$clean" || true)"
    sonnet_used_line="$(sed -n "$((sonnet_ln + 1))p" "$clean" || true)"
    sonnet_reset_line="$(sed -n "$((sonnet_ln + 2))p" "$clean" || true)"
  else
    sonnet_marker=""; sonnet_used_line=""; sonnet_reset_line=""
  fi

  if [[ -n "$extra_ln" ]]; then
    extra_marker="$(sed -n "${extra_ln}p" "$clean" || true)"
    extra_next_line="$(sed -n "$((extra_ln + 1))p" "$clean" || true)"
    extra_spent_line="$(sed -n "$((extra_ln + 2))p" "$clean" || true)"
  else
    extra_marker=""; extra_next_line=""; extra_spent_line=""
  fi

  local session_used week_used sonnet_used extra_used
  local session_reset week_reset sonnet_reset extra_reset
  local extra_spent extra_enabled

  session_used="$(extract_used_pct "$session_line")"
  week_used="$(extract_used_pct "$week_used_line")"
  sonnet_used="$(extract_used_pct "$sonnet_used_line")"
  extra_used="$(extract_used_pct "$extra_next_line")"

  session_reset="$(trim "$(extract_reset_loose "$session_line")")"
  week_reset="$(trim "$(extract_reset_loose "$week_reset_line")")"
  sonnet_reset="$(trim "$(extract_reset_loose "$sonnet_reset_line")")"
  extra_reset="$(trim "$(extract_reset_loose "$extra_spent_line")")"

  extra_spent="$(printf '%s\n' "$extra_spent_line" | sed -n -E 's/.*(\$[0-9]+\.[0-9]+\/\$[0-9]+\.[0-9]+spent).*/\1/p' | head -n 1)"
  if printf '%s%s\n' "$extra_next_line" "$extra_spent_line" | rg -qi "notenabled"; then
    extra_enabled="false"
  else
    extra_enabled="true"
  fi

  rm -f "$raw" "$clean"

  jq -n \
    --arg p "$profile" \
    --arg h "claude" \
    --arg s "ok" \
    --arg home "$home" \
    --arg session_used "$session_used" \
    --arg session_reset "$session_reset" \
    --arg week_used "$week_used" \
    --arg week_reset "$week_reset" \
    --arg sonnet_used "$sonnet_used" \
    --arg sonnet_reset "$sonnet_reset" \
    --arg extra_used "$extra_used" \
    --arg extra_reset "$extra_reset" \
    --arg extra_spent "$extra_spent" \
    --arg extra_enabled "$extra_enabled" \
    '{
      profile:$p,
      harness:$h,
      status:$s,
      home:$home,
      session:{
        used_pct:(if $session_used=="" then null else ($session_used|tonumber) end),
        remaining_pct:(if $session_used=="" then null else (100-($session_used|tonumber)) end),
        resets:(if $session_reset=="" then null else $session_reset end)
      },
      weekly_all_models:{
        used_pct:(if $week_used=="" then null else ($week_used|tonumber) end),
        remaining_pct:(if $week_used=="" then null else (100-($week_used|tonumber)) end),
        resets:(if $week_reset=="" then null else $week_reset end)
      },
      weekly_sonnet:{
        used_pct:(if $sonnet_used=="" then null else ($sonnet_used|tonumber) end),
        remaining_pct:(if $sonnet_used=="" then null else (100-($sonnet_used|tonumber)) end),
        resets:(if $sonnet_reset=="" then null else $sonnet_reset end)
      },
      extra_usage:{
        enabled:($extra_enabled=="true"),
        used_pct:(if $extra_used=="" then null else ($extra_used|tonumber) end),
        spent:(if $extra_spent=="" then null else $extra_spent end),
        resets:(if $extra_reset=="" then null else $extra_reset end)
      }
    }'
}

collect_profile() {
  local profile="$1"
  local startup_wait="$2"
  local render_wait="$3"
  case "$profile" in
    codex[0-9]*) collect_codex "$profile" "$startup_wait" "$render_wait" ;;
    cc[0-9]*) collect_claude "$profile" "$startup_wait" "$render_wait" ;;
    *)
      jq -n --arg p "$profile" --arg s "unsupported-profile" '{profile:$p,status:$s}'
      ;;
  esac
}

build_snapshot() {
  local profiles_csv="$1"
  local startup_wait="$2"
  local render_wait="$3"

  local tmpdir
  tmpdir=$(mktemp -d)
  trap 'rm -rf "$tmpdir"' RETURN

  IFS=',' read -r -a profiles <<<"$profiles_csv"
  local idx=0
  local profile
  for profile in "${profiles[@]}"; do
    profile="$(trim "$profile")"
    if [[ -z "$profile" ]]; then
      continue
    fi
    collect_profile "$profile" "$startup_wait" "$render_wait" > "$tmpdir/${idx}.json"
    idx=$((idx + 1))
  done

  if [[ "$idx" -eq 0 ]]; then
    echo "error: no profiles" >&2
    exit 2
  fi

  local ts
  ts="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

  jq -cs --arg ts "$ts" '
    def avg($arr): if ($arr|length)==0 then null else (($arr|add)/($arr|length)) end;
    . as $p
    | {
        timestamp:$ts,
        profiles:$p,
        summary:{
          codex:{
            profiles: ($p | map(select(.harness=="codex" and .status=="ok") | .profile)),
            five_hour_remaining_pct:{
              min: (($p | map(select(.harness=="codex" and .status=="ok" and .five_hour.remaining_pct!=null) | .five_hour.remaining_pct) | min) // null),
              max: (($p | map(select(.harness=="codex" and .status=="ok" and .five_hour.remaining_pct!=null) | .five_hour.remaining_pct) | max) // null),
              avg: avg(($p | map(select(.harness=="codex" and .status=="ok" and .five_hour.remaining_pct!=null) | .five_hour.remaining_pct)))
            },
            weekly_remaining_pct:{
              min: (($p | map(select(.harness=="codex" and .status=="ok" and .weekly.remaining_pct!=null) | .weekly.remaining_pct) | min) // null),
              max: (($p | map(select(.harness=="codex" and .status=="ok" and .weekly.remaining_pct!=null) | .weekly.remaining_pct) | max) // null),
              avg: avg(($p | map(select(.harness=="codex" and .status=="ok" and .weekly.remaining_pct!=null) | .weekly.remaining_pct)))
            }
          },
          claude:{
            profiles: ($p | map(select(.harness=="claude" and .status=="ok") | .profile)),
            session_remaining_pct:{
              min: (($p | map(select(.harness=="claude" and .status=="ok" and .session.remaining_pct!=null) | .session.remaining_pct) | min) // null),
              max: (($p | map(select(.harness=="claude" and .status=="ok" and .session.remaining_pct!=null) | .session.remaining_pct) | max) // null),
              avg: avg(($p | map(select(.harness=="claude" and .status=="ok" and .session.remaining_pct!=null) | .session.remaining_pct)))
            },
            weekly_remaining_pct:{
              min: (($p | map(select(.harness=="claude" and .status=="ok" and .weekly_all_models.remaining_pct!=null) | .weekly_all_models.remaining_pct) | min) // null),
              max: (($p | map(select(.harness=="claude" and .status=="ok" and .weekly_all_models.remaining_pct!=null) | .weekly_all_models.remaining_pct) | max) // null),
              avg: avg(($p | map(select(.harness=="claude" and .status=="ok" and .weekly_all_models.remaining_pct!=null) | .weekly_all_models.remaining_pct)))
            }
          }
        }
      }' "$tmpdir"/*.json
}

profiles_csv="$DEFAULT_PROFILES"
startup_wait="$DEFAULT_STARTUP_WAIT"
render_wait="$DEFAULT_RENDER_WAIT"
out_path=""
mode="json"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profiles) profiles_csv="${2:-}"; shift 2 ;;
    --startup-wait) startup_wait="${2:-}"; shift 2 ;;
    --render-wait) render_wait="${2:-}"; shift 2 ;;
    --out) out_path="${2:-}"; shift 2 ;;
    --json) mode="json"; shift ;;
    --text) mode="text"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unknown option $1" >&2; usage; exit 2 ;;
  esac
done

if ! command -v tmux >/dev/null 2>&1; then
  echo "error: tmux not found" >&2
  exit 2
fi

snapshot="$(build_snapshot "$profiles_csv" "$startup_wait" "$render_wait")"

if [[ -n "$out_path" ]]; then
  mkdir -p "$(dirname "$out_path")"
  printf '%s\n' "$snapshot" > "$out_path"
fi

if [[ "$mode" == "json" ]]; then
  printf '%s\n' "$snapshot"
else
  printf '%s\n' "$snapshot" \
    | jq -r '
      "timestamp: \(.timestamp)",
      "codex 5h remaining avg: \(.summary.codex.five_hour_remaining_pct.avg // \"n/a\")%",
      "codex weekly remaining avg: \(.summary.codex.weekly_remaining_pct.avg // \"n/a\")%",
      "claude session(5h) remaining avg: \(.summary.claude.session_remaining_pct.avg // \"n/a\")%",
      "claude weekly remaining avg: \(.summary.claude.weekly_remaining_pct.avg // \"n/a\")%",
      "",
      "profiles:",
      (.profiles[] | "  \(.profile) \(.harness) \(.status)")
    '
fi
