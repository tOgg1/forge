#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/rust-baseline-refresh.sh --approval <ref> [--dry-run|--apply] [--allow-drift] [--out-dir <dir>]

Modes:
  --dry-run     Generate snapshot and check against committed baseline (default)
  --apply       Generate refreshed snapshot artifacts without baseline diff check

Approval:
  --approval    Required. Accepts forge task id, PR-id (PR-123), or pull URL.

Options:
  --allow-drift In dry-run mode, return success even when drift is detected
  --out-dir     Snapshot output directory (default: build/rust-baseline/refresh)
EOF
}

json_escape() {
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

approval_ref=""
mode="dry-run"
allow_drift=0
out_dir="build/rust-baseline/refresh"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --approval)
      approval_ref="${2:-}"
      shift 2
      ;;
    --approval=*)
      approval_ref="${1#*=}"
      shift
      ;;
    --dry-run)
      mode="dry-run"
      shift
      ;;
    --apply)
      mode="apply"
      shift
      ;;
    --allow-drift)
      allow_drift=1
      shift
      ;;
    --out-dir)
      out_dir="${2:-}"
      shift 2
      ;;
    --out-dir=*)
      out_dir="${1#*=}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$approval_ref" ]]; then
  echo "missing required --approval" >&2
  exit 2
fi

if [[ ! "$approval_ref" =~ ^(forge-[a-z0-9]+|[Pp][Rr]-[0-9]+|https://github\.com/[^/]+/[^/]+/pull/[0-9]+)$ ]]; then
  echo "invalid approval reference: $approval_ref" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

snapshot_bin="${RUST_BASELINE_SNAPSHOT_BIN:-scripts/rust-baseline-snapshot.sh}"
if [[ "$snapshot_bin" != /* ]]; then
  snapshot_bin="$repo_root/$snapshot_bin"
fi

if [[ ! -f "$snapshot_bin" ]]; then
  echo "snapshot command not found: $snapshot_bin" >&2
  exit 1
fi

mkdir -p "$out_dir"
requested_by="${BASELINE_REFRESH_REQUESTED_BY:-${GITHUB_ACTOR:-manual}}"
generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

snapshot_status=0
if [[ "$mode" == "apply" ]]; then
  "$snapshot_bin" "$out_dir"
else
  if "$snapshot_bin" "$out_dir" --check; then
    snapshot_status=0
  else
    snapshot_status=$?
  fi
fi

drift_detected=false
if [[ "$mode" == "dry-run" && "$snapshot_status" -ne 0 ]]; then
  drift_detected=true
fi

report_path="$out_dir/baseline-refresh-report.json"
cat >"$report_path" <<EOF
{
  "protocol_version": "v1",
  "approval_ref": "$(json_escape "$approval_ref")",
  "requested_by": "$(json_escape "$requested_by")",
  "mode": "$mode",
  "allow_drift": $([[ "$allow_drift" -eq 1 ]] && echo true || echo false),
  "drift_detected": $drift_detected,
  "snapshot_dir": "$(json_escape "$out_dir")",
  "generated_at": "$generated_at"
}
EOF

echo "baseline refresh report: $report_path"

if [[ "$mode" == "dry-run" && "$drift_detected" == "true" && "$allow_drift" -eq 0 ]]; then
  exit "$snapshot_status"
fi

if [[ "$mode" == "apply" && "$snapshot_status" -ne 0 ]]; then
  exit "$snapshot_status"
fi

exit 0
