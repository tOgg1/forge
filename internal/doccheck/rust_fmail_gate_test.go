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
		"TestFmailGateCommandAndTUIBaseline",
	} {
		if !strings.Contains(gateDoc, want) {
			t.Fatalf("rust-fmail-gate.md drift: missing %q", want)
		}
	}

	if !strings.Contains(matrix, "docs/rust-fmail-gate.md") {
		t.Fatalf("rust parity matrix missing fmail gate doc reference")
	}

	if !strings.Contains(workflow, "TestFmailGateCommandAndTUIBaseline") {
		t.Fatalf("ci parity job missing fmail gate baseline test invocation")
	}
}
