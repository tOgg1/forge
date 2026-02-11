package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustRuntimeGateSpecPinned(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	gateDoc := readFile(t, filepath.Join(root, "docs/rust-runtime-gate.md"))
	matrix := readFile(t, filepath.Join(root, "docs/rust-parity-matrix.md"))
	workflow := readFile(t, filepath.Join(root, ".github/workflows/ci.yml"))

	for _, want := range []string{
		"internal/loop",
		"internal/queue",
		"internal/scheduler",
		"forge-loop",
		"Queue semantics parity",
		"Smart-stop parity",
		"Logging and ledger parity",
		"Runtime dispatch parity",
		"TestRuntimeGateLoopQueueSmartStopLedger",
	} {
		if !strings.Contains(gateDoc, want) {
			t.Fatalf("rust-runtime-gate.md drift: missing %q", want)
		}
	}

	if !strings.Contains(matrix, "docs/rust-runtime-gate.md") {
		t.Fatalf("rust parity matrix missing runtime gate doc reference")
	}

	if !strings.Contains(workflow, "TestRuntimeGateLoopQueueSmartStopLedger") {
		t.Fatalf("ci parity job missing runtime gate baseline test invocation")
	}
}
