package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustCoveragePolicyAndWorkflowPinned(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)

	policy := readFile(t, filepath.Join(root, "docs/rust-coverage-policy.md"))
	workflow := readFile(t, filepath.Join(root, ".github/workflows/ci.yml"))

	// Policy: tooling + canonical report format + artifact contract.
	for _, want := range []string{
		"`cargo-llvm-cov`",
		"Machine-readable report format: LCOV",
		"`rust/coverage/lcov.info`",
		"CI artifact name: `rust-coverage`",
		"`cargo llvm-cov report --summary-only`",
		"`rust/coverage-thresholds.txt`",
		"`rust/coverage-waivers.txt`",
		"`scripts/rust-coverage-gate.sh`",
		"`crate|expires_on|approved_by|issue|reason`",
	} {
		if !strings.Contains(policy, want) {
			t.Fatalf("coverage policy missing %q", want)
		}
	}

	// Workflow: must match policy.
	for _, want := range []string{
		"rust-coverage:",
		"taiki-e/install-action@cargo-llvm-cov",
		// Keep check tolerant of multiline YAML `run:` blocks.
		"cargo llvm-cov",
		"--workspace",
		"--lcov",
		"--output-path coverage/lcov.info",
		"cargo llvm-cov report --summary-only",
		"name: rust-coverage",
		"path: rust/coverage/lcov.info",
		"run: scripts/rust-coverage-gate.sh",
		"Waivers: rust/coverage-waivers.txt",
	} {
		if !strings.Contains(workflow, want) {
			t.Fatalf("ci workflow drift: missing %q", want)
		}
	}
}
