package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustCoverageNightlyWorkflowPinned(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	workflow := readFile(t, filepath.Join(root, ".github/workflows/rust-coverage-nightly.yml"))
	policy := readFile(t, filepath.Join(root, "docs/rust-coverage-policy.md"))

	for _, want := range []string{
		"schedule:",
		"cron:",
		"taiki-e/install-action@cargo-llvm-cov",
		"scripts/rust-coverage-gate.sh",
		"cargo llvm-cov",
		"--lcov",
		"--output-path coverage/lcov.info",
		"name: rust-coverage-nightly",
	} {
		if !strings.Contains(workflow, want) {
			t.Fatalf("rust-coverage-nightly workflow drift: missing %q", want)
		}
	}

	if !strings.Contains(policy, "rust-coverage-nightly.yml") {
		t.Fatalf("rust coverage policy missing reference to rust-coverage-nightly.yml")
	}
	if !strings.Contains(policy, "`rust-coverage-nightly`") {
		t.Fatalf("rust coverage policy missing nightly artifact name")
	}
}

