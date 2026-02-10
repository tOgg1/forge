#!/usr/bin/env bash
set -euo pipefail

print_help() {
  cat <<'USAGE'
swarm-quant-stop.sh

Quantitative-stop helper for forge loop swarms.

Exits 0 when both project task counts are at/below thresholds:
  - open <= --open-max
  - in_progress <= --in-progress-max

Otherwise exits 1.

Usage:
  scripts/swarm-quant-stop.sh --project <project-id> [options]

Options:
  --project <id>           sv project id (required)
  --open-max <n>           maximum open count to consider "stop" (default: 0)
  --in-progress-max <n>    maximum in_progress count to consider "stop" (default: 0)
  --quiet                  suppress status output
  -h, --help               show this help
USAGE
}

project=""
open_max=0
in_progress_max=0
quiet=0

while (($# > 0)); do
  case "$1" in
    --project)
      shift
      project="${1:-}"
      ;;
    --open-max)
      shift
      open_max="${1:-}"
      ;;
    --in-progress-max)
      shift
      in_progress_max="${1:-}"
      ;;
    --quiet)
      quiet=1
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

if [[ -z "$project" ]]; then
  echo "missing required --project" >&2
  exit 2
fi

if ! [[ "$open_max" =~ ^[0-9]+$ ]]; then
  echo "invalid --open-max: $open_max" >&2
  exit 2
fi

if ! [[ "$in_progress_max" =~ ^[0-9]+$ ]]; then
  echo "invalid --in-progress-max: $in_progress_max" >&2
  exit 2
fi

if ! command -v sv >/dev/null 2>&1; then
  echo "sv not found in PATH" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq not found in PATH" >&2
  exit 2
fi

open_count="$({ sv task count --project "$project" --status open --json || true; } | jq -r '.data.total // empty')"
in_progress_count="$({ sv task count --project "$project" --status in_progress --json || true; } | jq -r '.data.total // empty')"

if ! [[ "$open_count" =~ ^[0-9]+$ ]]; then
  echo "failed to read open count for project $project" >&2
  exit 2
fi

if ! [[ "$in_progress_count" =~ ^[0-9]+$ ]]; then
  echo "failed to read in_progress count for project $project" >&2
  exit 2
fi

if (( open_count <= open_max && in_progress_count <= in_progress_max )); then
  if (( quiet == 0 )); then
    echo "stop: open=$open_count/$open_max in_progress=$in_progress_count/$in_progress_max"
  fi
  exit 0
fi

if (( quiet == 0 )); then
  echo "continue: open=$open_count/$open_max in_progress=$in_progress_count/$in_progress_max"
fi
exit 1
