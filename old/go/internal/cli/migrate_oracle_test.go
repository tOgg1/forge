package cli

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"testing"
)

type migrateOracleReport struct {
	Steps []migrateOracleStep `json:"steps"`
}

type migrateOracleStep struct {
	Name     string   `json:"name"`
	Args     []string `json:"args,omitempty"`
	Stdout   string   `json:"stdout,omitempty"`
	Stderr   string   `json:"stderr,omitempty"`
	ExitCode int      `json:"exit_code"`
}

func TestMigrateOracleScenarioMatchesFixture(t *testing.T) {
	repo := t.TempDir()
	cleanupConfig := withTempConfig(t, repo)
	defer cleanupConfig()

	withWorkingDir(t, repo, func() {
		restore := snapshotMigrateGlobals()
		defer restore()

		restoreLogging := silenceLogging(t)
		defer restoreLogging()

		// Stable output, no prompts.
		noColor = true
		quiet = false
		yesFlag = true
		nonInteractive = true
		watchMode = false
		sinceDur = ""

		var report migrateOracleReport

		step := func(name string, args []string, fn func() error) {
			stdout, stderr, err := captureStdoutStderr(fn)
			exitCode := 0
			if err != nil {
				exitCode = 1
			}
			report.Steps = append(report.Steps, migrateOracleStep{
				Name:     name,
				Args:     args,
				Stdout:   stdout,
				Stderr:   strings.TrimSpace(stderr),
				ExitCode: exitCode,
			})
			if err != nil {
				t.Fatalf("%s: %v\nstderr:\n%s", name, err, stderr)
			}
		}

		jsonOutput = false
		step("migrate status (table)", []string{"migrate", "status"}, func() error {
			return migrateStatusCmd.RunE(migrateStatusCmd, nil)
		})

		jsonOutput = true
		step("migrate status (json)", []string{"--json", "migrate", "status"}, func() error {
			return migrateStatusCmd.RunE(migrateStatusCmd, nil)
		})

		jsonOutput = false
		step("migrate version (text)", []string{"migrate", "version"}, func() error {
			return migrateVersionCmd.RunE(migrateVersionCmd, nil)
		})

		jsonOutput = true
		step("migrate version (json)", []string{"--json", "migrate", "version"}, func() error {
			return migrateVersionCmd.RunE(migrateVersionCmd, nil)
		})

		jsonOutput = false
		migrateVersion = 0
		step("migrate up (apply pending)", []string{"migrate", "up"}, func() error {
			return migrateUpCmd.RunE(migrateUpCmd, nil)
		})

		step("migrate up (no pending)", []string{"migrate", "up"}, func() error {
			return migrateUpCmd.RunE(migrateUpCmd, nil)
		})

		jsonOutput = false
		migrateSteps = 1
		step("migrate down (1)", []string{"migrate", "down"}, func() error {
			return migrateDownCmd.RunE(migrateDownCmd, nil)
		})

		// Move back to latest version via `--to`.
		db, err := openDatabaseNoMigrate()
		if err != nil {
			t.Fatalf("open db: %v", err)
		}
		version, err := db.SchemaVersion(context.Background())
		_ = db.Close()
		if err != nil {
			t.Fatalf("schema version: %v", err)
		}
		latest := version + 1
		migrateVersion = latest
		step("migrate up --to (latest)", []string{"migrate", "up", "--to", strconv.Itoa(latest)}, func() error {
			return migrateUpCmd.RunE(migrateUpCmd, nil)
		})

		got := prettyJSON(t, report) + "\n"
		goldenPath := migrateGoldenPath(t)

		if os.Getenv("FORGE_UPDATE_GOLDENS") == "1" {
			if err := os.MkdirAll(filepath.Dir(goldenPath), 0o755); err != nil {
				t.Fatalf("mkdir golden dir: %v", err)
			}
			if err := os.WriteFile(goldenPath, []byte(got), 0o644); err != nil {
				t.Fatalf("write golden: %v", err)
			}
			return
		}

		wantBytes, err := os.ReadFile(goldenPath)
		if err != nil {
			t.Fatalf("read golden: %v (set FORGE_UPDATE_GOLDENS=1 to generate)", err)
		}
		want := string(wantBytes)
		if got != want {
			t.Fatalf("migrate oracle fixture drift: %s (set FORGE_UPDATE_GOLDENS=1 to regenerate)\n--- want\n%s\n--- got\n%s", goldenPath, want, got)
		}
	})
}

func snapshotMigrateGlobals() func() {
	prevJSON := jsonOutput
	prevSteps := migrateSteps
	prevTo := migrateVersion
	return func() {
		jsonOutput = prevJSON
		migrateSteps = prevSteps
		migrateVersion = prevTo
	}
}

func migrateGoldenPath(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve test file path")
	}
	base := filepath.Dir(file)
	return filepath.Join(base, "testdata", "oracle", "migrate.json")
}
