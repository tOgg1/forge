#!/usr/bin/env bash
set -euo pipefail

count="$({ sv task list --status open --json | jq -r '[.data.tasks[] | select((.project=="prj-v5pc07bf") and ((.title|type)=="string") and (.title|test("^(TUI[-:]|PAR-)")) and (.title|test("Epic";"i")|not))] | length'; } 2>/dev/null || echo 9999)"

echo "tui_next_non_epic_open=${count}"
[ "${count}" -eq 0 ]
