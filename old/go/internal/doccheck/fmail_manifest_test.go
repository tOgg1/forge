package doccheck

import (
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

func TestFmailHelpSnapshotsCurrent(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	fmailSnapshot := readFile(t, filepath.Join(root, "docs/forge-mail/help/fmail-help.txt"))
	fmailTUISnapshot := readFile(t, filepath.Join(root, "docs/forge-mail/help/fmail-tui-help.txt"))

	fmailNow := runGoHelp(t, root, "./cmd/fmail")
	fmailTUINow := runGoHelp(t, root, "./cmd/fmail-tui")

	if normalize(fmailNow) != normalize(fmailSnapshot) {
		t.Fatalf("fmail help snapshot drift; regenerate docs/forge-mail/help/fmail-help.txt")
	}
	if normalize(fmailTUINow) != normalize(fmailTUISnapshot) {
		t.Fatalf("fmail-tui help snapshot drift; regenerate docs/forge-mail/help/fmail-tui-help.txt")
	}
}

func TestFmailManifestCoversHelpSnapshotSurface(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	manifest := readFile(t, filepath.Join(root, "docs/rust-fmail-command-manifest.md"))
	fmailSnapshot := readFile(t, filepath.Join(root, "docs/forge-mail/help/fmail-help.txt"))
	fmailTUISnapshot := readFile(t, filepath.Join(root, "docs/forge-mail/help/fmail-tui-help.txt"))

	for _, cmd := range extractFmailCommands(fmailSnapshot) {
		if !strings.Contains(manifest, "| `"+cmd+"` |") {
			t.Fatalf("manifest missing fmail command row for %q", cmd)
		}
	}
	for _, flag := range extractLongFlags(fmailSnapshot) {
		if !strings.Contains(manifest, "| `"+flag+"` |") {
			t.Fatalf("manifest missing fmail global flag row for %q", flag)
		}
	}
	for _, flag := range extractLongFlags(fmailTUISnapshot) {
		if !strings.Contains(manifest, "| `"+flag+"` |") {
			t.Fatalf("manifest missing fmail-tui flag row for %q", flag)
		}
	}
	if !strings.Contains(manifest, "fmail` with **no args** launches `fmail-tui`") {
		t.Fatalf("manifest missing no-args launch behavior note")
	}
}

func repoRoot(t *testing.T) string {
	t.Helper()
	wd, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	cur := wd
	for {
		if _, err := os.Stat(filepath.Join(cur, "go.mod")); err == nil {
			return cur
		}
		next := filepath.Dir(cur)
		if next == cur {
			t.Fatal("repo root with go.mod not found")
		}
		cur = next
	}
}

func runGoHelp(t *testing.T, root, pkg string) string {
	t.Helper()
	cmd := exec.Command("go", "run", pkg, "--help")
	cmd.Dir = root
	cmd.Env = withoutToolchainOverrides(os.Environ())
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("run %s --help: %v\n%s", pkg, err, string(out))
	}
	return string(out)
}

func withoutToolchainOverrides(env []string) []string {
	filtered := make([]string, 0, len(env))
	for _, kv := range env {
		if strings.HasPrefix(kv, "GOROOT=") || strings.HasPrefix(kv, "GOTOOLDIR=") {
			continue
		}
		filtered = append(filtered, kv)
	}
	return filtered
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	body, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	return string(body)
}

func normalize(s string) string {
	return strings.TrimSpace(strings.ReplaceAll(s, "\r\n", "\n"))
}

func extractFmailCommands(help string) []string {
	var cmds []string
	inCommands := false
	for _, line := range strings.Split(help, "\n") {
		trimmed := strings.TrimSpace(line)
		switch {
		case strings.HasPrefix(trimmed, "Available Commands:"):
			inCommands = true
			continue
		case strings.HasPrefix(trimmed, "Flags:"):
			inCommands = false
		}
		if !inCommands || trimmed == "" {
			continue
		}
		fields := strings.Fields(trimmed)
		if len(fields) == 0 {
			continue
		}
		cmds = append(cmds, fields[0])
	}
	sort.Strings(cmds)
	return cmds
}

func extractLongFlags(help string) []string {
	set := map[string]struct{}{}
	for _, line := range strings.Split(help, "\n") {
		for _, field := range strings.Fields(line) {
			if strings.HasPrefix(field, "--") {
				flag := cleanFlagToken(field)
				if strings.HasPrefix(flag, "--") && strings.IndexFunc(flag[2:], func(r rune) bool {
					return !(r >= 'a' && r <= 'z' || r >= '0' && r <= '9' || r == '-')
				}) == -1 {
					set[flag] = struct{}{}
				}
			}
		}
	}
	flags := make([]string, 0, len(set))
	for flag := range set {
		flags = append(flags, flag)
	}
	sort.Strings(flags)
	return flags
}

func cleanFlagToken(token string) string {
	token = strings.TrimSpace(token)
	for len(token) > 0 {
		last := token[len(token)-1]
		if (last >= 'a' && last <= 'z') || (last >= '0' && last <= '9') || last == '-' {
			break
		}
		token = token[:len(token)-1]
	}
	return token
}
