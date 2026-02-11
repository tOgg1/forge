#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Parity gate baseline for command surface + TUI baseline fixtures.
(
  cd "$repo_root/old/go"
  env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestFmailGateCommandAndTUIBaseline$' -count=1
)

# Go fmailtui behavior probes for operator/topic/timeline/layout workflows.
(
  cd "$repo_root/old/go"
  env -u GOROOT -u GOTOOLDIR go test ./internal/fmailtui -run '^(TestTopicsViewComposeWritesMessageAndMarksRead|TestTopicsViewRebuildItemsHonorsStarFilterAndSort|TestTimelineLoadMergesTopicsAndDMsChronologically|TestOperatorSlashCommandsApplyPriorityTagsAndDM|TestLayoutControlsAndPersistence)$' -count=1
)

# Rust fmail-tui workflow probes (topic/operator/timeline/thread snapshots).
(
  cargo test -p fmail-tui --lib topics::tests::topics_snapshot_render
  cargo test -p fmail-tui --lib operator::tests::render_with_conversations_and_messages
  cargo test -p fmail-tui --lib timeline::tests::timeline_snapshot_chronological
  cargo test -p fmail-tui --lib thread::tests::thread_snapshot
)

echo "rust-fmail-tui-smoke: PASS"
