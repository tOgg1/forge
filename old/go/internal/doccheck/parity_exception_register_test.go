package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestParityExceptionRegisterMustRemainEmpty(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	register := readFile(t, filepath.Join(root, "docs/rust-parity-exception-register.md"))
	releaseChecklist := readFile(t, filepath.Join(root, "docs/rust-release-gate-checklist.md"))
	matrix := readFile(t, filepath.Join(root, "docs/rust-parity-matrix.md"))

	if !strings.Contains(releaseChecklist, "docs/rust-parity-exception-register.md") {
		t.Fatalf("release gate checklist missing parity exception register reference")
	}
	if !strings.Contains(matrix, "docs/rust-parity-exception-register.md") {
		t.Fatalf("rust parity matrix missing parity exception register reference")
	}

	section := markdownSection(t, register, "## Active exceptions")
	lines := nonEmptyLines(section)
	if len(lines) != 1 || lines[0] != "(none)" {
		t.Fatalf("parity exception register must stay empty; got: %q", strings.Join(lines, " | "))
	}
}

func markdownSection(t *testing.T, doc, heading string) string {
	t.Helper()
	start := strings.Index(doc, heading)
	if start == -1 {
		t.Fatalf("missing heading %q", heading)
	}
	start += len(heading)
	rest := doc[start:]
	if idx := strings.Index(rest, "\n## "); idx >= 0 {
		return rest[:idx]
	}
	return rest
}

func nonEmptyLines(s string) []string {
	var out []string
	for _, line := range strings.Split(s, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			continue
		}
		out = append(out, trimmed)
	}
	return out
}
