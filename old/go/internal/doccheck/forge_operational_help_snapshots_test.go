package doccheck

import (
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestForgeOperationalHelpSnapshotsCurrent(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	forgeBin := buildGoBinary(t, root, "./cmd/forge")

	home := t.TempDir()
	for _, cmd := range []string{
		"audit",
		"export",
		"status",
		"doctor",
		"wait",
		"explain",
		"hook",
		"lock",
		"context",
		"use",
		"init",
		"config",
		"skills",
		"completion",
	} {
		snapshotPath := filepath.Join(root, "docs/forge/help/forge-help-"+cmd+".txt")
		want := readFile(t, snapshotPath)

		got := runBinaryWithHome(t, root, forgeBin, home, "help", cmd)
		if got.exitCode != 0 {
			t.Fatalf("forge help %s exit code = %d, want 0 (stderr=%q)", cmd, got.exitCode, got.stderr)
		}
		if normalize(got.stdout) != normalize(want) {
			t.Fatalf("forge help %s snapshot drift; regenerate %s", cmd, snapshotPath)
		}
	}
}

func runBinaryWithHome(t *testing.T, root, bin, home string, args ...string) binaryResult {
	t.Helper()

	cmd := exec.Command(bin, args...)
	cmd.Dir = root

	env := withoutToolchainOverrides(os.Environ())
	env = append(env, "HOME="+home)
	env = append(env, "XDG_CONFIG_HOME="+filepath.Join(home, ".config"))
	env = append(env, "XDG_DATA_HOME="+filepath.Join(home, ".local", "share"))
	cmd.Env = env

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	result := binaryResult{exitCode: 0}
	if err := cmd.Run(); err != nil {
		exitErr, ok := err.(*exec.ExitError)
		if !ok {
			t.Fatalf("run %s %v: %v", bin, args, err)
		}
		result.exitCode = exitErr.ExitCode()
	}
	result.stdout = stdout.String()
	result.stderr = stderr.String()
	return result
}

