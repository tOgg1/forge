#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root/rust"

threshold_file="${1:-coverage-thresholds.txt}"
waiver_file="${2:-coverage-waivers.txt}"

tmp_threshold_crates="$(mktemp)"
tmp_waivers="$(mktemp)"
cleanup() {
  rm -f "$tmp_threshold_crates" "$tmp_waivers"
}
trap cleanup EXIT

trim() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s' "$value"
}

if [[ ! -f "$threshold_file" ]]; then
  echo "threshold file not found: $threshold_file" >&2
  exit 1
fi

if [[ ! -f "$waiver_file" ]]; then
  echo "waiver file not found: $waiver_file" >&2
  exit 1
fi

today="$(date -u +%F)"

mkdir -p coverage
per_crate_summary_path="coverage/per-crate-summary.txt"
: > "$per_crate_summary_path"

while IFS= read -r raw_line || [[ -n "${raw_line:-}" ]]; do
  line="$(trim "$raw_line")"
  [[ -z "$line" ]] && continue
  [[ "$line" =~ ^# ]] && continue

  IFS='|' read -r raw_crate raw_expires raw_approved raw_issue raw_reason raw_extra <<< "$raw_line"
  crate="$(trim "${raw_crate:-}")"
  expires_on="$(trim "${raw_expires:-}")"
  approved_by="$(trim "${raw_approved:-}")"
  issue_ref="$(trim "${raw_issue:-}")"
  reason="$(trim "${raw_reason:-}")"
  extra="$(trim "${raw_extra:-}")"

  if [[ -z "$crate" || -z "$expires_on" || -z "$approved_by" || -z "$issue_ref" || -z "$reason" ]]; then
    echo "invalid waiver row (expected crate|expires_on|approved_by|issue|reason): $raw_line" >&2
    exit 1
  fi
  if [[ -n "$extra" ]]; then
    echo "invalid waiver row (too many fields): $raw_line" >&2
    exit 1
  fi
  if [[ ! "$expires_on" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
    echo "invalid waiver expiry date for $crate: $expires_on (expected YYYY-MM-DD)" >&2
    exit 1
  fi
  if [[ "$expires_on" < "$today" ]]; then
    echo "expired waiver for $crate: $expires_on (today: $today)" >&2
    exit 1
  fi

  printf '%s\t%s\t%s\t%s\t%s\n' "$crate" "$expires_on" "$approved_by" "$issue_ref" "$reason" >> "$tmp_waivers"
done < "$waiver_file"

if [[ -s "$tmp_waivers" ]]; then
  duplicates="$(cut -f1 "$tmp_waivers" | sort | uniq -d || true)"
  if [[ -n "$duplicates" ]]; then
    echo "duplicate waiver rows for crate(s): $duplicates" >&2
    exit 1
  fi
fi

while IFS= read -r raw_line || [[ -n "${raw_line:-}" ]]; do
  line="$(trim "$raw_line")"
  [[ -z "$line" ]] && continue
  [[ "$line" =~ ^# ]] && continue

  IFS=' ' read -r crate threshold extra <<< "$line"
  crate="$(trim "${crate:-}")"
  threshold="$(trim "${threshold:-}")"
  extra="$(trim "${extra:-}")"
  if [[ -z "$crate" || -z "$threshold" || -n "$extra" ]]; then
    echo "invalid threshold row (expected crate threshold): $raw_line" >&2
    exit 1
  fi
  if [[ ! "$threshold" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    echo "invalid threshold for $crate: $threshold" >&2
    exit 1
  fi
  printf '%s\n' "$crate" >> "$tmp_threshold_crates"

  waiver_row="$(awk -F'\t' -v crate="$crate" '$1==crate {print; exit}' "$tmp_waivers")"
  if [[ -n "$waiver_row" ]]; then
    IFS=$'\t' read -r _ expires_on approved_by issue_ref reason <<< "$waiver_row"
    echo "waiving $crate >= ${threshold}% until $expires_on (approved_by=$approved_by issue=$issue_ref)"
    {
      echo "crate=$crate (WAIVED until $expires_on; approved_by=$approved_by; issue=$issue_ref)"
      cargo llvm-cov --package "$crate" --summary-only
      echo ""
    } | tee -a "$per_crate_summary_path"
    continue
  fi

  echo "enforcing $crate >= ${threshold}% line coverage"
  {
    echo "crate=$crate"
    cargo llvm-cov --package "$crate" --summary-only --fail-under-lines "$threshold"
    echo ""
  } | tee -a "$per_crate_summary_path"
done < "$threshold_file"

if [[ -s "$tmp_waivers" ]]; then
  unknown_waiver_crates="$(comm -23 <(cut -f1 "$tmp_waivers" | sort -u) <(sort -u "$tmp_threshold_crates") || true)"
  if [[ -n "$unknown_waiver_crates" ]]; then
    echo "waiver references unknown crate(s):" >&2
    echo "$unknown_waiver_crates" >&2
    exit 1
  fi
fi
