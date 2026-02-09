#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

env -u GOROOT -u GOTOOLDIR go test ./internal/looptui -run '^(TestMainModeTabAndThemeShortcuts|TestRunSelectionAndLogSourceCycle|TestMainModeMultiLogsPagingKeys|TestMainModePgUpScrollsLogs|TestModeTransitions|TestFilterModeRealtimeTextAndStatus)$' -count=1

echo "rust-loop-tui-smoke: PASS"
