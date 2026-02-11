package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestCoverageGateSelfTestWorkflowPinned(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	workflow := readFile(t, filepath.Join(root, ".github/workflows/coverage-gate-self-test.yml"))
	policy := readFile(t, filepath.Join(root, "docs/rust-coverage-policy.md"))

	// Ensure the intentional-fail path stays wired and obvious.
	for _, want := range []string{
		"workflow_dispatch:",
		"scripts/rust-coverage-gate.sh",
		"coverage-thresholds.intentional-fail.txt",
		"forge-parity-stub 101",
		"expected coverage gate to fail",
	} {
		if !strings.Contains(workflow, want) {
			t.Fatalf("coverage-gate-self-test.yml drift: missing %q", want)
		}
	}

	if !strings.Contains(policy, "coverage-gate-self-test.yml") {
		t.Fatalf("rust coverage policy missing reference to coverage gate self-test workflow")
	}
}

