package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustDaemonProtoGateSpecPinned(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	gateDoc := readFile(t, filepath.Join(root, "docs/rust-daemon-proto-gate.md"))
	matrix := readFile(t, filepath.Join(root, "docs/rust-parity-matrix.md"))
	workflow := readFile(t, filepath.Join(root, ".github/workflows/ci.yml"))

	for _, want := range []string{
		"proto/forged/v1/forged.proto",
		"Rust client -> Go server",
		"Go client -> Rust server",
		"StartLoopRunner",
		"StreamEvents",
		"TestDaemonProtoGateProtoSurfaceLocked",
		"internal/parity/testdata/oracle/expected/forged/proto-wire/summary.json",
		"TestProtoWireGateCriticalRPCFixtures",
	} {
		if !strings.Contains(gateDoc, want) {
			t.Fatalf("rust-daemon-proto-gate.md drift: missing %q", want)
		}
	}

	if !strings.Contains(matrix, "docs/rust-daemon-proto-gate.md") {
		t.Fatalf("rust parity matrix missing daemon/proto gate doc reference")
	}

	if !strings.Contains(workflow, "TestDaemonProtoGate") {
		t.Fatalf("ci parity job missing daemon/proto gate test invocation")
	}
	if !strings.Contains(workflow, "TestProtoWireGate") {
		t.Fatalf("ci parity job missing proto wire gate test invocation")
	}
}
