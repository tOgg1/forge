#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: scripts/parity-scenario-compare.sh \
  --scenario <json> \
  --go-bin <path> \
  --rust-bin <path> \
  [--fixture <dir>] \
  [--out-dir <dir>] \
  [--db-path <relative-path>]...

Runs the same scenario against Go and Rust binaries, then writes:
  - report.json (step-level output + exit-code diff + DB side-effect diff)
  - summary.txt
  - diff/* (stdout/stderr/db dumps when drift is detected)

Default DB paths:
  .forge/forge.db
  .fmail/local.db
USAGE
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

run_cmd() {
  local binary="$1"
  local fixture_dir="$2"
  local stdout_file="$3"
  local stderr_file="$4"
  shift 4
  local args=("$@")

  (
    cd "$fixture_dir"
    set +e
    "$binary" "${args[@]}" >"$stdout_file" 2>"$stderr_file"
    echo "$?" >"${stdout_file}.exit"
  )
}

canon_db_dump() {
  local db_path="$1"
  local dump_out="$2"
  sqlite3 "$db_path" ".dump" \
    | sed '/^--/d' \
    | sed '/^PRAGMA/d' \
    >"$dump_out"
}

scenario=""
go_bin=""
rust_bin=""
fixture="."
out_dir="build/parity-scenario/latest"
db_paths=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --scenario)
      shift
      scenario="${1:-}"
      ;;
    --go-bin)
      shift
      go_bin="${1:-}"
      ;;
    --rust-bin)
      shift
      rust_bin="${1:-}"
      ;;
    --fixture)
      shift
      fixture="${1:-}"
      ;;
    --out-dir)
      shift
      out_dir="${1:-}"
      ;;
    --db-path)
      shift
      db_paths+=("${1:-}")
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

if [[ -z "$scenario" || -z "$go_bin" || -z "$rust_bin" ]]; then
  usage >&2
  exit 2
fi

if [[ ! -f "$scenario" ]]; then
  echo "scenario not found: $scenario" >&2
  exit 1
fi
if [[ ! -d "$fixture" ]]; then
  echo "fixture dir not found: $fixture" >&2
  exit 1
fi
if [[ ! -x "$go_bin" || ! -x "$rust_bin" ]]; then
  echo "binary path is not executable (go or rust)" >&2
  exit 1
fi

if [[ ${#db_paths[@]} -eq 0 ]]; then
  db_paths=(".forge/forge.db" ".fmail/local.db")
fi

require_cmd jq
require_cmd sqlite3
require_cmd shasum
require_cmd diff

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/parity-scenario-compare.XXXXXX")"
trap 'rm -rf "$tmp_root"' EXIT

go_fixture="$tmp_root/go-fixture"
rust_fixture="$tmp_root/rust-fixture"
mkdir -p "$go_fixture" "$rust_fixture"
cp -R "$fixture"/. "$go_fixture"/
cp -R "$fixture"/. "$rust_fixture"/

mkdir -p "$out_dir/diff"

scenario_name="$(jq -r '.name // "unnamed-scenario"' "$scenario")"
step_count="$(jq '.steps | length' "$scenario")"
if [[ "$step_count" -le 0 ]]; then
  echo "scenario has no steps: $scenario" >&2
  exit 1
fi

steps_json='[]'
db_json='[]'
drift=0
step_drift_count=0
db_drift_count=0

for ((i=0; i<step_count; i++)); do
  step_name="$(jq -r ".steps[$i].name" "$scenario")"
  args=()
  while IFS= read -r arg; do
    args+=("$arg")
  done < <(jq -r ".steps[$i].args[]" "$scenario")

  go_stdout="$tmp_root/go-step-$i.stdout"
  go_stderr="$tmp_root/go-step-$i.stderr"
  rust_stdout="$tmp_root/rust-step-$i.stdout"
  rust_stderr="$tmp_root/rust-step-$i.stderr"

  run_cmd "$go_bin" "$go_fixture" "$go_stdout" "$go_stderr" "${args[@]}"
  run_cmd "$rust_bin" "$rust_fixture" "$rust_stdout" "$rust_stderr" "${args[@]}"

  go_exit="$(cat "${go_stdout}.exit")"
  rust_exit="$(cat "${rust_stdout}.exit")"

  stdout_equal=true
  stderr_equal=true
  if ! diff -u "$go_stdout" "$rust_stdout" >"$out_dir/diff/step-$i-stdout.diff"; then
    stdout_equal=false
  fi
  if ! diff -u "$go_stderr" "$rust_stderr" >"$out_dir/diff/step-$i-stderr.diff"; then
    stderr_equal=false
  fi
  if [[ "$stdout_equal" == true ]]; then
    rm -f "$out_dir/diff/step-$i-stdout.diff"
  fi
  if [[ "$stderr_equal" == true ]]; then
    rm -f "$out_dir/diff/step-$i-stderr.diff"
  fi

  has_step_drift=false
  if [[ "$go_exit" != "$rust_exit" || "$stdout_equal" != true || "$stderr_equal" != true ]]; then
    has_step_drift=true
    drift=1
    step_drift_count=$((step_drift_count + 1))
  fi

  args_json="$(printf '%s\n' "${args[@]}" | jq -R . | jq -s .)"
  steps_json="$(jq \
    --arg name "$step_name" \
    --argjson args "$args_json" \
    --argjson go_exit "$go_exit" \
    --argjson rust_exit "$rust_exit" \
    --argjson stdout_equal "$stdout_equal" \
    --argjson stderr_equal "$stderr_equal" \
    --argjson has_drift "$has_step_drift" \
    '. += [{
      name: $name,
      args: $args,
      go_exit_code: $go_exit,
      rust_exit_code: $rust_exit,
      stdout_equal: $stdout_equal,
      stderr_equal: $stderr_equal,
      has_drift: $has_drift
    }]' <<<"$steps_json")"
done

for rel_path in "${db_paths[@]}"; do
  go_db="$go_fixture/$rel_path"
  rust_db="$rust_fixture/$rel_path"
  go_exists=false
  rust_exists=false
  equal=true
  go_hash=""
  rust_hash=""

  if [[ -f "$go_db" ]]; then
    go_exists=true
  fi
  if [[ -f "$rust_db" ]]; then
    rust_exists=true
  fi

  if [[ "$go_exists" == true && "$rust_exists" == true ]]; then
    go_dump="$tmp_root/go-$(basename "$rel_path").dump.sql"
    rust_dump="$tmp_root/rust-$(basename "$rel_path").dump.sql"
    canon_db_dump "$go_db" "$go_dump"
    canon_db_dump "$rust_db" "$rust_dump"
    go_hash="$(shasum -a 256 "$go_dump" | awk '{print $1}')"
    rust_hash="$(shasum -a 256 "$rust_dump" | awk '{print $1}')"
    if ! diff -u "$go_dump" "$rust_dump" >"$out_dir/diff/db-$(basename "$rel_path").diff"; then
      equal=false
    else
      rm -f "$out_dir/diff/db-$(basename "$rel_path").diff"
    fi
  elif [[ "$go_exists" == false && "$rust_exists" == false ]]; then
    equal=true
  else
    equal=false
  fi

  if [[ "$equal" != true ]]; then
    drift=1
    db_drift_count=$((db_drift_count + 1))
  fi

  db_json="$(jq \
    --arg path "$rel_path" \
    --argjson go_exists "$go_exists" \
    --argjson rust_exists "$rust_exists" \
    --arg go_hash "$go_hash" \
    --arg rust_hash "$rust_hash" \
    --argjson equal "$equal" \
    '. += [{
      path: $path,
      go_exists: $go_exists,
      rust_exists: $rust_exists,
      go_sha256: $go_hash,
      rust_sha256: $rust_hash,
      equal: $equal
    }]' <<<"$db_json")"
done

generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
jq -n \
  --arg scenario "$scenario_name" \
  --arg generated_at "$generated_at" \
  --arg go_bin "$go_bin" \
  --arg rust_bin "$rust_bin" \
  --arg fixture "$fixture" \
  --argjson steps "$steps_json" \
  --argjson db_side_effects "$db_json" \
  --argjson step_drift_count "$step_drift_count" \
  --argjson db_drift_count "$db_drift_count" \
  --argjson drift "$drift" \
  '{
    schema: "parity.scenario.v1",
    scenario: $scenario,
    generated_at: $generated_at,
    go_bin: $go_bin,
    rust_bin: $rust_bin,
    fixture: $fixture,
    step_drift_count: $step_drift_count,
    db_drift_count: $db_drift_count,
    drift: ($drift != 0),
    steps: $steps,
    db_side_effects: $db_side_effects
  }' >"$out_dir/report.json"

{
  echo "scenario=$scenario_name"
  echo "go_bin=$go_bin"
  echo "rust_bin=$rust_bin"
  echo "fixture=$fixture"
  echo "step_drift_count=$step_drift_count"
  echo "db_drift_count=$db_drift_count"
  echo "drift=$([[ "$drift" -eq 0 ]] && echo false || echo true)"
  echo "report=$out_dir/report.json"
  echo "diff_dir=$out_dir/diff"
} >"$out_dir/summary.txt"

cat "$out_dir/summary.txt"

if [[ "$drift" -ne 0 ]]; then
  exit 1
fi
