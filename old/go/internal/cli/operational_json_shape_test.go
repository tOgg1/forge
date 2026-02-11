package cli

import (
	"encoding/json"
	"os"
	"testing"
)

func TestOperationalCommands_JSONShape_Status(t *testing.T) {
	repo := t.TempDir()
	cleanupConfig := withTempConfig(t, repo)
	defer cleanupConfig()

	withWorkingDir(t, repo, func() {
		restore := snapshotCLIFlags()
		defer restore()

		restoreLogging := silenceLogging(t)
		defer restoreLogging()

		jsonOutput = true
		jsonlOutput = false
		noColor = true
		quiet = true

		stdout, stderr, err := captureStdoutStderr(func() error { return statusCmd.RunE(statusCmd, nil) })
		if err != nil {
			t.Fatalf("status --json: %v\nstderr:\n%s", err, stderr)
		}

		var payload map[string]any
		if err := json.Unmarshal([]byte(stdout), &payload); err != nil {
			t.Fatalf("status output not valid json: %v\nstdout:\n%s\nstderr:\n%s", err, stdout, stderr)
		}

		nodesAny, ok := payload["nodes"].(map[string]any)
		if !ok {
			t.Fatalf("status json missing nodes object: %#v", payload["nodes"])
		}
		if total, ok := nodesAny["total"].(float64); !ok || total != 0 {
			t.Fatalf("status json nodes.total want 0, got %#v", nodesAny["total"])
		}

		if _, ok := payload["timestamp"]; !ok {
			t.Fatalf("status json missing timestamp key")
		}
		if _, ok := payload["agents"]; !ok {
			t.Fatalf("status json missing agents key")
		}
		if _, ok := payload["workspaces"]; !ok {
			t.Fatalf("status json missing workspaces key")
		}
		if _, ok := payload["alerts"]; !ok {
			t.Fatalf("status json missing alerts key")
		}

		// Ensure the test doesn't leak state via the user's environment.
		if v := os.Getenv("HOME"); v == "" {
			t.Fatalf("expected HOME to be set in test env")
		}
	})
}

