package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustCLIGateSpecPinned(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	gateDoc := readFile(t, filepath.Join(root, "docs/rust-cli-gate.md"))
	matrix := readFile(t, filepath.Join(root, "docs/rust-parity-matrix.md"))
	workflow := readFile(t, filepath.Join(root, ".github/workflows/ci.yml"))

	for _, want := range []string{
		"cmd/forge",
		"internal/cli",
		"forge-cli",
		"help.txt",
		"global-flags.txt",
		"invalid-flag.exit-code.txt",
		"TestCLIGateRootOracleBaseline",
		"100% parity",
	} {
		if !strings.Contains(gateDoc, want) {
			t.Fatalf("rust-cli-gate.md drift: missing %q", want)
		}
	}

	if !strings.Contains(matrix, "docs/rust-cli-gate.md") {
		t.Fatalf("rust parity matrix missing CLI gate doc reference")
	}

	if !strings.Contains(workflow, "TestCLIGateRootOracleBaseline") {
		t.Fatalf("ci parity job missing CLI gate baseline test invocation")
	}
}
