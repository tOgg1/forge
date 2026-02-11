package doccheck

import (
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"testing"
)

func TestLegacyDropListCoversAddLegacyRegistrations(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	doc := readFile(t, filepath.Join(root, "docs/rust-legacy-drop-list.md"))

	files, err := filepath.Glob(filepath.Join(root, "internal/cli/*.go"))
	if err != nil {
		t.Fatalf("glob cli files: %v", err)
	}
	callRE := regexp.MustCompile(`addLegacyCommand\(\s*[a-zA-Z0-9_]+Cmd\s*\)`)

	set := map[string]struct{}{}
	for _, path := range files {
		body := readFile(t, path)
		if !callRE.MatchString(body) {
			continue
		}
		name := strings.TrimSuffix(filepath.Base(path), ".go")
		set[name] = struct{}{}
	}

	if len(set) == 0 {
		t.Fatal("expected at least one addLegacyCommand registration")
	}

	names := make([]string, 0, len(set))
	for name := range set {
		names = append(names, name)
	}
	sort.Strings(names)

	for _, name := range names {
		switch name {
		case "workspace":
			if !strings.Contains(doc, "`workspace` (`ws`)") {
				t.Fatalf("legacy drop list missing workspace/ws row")
			}
		default:
			if !strings.Contains(doc, "| `"+name+"` |") {
				t.Fatalf("legacy drop list missing command group %q", name)
			}
		}
	}

	if !strings.Contains(doc, "legacyCommandsEnabled = false") {
		t.Fatalf("legacy drop list missing legacyCommandsEnabled source-of-truth note")
	}
}
