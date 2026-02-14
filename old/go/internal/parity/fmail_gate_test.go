package parity

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestFmailGateCommandAndTUIBaseline(t *testing.T) {
	t.Parallel()

	root := workspaceRoot(t)

	checks := []struct {
		path   string
		tokens []string
	}{
		{
			path: "docs/forge-mail/help/fmail-help.txt",
			tokens: []string{
				"fmail [command]",
				"send",
				"watch",
				"--robot-help",
			},
		},
		{
			path: "docs/forge-mail/help/fmail-tui-help.txt",
			tokens: []string{
				"fmail-tui [flags]",
				"--project",
				"--theme",
				"--poll-interval",
			},
		},
		{
			path: "docs/rust-fmail-command-manifest.md",
			tokens: []string{
				"`fmail` with **no args** launches `fmail-tui`",
				"`fmail` top-level command matrix",
				"`fmail-tui` CLI flag matrix",
			},
		},
	}

	for _, check := range checks {
		body := mustReadFile(t, filepath.Join(root, check.path))
		for _, token := range check.tokens {
			if !strings.Contains(body, token) {
				t.Fatalf("fmail gate drift: %s missing token %q", check.path, token)
			}
		}
	}
}
