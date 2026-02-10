#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root/rust"

threshold_file="${1:-coverage-thresholds.txt}"
waiver_file="${2:-coverage-waivers.txt}"
lcov_path="coverage/lcov.info"

tmp_threshold_crates="$(mktemp)"
tmp_threshold_rows="$(mktemp)"
tmp_waivers="$(mktemp)"
tmp_lcov_index="$(mktemp)"
tmp_modified_files="$(mktemp)"
cleanup() {
  rm -f "$tmp_threshold_crates" "$tmp_threshold_rows" "$tmp_waivers" "$tmp_lcov_index" "$tmp_modified_files"
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

diff_base="${RUST_COVERAGE_DIFF_BASE:-}"
if [[ -z "${diff_base}" ]]; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    diff_base="origin/${GITHUB_BASE_REF}"
  else
    diff_base="origin/main"
  fi
fi

if ! git -C "$repo_root" rev-parse --verify "${diff_base}^{commit}" >/dev/null 2>&1; then
  echo "cannot resolve diff base ${diff_base}; set RUST_COVERAGE_DIFF_BASE or fetch base ref" >&2
  exit 1
fi

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
  printf '%s\t%s\n' "$crate" "$threshold" >> "$tmp_threshold_rows"
done < "$threshold_file"

if [[ ! -f "$lcov_path" ]]; then
  echo "lcov file not found at $lcov_path; generating via cargo llvm-cov" >&2
  cargo llvm-cov --workspace --all-features --lcov --output-path "$lcov_path"
fi

if [[ ! -f "$lcov_path" ]]; then
  echo "lcov file not found after generation: $lcov_path" >&2
  exit 1
fi

awk -F: '
  /^SF:/ { sf=substr($0,4); lf=""; lh=""; next }
  /^LF:/ { lf=$2; next }
  /^LH:/ { lh=$2; next }
  /^end_of_record/ {
    if (sf != "") {
      if (lf == "") lf=0
      if (lh == "") lh=0
      printf "%s\t%s\t%s\n", sf, lh, lf
    }
    sf=""
  }
' "$lcov_path" > "$tmp_lcov_index"

while IFS= read -r path; do
  if [[ "$path" =~ ^rust/crates/([^/]+)/src/.+\\.rs$ ]]; then
    crate="${BASH_REMATCH[1]}"
    abs="$repo_root/$path"
    printf '%s\t%s\n' "$crate" "$abs" >> "$tmp_modified_files"
  fi
done < <(git -C "$repo_root" diff --name-only "${diff_base}...HEAD" -- rust/crates)

file_pct() {
  local abs="$1"
  local row lh lf pct
  row="$(awk -F'\t' -v f="$abs" '$1==f {print $2 "\t" $3; found=1; exit} END { if (!found) exit 1 }' "$tmp_lcov_index")" || return 1
  lh="$(printf '%s' "$row" | cut -f1)"
  lf="$(printf '%s' "$row" | cut -f2)"
  pct="$(awk -v lh="$lh" -v lf="$lf" 'BEGIN{ if (lf+0 <= 0) { printf "%.2f", 100.0; exit } printf "%.2f", (lh/lf)*100.0 }')"
  printf '%s\t%s\t%s' "$pct" "$lh" "$lf"
}

while IFS=$'\t' read -r crate threshold; do
  waiver_row="$(awk -F'\t' -v crate="$crate" '$1==crate {print; exit}' "$tmp_waivers")"
  if [[ -n "$waiver_row" ]]; then
    IFS=$'\t' read -r _ expires_on approved_by issue_ref reason <<< "$waiver_row"
    echo "waiving $crate >= ${threshold}% until $expires_on (approved_by=$approved_by issue=$issue_ref)"
    {
      echo "crate=$crate (WAIVED until $expires_on; approved_by=$approved_by; issue=$issue_ref)"
      echo ""
    } | tee -a "$per_crate_summary_path"
    continue
  fi

  files="$(awk -F'\t' -v c="$crate" '$1==c {print $2}' "$tmp_modified_files" || true)"
  if [[ -z "$(trim "${files:-}")" ]]; then
    echo "skipping $crate: no modified rust source files in rust/crates/$crate/src/"
    {
      echo "crate=$crate (SKIP: no modified files)"
      echo ""
    } | tee -a "$per_crate_summary_path"
    continue
  fi

  echo "enforcing $crate >= ${threshold}% line coverage on modified file(s)"
  {
    echo "crate=$crate"
    while IFS= read -r abs; do
      abs="$(trim "$abs")"
      [[ -z "$abs" ]] && continue
      if ! got="$(file_pct "$abs")"; then
        echo "missing coverage data for file: $abs" >&2
        exit 1
      fi
      pct="$(printf '%s' "$got" | cut -f1)"
      lh="$(printf '%s' "$got" | cut -f2)"
      lf="$(printf '%s' "$got" | cut -f3)"
      printf 'file=%s lines=%s/%s pct=%s%% threshold=%s%%\n' "$abs" "$lh" "$lf" "$pct" "$threshold"
      if awk -v pct="$pct" -v thr="$threshold" 'BEGIN{ exit !(pct+0 < thr+0) }'; then
        echo "coverage below threshold for $crate: $abs is ${pct}% (< ${threshold}%)" >&2
        exit 1
      fi
done <<< "$files"
    echo ""
  } | tee -a "$per_crate_summary_path"
done < "$tmp_threshold_rows"

if [[ -s "$tmp_waivers" ]]; then
  unknown_waiver_crates="$(comm -23 <(cut -f1 "$tmp_waivers" | sort -u) <(sort -u "$tmp_threshold_crates") || true)"
  if [[ -n "$unknown_waiver_crates" ]]; then
    echo "waiver references unknown crate(s):" >&2
    echo "$unknown_waiver_crates" >&2
    exit 1
  fi
fi
