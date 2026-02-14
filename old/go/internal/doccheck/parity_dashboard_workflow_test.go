package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestParityDashboardWorkflowUsesWorkspaceOutputPaths(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	ci := readFile(t, filepath.Join(root, ".github/workflows/ci.yml"))
	nightly := readFile(t, filepath.Join(root, ".github/workflows/parity-nightly.yml"))

	for name, body := range map[string]string{
		"ci":      ci,
		"nightly": nightly,
	} {
		if !strings.Contains(body, "(cd old/go && go run ./cmd/parity-dashboard") {
			t.Fatalf("%s workflow missing parity-dashboard subshell execution", name)
		}
		if !strings.Contains(body, `--out "$GITHUB_WORKSPACE/parity-dashboard"`) {
			t.Fatalf("%s workflow missing workspace parity-dashboard output path", name)
		}
		if strings.Contains(body, "cat parity-dashboard/parity-dashboard.md") {
			t.Fatalf("%s workflow still cats parity-dashboard markdown using relative path", name)
		}
		if !strings.Contains(body, `cat "$GITHUB_WORKSPACE/parity-dashboard/parity-dashboard.md"`) {
			t.Fatalf("%s workflow missing workspace-absolute parity-dashboard markdown summary path", name)
		}
	}
}
