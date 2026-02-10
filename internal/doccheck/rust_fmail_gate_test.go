package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustFmailGateSpecPinned(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	gateDoc := readFile(t, filepath.Join(root, "docs/rust-fmail-gate.md"))
	checklist := readFile(t, filepath.Join(root, "docs/rust-fmail-tui-checklist.md"))
	smokeScript := readFile(t, filepath.Join(root, "scripts/rust-fmail-tui-smoke.sh"))
	matrix := readFile(t, filepath.Join(root, "docs/rust-parity-matrix.md"))
	workflow := readFile(t, filepath.Join(root, ".github/workflows/ci.yml"))

	for _, want := range []string{
		"cmd/fmail",
		"internal/fmail",
		"cmd/fmail-tui",
		"internal/fmailtui",
		"fmail-core",
		"fmail-cli",
		"fmail-tui",
		"Command surface parity (`fmail`)",
		"TUI CLI parity (`fmail-tui`)",
		"docs/rust-fmail-tui-checklist.md",
		"scripts/rust-fmail-tui-smoke.sh",
		"TestFmailGateCommandAndTUIBaseline",
	} {
		if !strings.Contains(gateDoc, want) {
			t.Fatalf("rust-fmail-gate.md drift: missing %q", want)
		}
	}

	if !strings.Contains(matrix, "docs/rust-fmail-gate.md") {
		t.Fatalf("rust parity matrix missing fmail gate doc reference")
	}
	if !strings.Contains(matrix, "docs/rust-fmail-tui-checklist.md") {
		t.Fatalf("rust parity matrix missing fmail TUI checklist reference")
	}

	if !strings.Contains(workflow, "TestFmailGateCommandAndTUIBaseline") {
		t.Fatalf("ci parity job missing fmail gate baseline test invocation")
	}

	for _, want := range []string{
		"TestFmailGateCommandAndTUIBaseline",
		"TestTopicsViewComposeWritesMessageAndMarksRead",
		"TestOperatorSlashCommandsApplyPriorityTagsAndDM",
		"cargo test -p fmail-tui --lib topics::tests::topics_snapshot_render",
		"cargo test -p fmail-tui --lib operator::tests::render_with_conversations_and_messages",
		"cargo test -p fmail-tui --lib timeline::tests::timeline_snapshot_chronological",
		"cargo test -p fmail-tui --lib thread::tests::thread_snapshot",
	} {
		if !strings.Contains(smokeScript, want) {
			t.Fatalf("fmail smoke script drift: missing %q", want)
		}
	}

	for _, want := range []string{
		"targeted Go `internal/fmailtui` workflow probes pass",
		"targeted Rust `fmail-tui` topic/operator/timeline/thread probes pass",
		"operator reply flow",
	} {
		if !strings.Contains(checklist, want) {
			t.Fatalf("fmail tui checklist drift: missing %q", want)
		}
	}
}
