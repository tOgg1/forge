package parity

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestRunLoopLifecycleHarnessNoDriftWithNormalization(t *testing.T) {
	t.Parallel()

	tmp := t.TempDir()
	fixture := filepath.Join(tmp, "fixture")
	if err := os.MkdirAll(fixture, 0o755); err != nil {
		t.Fatalf("mkdir fixture: %v", err)
	}
	if err := os.WriteFile(filepath.Join(fixture, "PROMPT.md"), []byte("prompt"), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	goBin := filepath.Join(tmp, "go-cli.sh")
	rustBin := filepath.Join(tmp, "rust-cli.sh")
	writeScript(t, goBin, fakeGoScript(false))
	writeScript(t, rustBin, fakeRustScript(false))

	scenario, err := LoadLifecycleScenario(filepath.Join("testdata", "lifecycle_harness", "scenario.json"))
	if err != nil {
		t.Fatalf("load scenario fixture: %v", err)
	}

	report, err := RunLoopLifecycleHarness(context.Background(), LifecycleHarnessConfig{
		GoBinary:   goBin,
		RustBinary: rustBin,
		FixtureDir: fixture,
		Scenario:   scenario,
		Timeout:    5 * time.Second,
	})
	if err != nil {
		t.Fatalf("run harness: %v", err)
	}
	if report.HasDrift() {
		t.Fatalf("expected no drift, got %+v", report.Steps)
	}
	if len(report.Steps) != 3 {
		t.Fatalf("expected 3 steps, got %d", len(report.Steps))
	}
	if report.Steps[2].Go.Stdout != "1\n" || report.Steps[2].Rust.Stdout != "1\n" {
		t.Fatalf("expected isolated fixture copies, got go=%q rust=%q", report.Steps[2].Go.Stdout, report.Steps[2].Rust.Stdout)
	}
}

func TestRunLoopLifecycleHarnessDetectsDrift(t *testing.T) {
	t.Parallel()

	tmp := t.TempDir()
	goBin := filepath.Join(tmp, "go-cli.sh")
	rustBin := filepath.Join(tmp, "rust-cli.sh")
	writeScript(t, goBin, fakeGoScript(false))
	writeScript(t, rustBin, fakeRustScript(true))

	scenario := LifecycleScenario{
		Name: "loop-lifecycle-drift",
		Steps: []LifecycleStep{
			{Name: "ps", Args: []string{"ps"}, StdoutFormat: FormatText},
		},
	}

	report, err := RunLoopLifecycleHarness(context.Background(), LifecycleHarnessConfig{
		GoBinary:   goBin,
		RustBinary: rustBin,
		FixtureDir: t.TempDir(),
		Scenario:   scenario,
		Timeout:    5 * time.Second,
	})
	if err != nil {
		t.Fatalf("run harness: %v", err)
	}
	if !report.HasDrift() {
		t.Fatalf("expected drift, got %+v", report.Steps)
	}
	if !report.Steps[0].HasDrift {
		t.Fatalf("expected drift on first step")
	}
}

func TestLoadLifecycleScenarioValidation(t *testing.T) {
	t.Parallel()

	tmp := t.TempDir()
	valid := filepath.Join(tmp, "valid.json")
	invalid := filepath.Join(tmp, "invalid.json")

	validBody := `{
  "name": "ok",
  "steps": [
    {"name": "up", "args": ["up"], "stdout_format": "json"},
    {"name": "ps", "args": ["ps"]}
  ]
}`
	if err := os.WriteFile(valid, []byte(validBody), 0o644); err != nil {
		t.Fatalf("write valid scenario: %v", err)
	}

	if _, err := LoadLifecycleScenario(valid); err != nil {
		t.Fatalf("load valid scenario: %v", err)
	}

	invalidBody := `{"steps":[{"name":"bad","args":["ps"],"stdout_format":"yaml"}]}`
	if err := os.WriteFile(invalid, []byte(invalidBody), 0o644); err != nil {
		t.Fatalf("write invalid scenario: %v", err)
	}
	if _, err := LoadLifecycleScenario(invalid); err == nil {
		t.Fatalf("expected invalid format error")
	}
}

func writeScript(t *testing.T, path, body string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(body), 0o755); err != nil {
		t.Fatalf("write script: %v", err)
	}
}

func fakeGoScript(forceDrift bool) string {
	psLine := "loop 20260210-120001-1234 at 2026-02-10T12:00:01Z path /Users/alex/work/repo\n"
	if forceDrift {
		psLine = "loop drift-go\n"
	}
	return strings.Join([]string{
		"#!/usr/bin/env bash",
		"set -euo pipefail",
		"cmd=\"${1:-}\"",
		"case \"$cmd\" in",
		"  up)",
		"    echo '{\"id\":\"20260210-120000-1234\",\"created_at\":\"2026-02-10T12:00:00Z\",\"repo_path\":\"/Users/alex/work/repo\",\"items\":[{\"name\":\"b\"},{\"name\":\"a\"}]}'",
		"    ;;",
		"  ps)",
		"    printf '" + shellEscapeSingle(psLine) + "'",
		"    ;;",
		"  touch)",
		"    n=0",
		"    [[ -f .counter ]] && n=\"$(cat .counter)\"",
		"    n=$((n+1))",
		"    echo \"$n\" > .counter",
		"    echo \"$n\"",
		"    ;;",
		"  *)",
		"    echo \"unknown command: $cmd\" >&2",
		"    exit 2",
		"    ;;",
		"esac",
		"",
	}, "\n")
}

func fakeRustScript(forceDrift bool) string {
	psLine := "loop 20260210-120009-9999 at 2026-02-10T12:00:09Z path /Users/trmd/Code/oss--forge/repos/forge  \n"
	if forceDrift {
		psLine = "loop drift-rust\n"
	}
	return strings.Join([]string{
		"#!/usr/bin/env bash",
		"set -euo pipefail",
		"cmd=\"${1:-}\"",
		"case \"$cmd\" in",
		"  up)",
		"    echo '{\"items\":[{\"name\":\"a\"},{\"name\":\"b\"}],\"repo_path\":\"/Users/trmd/Code/oss--forge/repos/forge\",\"created_at\":\"2026-02-10T12:00:09Z\",\"id\":\"20260210-120009-9999\"}'",
		"    ;;",
		"  ps)",
		"    printf '" + shellEscapeSingle(psLine) + "'",
		"    ;;",
		"  touch)",
		"    n=0",
		"    [[ -f .counter ]] && n=\"$(cat .counter)\"",
		"    n=$((n+1))",
		"    echo \"$n\" > .counter",
		"    echo \"$n\"",
		"    ;;",
		"  *)",
		"    echo \"unknown command: $cmd\" >&2",
		"    exit 2",
		"    ;;",
		"esac",
		"",
	}, "\n")
}

func shellEscapeSingle(s string) string {
	return strings.ReplaceAll(s, "'", "'\"'\"'")
}
