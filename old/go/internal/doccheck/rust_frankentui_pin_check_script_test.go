package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustFrankentuiPinCheckScriptSupportsGrepFallback(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	script := readFile(t, filepath.Join(root, "scripts", "rust-frankentui-pin-check.sh"))

	for _, want := range []string{
		"command -v rg",
		"grep -En",
		"search_pattern",
	} {
		if !strings.Contains(script, want) {
			t.Fatalf("rust-frankentui pin check script missing fallback contract %q", want)
		}
	}
}
