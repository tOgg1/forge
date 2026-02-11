#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Go loop TUI workflow + failure-state probes.
(
  cd "$repo_root/old/go"
  env -u GOROOT -u GOTOOLDIR go test ./internal/looptui -run '^(TestMainModeTabAndThemeShortcuts|TestRunSelectionAndLogSourceCycle|TestMainModeMultiLogsPagingKeys|TestMainModePgUpScrollsLogs|TestModeTransitions|TestFilterModeRealtimeTextAndStatus|TestSelectionChoosesNearestRowWhenLoopDisappears|TestDeleteConfirmPromptMatchesPRD|TestViewRendersErrorStateWithoutCrashing|TestWizardStepValidation|TestCreateLoopsWizardPath)$' -count=1
)

# Rust forge-tui workflow + failure-state probes.
(
  cargo test -p forge-tui --lib app::tests::bracket_keys_cycle_tabs
  cargo test -p forge-tui --lib app::tests::help_returns_to_previous_mode
  cargo test -p forge-tui --lib app::tests::comma_dot_move_run_selection
  cargo test -p forge-tui --lib app::tests::multi_logs_tab_sets_focus_right
  cargo test -p forge-tui --lib app::tests::u_d_scroll_in_logs_tab
  cargo test -p forge-tui --lib app::tests::filter_text_narrows_results
  cargo test -p forge-tui --lib app::tests::wizard_enter_advances_steps_and_back_goes_previous
  cargo test -p forge-tui --lib app::tests::wizard_enter_validates_count_and_stays_on_step
  cargo test -p forge-tui --lib app::tests::render_error_state_shows_prefixed_error_text
  cargo test -p forge-tui --lib app::tests::delete_running_loop_shows_force
  cargo test -p forge-tui --lib actions::tests::stop_and_kill_prompts_match_go_shape
)

echo "rust-loop-tui-smoke: PASS"
