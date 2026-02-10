#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: scripts/rust-parity-benchmark-pack.sh \
  --go-bin <path> \
  --rust-bin <path> \
  [--workdir <path>] \
  [--runs <n>] \
  [--budget-ratio <float>] \
  [--out-dir <dir>] \
  [--command "<args>"]...

Default command pack:
  "ps --json"
  "status --json"
  "tui --json"

Exit code:
  0 when all commands execute and Rust/Go ratio <= budget for every command.
  1 otherwise.
USAGE
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

go_bin=""
rust_bin=""
workdir="."
runs=10
budget_ratio="1.20"
out_dir="build/rust-parity-bench/latest"
commands=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --go-bin)
      shift
      go_bin="${1:-}"
      ;;
    --rust-bin)
      shift
      rust_bin="${1:-}"
      ;;
    --workdir)
      shift
      workdir="${1:-}"
      ;;
    --runs)
      shift
      runs="${1:-}"
      ;;
    --budget-ratio)
      shift
      budget_ratio="${1:-}"
      ;;
    --out-dir)
      shift
      out_dir="${1:-}"
      ;;
    --command)
      shift
      commands+=("${1:-}")
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift || true
done

if [[ -z "$go_bin" || -z "$rust_bin" ]]; then
  usage >&2
  exit 2
fi

if [[ ! -x "$go_bin" || ! -x "$rust_bin" ]]; then
  echo "go or rust binary path is not executable" >&2
  exit 1
fi
if [[ ! -d "$workdir" ]]; then
  echo "workdir not found: $workdir" >&2
  exit 1
fi

if [[ ${#commands[@]} -eq 0 ]]; then
  commands=("ps --json" "status --json" "tui --json")
fi

require_cmd jq
require_cmd python3

mkdir -p "$out_dir"

bench_json='[]'
comparison_json='[]'
failed=0

for command in "${commands[@]}"; do
  go_result="$(python3 - "$go_bin" "$workdir" "$runs" "$command" <<'PY'
import json, shlex, subprocess, sys, time
binary, workdir, runs, command = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
args = [binary] + shlex.split(command)
dur = []
fail = None
for _ in range(runs):
    start = time.perf_counter()
    proc = subprocess.run(args, cwd=workdir, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    elapsed_ms = (time.perf_counter() - start) * 1000.0
    dur.append(elapsed_ms)
    if proc.returncode != 0 and fail is None:
        fail = proc.returncode
out = {
    "mean_ms": sum(dur) / len(dur),
    "min_ms": min(dur),
    "max_ms": max(dur),
    "return_code": fail if fail is not None else 0,
}
print(json.dumps(out))
PY
)"

  rust_result="$(python3 - "$rust_bin" "$workdir" "$runs" "$command" <<'PY'
import json, shlex, subprocess, sys, time
binary, workdir, runs, command = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
args = [binary] + shlex.split(command)
dur = []
fail = None
for _ in range(runs):
    start = time.perf_counter()
    proc = subprocess.run(args, cwd=workdir, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    elapsed_ms = (time.perf_counter() - start) * 1000.0
    dur.append(elapsed_ms)
    if proc.returncode != 0 and fail is None:
        fail = proc.returncode
out = {
    "mean_ms": sum(dur) / len(dur),
    "min_ms": min(dur),
    "max_ms": max(dur),
    "return_code": fail if fail is not None else 0,
}
print(json.dumps(out))
PY
)"

  go_mean="$(jq -r '.mean_ms' <<<"$go_result")"
  rust_mean="$(jq -r '.mean_ms' <<<"$rust_result")"
  go_rc="$(jq -r '.return_code' <<<"$go_result")"
  rust_rc="$(jq -r '.return_code' <<<"$rust_result")"
  ratio="$(python3 - "$go_mean" "$rust_mean" <<'PY'
import sys
go_mean = float(sys.argv[1])
rust_mean = float(sys.argv[2])
if go_mean <= 0:
    print("inf")
else:
    print(rust_mean / go_mean)
PY
)"

  within_budget="$(python3 - "$ratio" "$budget_ratio" "$go_rc" "$rust_rc" <<'PY'
import sys
ratio = float(sys.argv[1]) if sys.argv[1] != "inf" else float("inf")
budget = float(sys.argv[2])
go_rc = int(sys.argv[3])
rust_rc = int(sys.argv[4])
print("true" if go_rc == 0 and rust_rc == 0 and ratio <= budget else "false")
PY
)"

  if [[ "$within_budget" != "true" ]]; then
    failed=1
  fi

  bench_json="$(jq \
    --arg command "$command" \
    --argjson go "$go_result" \
    --argjson rust "$rust_result" \
    '. += [{command: $command, go: $go, rust: $rust}]' <<<"$bench_json")"

  comparison_json="$(jq \
    --arg command "$command" \
    --argjson go_mean "$go_mean" \
    --argjson rust_mean "$rust_mean" \
    --argjson ratio "$ratio" \
    --argjson within_budget "$within_budget" \
    --argjson go_return_code "$go_rc" \
    --argjson rust_return_code "$rust_rc" \
    '. += [{
      command: $command,
      go_mean_ms: $go_mean,
      rust_mean_ms: $rust_mean,
      ratio: $ratio,
      within_budget: $within_budget,
      go_return_code: $go_return_code,
      rust_return_code: $rust_return_code
    }]' <<<"$comparison_json")"
done

generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
jq -n \
  --arg schema "parity.perf.v1" \
  --arg generated_at "$generated_at" \
  --arg go_bin "$go_bin" \
  --arg rust_bin "$rust_bin" \
  --arg workdir "$workdir" \
  --argjson runs "$runs" \
  --argjson budget_ratio "$budget_ratio" \
  --argjson failed "$failed" \
  --argjson benchmarks "$bench_json" \
  --argjson comparisons "$comparison_json" \
  '{
    schema: $schema,
    generated_at: $generated_at,
    go_bin: $go_bin,
    rust_bin: $rust_bin,
    workdir: $workdir,
    runs: $runs,
    budget_ratio: $budget_ratio,
    status: (if $failed == 0 then "pass" else "fail" end),
    benchmarks: $benchmarks,
    comparisons: $comparisons
  }' >"$out_dir/report.json"

{
  echo "status=$(jq -r '.status' "$out_dir/report.json")"
  echo "report=$out_dir/report.json"
  echo "commands=$(jq -r '.comparisons | length' "$out_dir/report.json")"
  echo "budget_ratio=$budget_ratio"
} >"$out_dir/summary.txt"

cat "$out_dir/summary.txt"

if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
