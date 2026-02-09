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
	matrix := readFile(t, filepath.Join(root, "docs/rust-parity-matrix.md"))

	for _, want := range []string{
		"internal/looptui",
		"forge-tui",
		"Workflow parity",
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
}
