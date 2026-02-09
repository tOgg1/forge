#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

env -u GOROOT -u GOTOOLDIR go test ./internal/fmailtui -run '^(TestTopicsViewComposeWritesMessageAndMarksRead|TestTopicsViewRebuildItemsHonorsStarFilterAndSort|TestTimelineLoadMergesTopicsAndDMsChronologically|TestOperatorSlashCommandsApplyPriorityTagsAndDM|TestLayoutControlsAndPersistence)$' -count=1

echo "rust-fmail-tui-smoke: PASS"
