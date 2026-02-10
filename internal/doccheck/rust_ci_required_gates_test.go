package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustCIRequiredGatesPinned(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	workflow := readFile(t, filepath.Join(root, ".github/workflows/ci.yml"))
	releaseChecklist := readFile(t, filepath.Join(root, "docs/rust-release-gate-checklist.md"))
	matrix := readFile(t, filepath.Join(root, "docs/rust-parity-matrix.md"))
	adr := readFile(t, filepath.Join(root, "docs/adr/0005-rust-single-switch-policy.md"))

	for _, want := range []string{
		"parity:",
		"rust-boundary:",
		"rust-quality:",
		"db-compat:",
		"rust-coverage:",
		"scripts/rust-boundary-check.sh",
		"scripts/rust-db-compat-check.sh",
		"needs: [lint, test, parity, baseline-snapshot, baseline-refresh-protocol, rust-boundary, rust-quality, db-compat, rust-coverage]",
	} {
		if !strings.Contains(workflow, want) {
			t.Fatalf("ci required gate drift: missing %q", want)
		}
	}

	for _, want := range []string{
		"`parity` job green",
		"`rust-boundary` job green",
		"`rust-quality` job green",
		"`db-compat` job green",
	} {
		if !strings.Contains(releaseChecklist, want) {
			t.Fatalf("release gate checklist drift: missing %q", want)
		}
	}

	for _, want := range []string{
		"`parity`",
		"`rust-boundary`",
		"`rust-quality`",
		"`db-compat`",
		"`rust-coverage`",
	} {
		if !strings.Contains(matrix, want) {
			t.Fatalf("rust parity matrix required checks drift: missing %q", want)
		}
	}

	for _, want := range []string{
		"`parity`",
		"`rust-boundary`",
		"`rust-quality`",
		"`db-compat`",
		"`rust-coverage`",
	} {
		if !strings.Contains(adr, want) {
			t.Fatalf("single-switch ADR required checks drift: missing %q", want)
		}
	}
}
