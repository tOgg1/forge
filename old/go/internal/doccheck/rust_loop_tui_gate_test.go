package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustLoopTUIGateSpecPinned(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	gateDoc := readFile(t, filepath.Join(root, "docs/rust-loop-tui-gate.md"))
	checklist := readFile(t, filepath.Join(root, "docs/rust-loop-tui-checklist.md"))
	smokeScript := readFile(t, filepath.Join(root, "scripts/rust-loop-tui-smoke.sh"))
	matrix := readFile(t, filepath.Join(root, "docs/rust-parity-matrix.md"))

	for _, want := range []string{
		"internal/looptui",
		"forge-tui",
		"Workflow parity",
		"Failure-state parity",
		"Keymap parity",
		"Performance/readability parity",
		"cutover is blocked",
		"go test ./internal/looptui -count=1",
		"docs/rust-release-gate-checklist.md",
	} {
		if !strings.Contains(gateDoc, want) {
			t.Fatalf("rust-loop-tui-gate.md drift: missing %q", want)
		}
	}

	if !strings.Contains(matrix, "docs/rust-loop-tui-gate.md") {
		t.Fatalf("rust parity matrix missing loop TUI gate doc reference")
	}
	if !strings.Contains(matrix, "docs/rust-loop-tui-checklist.md") {
		t.Fatalf("rust parity matrix missing loop TUI checklist reference")
	}

	for _, want := range []string{
		"TestMainModeTabAndThemeShortcuts",
		"TestRunSelectionAndLogSourceCycle",
		"TestMainModeMultiLogsPagingKeys",
		"TestMainModePgUpScrollsLogs",
		"TestModeTransitions",
		"TestFilterModeRealtimeTextAndStatus",
		"TestSelectionChoosesNearestRowWhenLoopDisappears",
		"TestDeleteConfirmPromptMatchesPRD",
		"TestViewRendersErrorStateWithoutCrashing",
		"cargo test -p forge-tui --lib app::tests::bracket_keys_cycle_tabs",
		"cargo test -p forge-tui --lib app::tests::wizard_enter_validates_count_and_stays_on_step",
		"cargo test -p forge-tui --lib app::tests::render_error_state_shows_prefixed_error_text",
		"cargo test -p forge-tui --lib app::tests::delete_running_loop_shows_force",
		"cargo test -p forge-tui --lib actions::tests::stop_and_kill_prompts_match_go_shape",
	} {
		if !strings.Contains(smokeScript, want) {
			t.Fatalf("loop tui smoke script drift: missing %q", want)
		}
	}

	for _, want := range []string{
		"targeted Go `internal/looptui` workflow + failure-state probes pass",
		"targeted Rust `forge-tui` workflow + failure-state probes pass",
		"Force-delete prompt appears for a running loop",
		"Error banner/state rendering stays readable and non-crashing",
		"Stop/kill confirm prompts match expected operator wording",
	} {
		if !strings.Contains(checklist, want) {
			t.Fatalf("loop tui checklist drift: missing %q", want)
		}
	}
}
