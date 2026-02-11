package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustParityMatrixTemplateIsCompleteAndWellFormed(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	doc := readFile(t, filepath.Join(root, "docs/rust-parity-matrix.md"))

	if !strings.Contains(doc, "| Surface | Go source of truth | Rust target | Gate artifact | Status | Notes |") {
		t.Fatalf("parity matrix missing expected table header")
	}

	allowedStatus := map[string]struct{}{
		"not-started":  {},
		"in-progress":  {},
		"parity-green": {},
		"blocked":      {},
	}

	requiredSurfaces := []string{
		"Forge CLI help/flags",
		"fmail CLI help/flags",
		"fmail-tui CLI flags",
		"DB migrations/schema",
		"Loop runtime semantics",
		"Daemon + runner protocol",
		"Loop TUI workflows",
		"fmail/fmail-tui workflows",
	}

	seen := map[string]bool{}
	for _, s := range requiredSurfaces {
		seen[s] = false
	}

	for _, line := range strings.Split(doc, "\n") {
		if !strings.HasPrefix(line, "|") {
			continue
		}
		if strings.Contains(line, "---") { // separator row
			continue
		}
		if strings.Contains(line, "Surface") && strings.Contains(line, "Go source of truth") {
			continue
		}

		fields := strings.Split(line, "|")
		// Leading/trailing pipes yield empty first/last fields.
		if len(fields) < 7 {
			t.Fatalf("parity matrix row malformed: %q", line)
		}

		surface := strings.TrimSpace(fields[1])
		gateArtifact := strings.TrimSpace(fields[4])
		status := strings.TrimSpace(fields[5])

		if surface == "" {
			t.Fatalf("parity matrix row has empty surface: %q", line)
		}
		if gateArtifact == "" {
			t.Fatalf("parity matrix row has empty gate artifact for surface %q", surface)
		}
		if _, ok := allowedStatus[status]; !ok {
			t.Fatalf("parity matrix row has invalid status %q for surface %q", status, surface)
		}

		if _, ok := seen[surface]; ok {
			seen[surface] = true
		}
	}

	for _, s := range requiredSurfaces {
		if !seen[s] {
			t.Fatalf("parity matrix missing required surface row %q", s)
		}
	}
}

