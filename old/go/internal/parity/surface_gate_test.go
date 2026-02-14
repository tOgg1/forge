package parity

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tOgg1/forge/internal/cli"
)

// surfaceDriftBaseline is the committed count of known parity gaps between Go
// and Rust command surfaces.  The gate test fails if the actual drift count
// EXCEEDS this number (regression).  Lower it as gaps are resolved; the test
// will also fail if you lower it below the real count.
const surfaceDriftBaseline = 124

// TestSurfaceGateGoVsRust is the primary parity gate.  It compares Go and
// Rust forge CLI command/flag/alias surfaces, writes artifacts for CI, and
// fails on regression (new drifts beyond the committed baseline).
func TestSurfaceGateGoVsRust(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping surface gate (requires Rust build)")
	}
	// Not parallel — cli.CommandSurfaceJSON walks Cobra's global rootCmd.

	goManifest := goSurface(t)
	rustManifest := rustSurface(t)

	report := CompareSurfaces(goManifest, rustManifest)

	// Write artifacts for debugging / CI.
	artifactDir := filepath.Join(t.TempDir(), "surface-gate")
	if err := os.MkdirAll(artifactDir, 0o755); err != nil {
		t.Fatalf("mkdir artifact dir: %v", err)
	}
	writeJSON(t, filepath.Join(artifactDir, "go-surface.json"), goManifest)
	writeJSON(t, filepath.Join(artifactDir, "rust-surface.json"), rustManifest)
	writeJSON(t, filepath.Join(artifactDir, "report.json"), report)

	t.Logf("surface comparison: %s", report.Summary())
	t.Logf("artifacts written to: %s", artifactDir)

	if report.HasDrift() {
		t.Logf("\n%s", FormatDriftReport(report))
	}

	driftCount := len(report.Drifts)

	if driftCount > surfaceDriftBaseline {
		t.Fatalf("surface parity REGRESSION: drift count %d exceeds baseline %d (+%d new gaps)",
			driftCount, surfaceDriftBaseline, driftCount-surfaceDriftBaseline)
	}

	if driftCount < surfaceDriftBaseline {
		t.Logf("NOTE: drift count %d is below baseline %d — lower surfaceDriftBaseline to %d",
			driftCount, surfaceDriftBaseline, driftCount)
	}
}

// TestSurfaceGateStrict fails on ANY drift.  Run this when targeting zero
// parity gaps: go test ./internal/parity -run TestSurfaceGateStrict
func TestSurfaceGateStrict(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping strict surface gate (requires Rust build)")
	}
	// Only run when explicitly requested via -run flag, not as part of ./...
	if os.Getenv("SURFACE_GATE_STRICT") == "" {
		t.Skip("set SURFACE_GATE_STRICT=1 to enable strict mode")
	}
	// Not parallel — cli.CommandSurfaceJSON walks Cobra's global rootCmd.

	goManifest := goSurface(t)
	rustManifest := rustSurface(t)

	report := CompareSurfaces(goManifest, rustManifest)

	if report.HasDrift() {
		t.Logf("\n%s", FormatDriftReport(report))
		t.Fatalf("strict surface parity gate FAILED: %d drift(s) detected", len(report.Drifts))
	}
}

// TestSurfaceSnapshotReproducible verifies that the Go surface extraction is
// deterministic across two runs.
func TestSurfaceSnapshotReproducible(t *testing.T) {
	// Not parallel — cli.CommandSurfaceJSON walks Cobra's global rootCmd which
	// is not safe for concurrent access.
	a, err := cli.CommandSurfaceJSON()
	if err != nil {
		t.Fatalf("first extraction: %v", err)
	}
	b, err := cli.CommandSurfaceJSON()
	if err != nil {
		t.Fatalf("second extraction: %v", err)
	}
	if string(a) != string(b) {
		t.Fatalf("go surface is not deterministic between runs")
	}
}

// ---------- helpers ----------

func goSurface(t *testing.T) SurfaceManifest {
	t.Helper()
	data, err := cli.CommandSurfaceJSON()
	if err != nil {
		t.Fatalf("extract go surface: %v", err)
	}
	var m SurfaceManifest
	if err := json.Unmarshal(data, &m); err != nil {
		t.Fatalf("unmarshal go surface: %v", err)
	}
	return m
}

func rustSurface(t *testing.T) SurfaceManifest {
	t.Helper()
	root := workspaceRoot(t)
	rustDir := root

	// Build the Rust forge-cli binary.
	buildCmd := exec.Command("cargo", "build", "-p", "forge-cli", "--quiet")
	buildCmd.Dir = rustDir
	buildCmd.Env = cleanRustEnv()
	if out, err := buildCmd.CombinedOutput(); err != nil {
		t.Fatalf("build rust forge-cli: %v\n%s", err, out)
	}

	// Find the binary.
	bin := findRustBin(t, rustDir, "forge-cli")

	// Run root --help.
	rootHelp := runBin(t, bin, "--help")

	// Parse commands from root help.
	commands := ParseRootHelp(rootHelp)
	globalFlags := ParseGlobalFlagsFromHelp(rootHelp)

	// For each command, get its help and parse flags/subcommands.
	for i, cmd := range commands {
		cmdHelp := runBin(t, bin, cmd.Name, "--help")
		// The help might be on stderr (error-style) or stdout.
		if cmdHelp == "" {
			cmdHelp = runBinStderr(t, bin, cmd.Name, "--help")
		}
		flags, subs := ParseCommandHelp(cmdHelp)
		commands[i].Flags = flags
		commands[i].Subcommands = subs

		// Recurse one level for subcommands.
		for j, sub := range subs {
			subHelp := runBin(t, bin, cmd.Name, sub.Name, "--help")
			if subHelp == "" {
				subHelp = runBinStderr(t, bin, cmd.Name, sub.Name, "--help")
			}
			subFlags, subSubs := ParseCommandHelp(subHelp)
			commands[i].Subcommands[j].Flags = subFlags
			commands[i].Subcommands[j].Subcommands = subSubs
		}
	}

	// Populate aliases from the Rust match dispatcher by reading lib.rs.
	populateRustAliases(t, root, commands)

	return SurfaceManifest{
		CLI:         "forge",
		GlobalFlags: globalFlags,
		Commands:    commands,
	}
}

func findRustBin(t *testing.T, rustDir, name string) string {
	t.Helper()
	// Try target/debug first.
	bin := filepath.Join(rustDir, "target", "debug", name)
	if _, err := os.Stat(bin); err == nil {
		return bin
	}
	// Try target/release.
	bin = filepath.Join(rustDir, "target", "release", name)
	if _, err := os.Stat(bin); err == nil {
		return bin
	}
	t.Fatalf("rust binary %q not found in %s/target/{debug,release}", name, rustDir)
	return ""
}

func runBin(t *testing.T, bin string, args ...string) string {
	t.Helper()
	cmd := exec.Command(bin, args...)
	cmd.Env = append(os.Environ(),
		"FORGE_DB_PATH=:memory:",
		"FORGE_DATA_DIR="+t.TempDir(),
	)
	out, _ := cmd.Output()
	return string(out)
}

func runBinStderr(t *testing.T, bin string, args ...string) string {
	t.Helper()
	cmd := exec.Command(bin, args...)
	cmd.Env = append(os.Environ(),
		"FORGE_DB_PATH=:memory:",
		"FORGE_DATA_DIR="+t.TempDir(),
	)
	out, _ := cmd.CombinedOutput()
	return string(out)
}

// populateRustAliases reads the Rust lib.rs match arms to find command aliases.
// e.g. `Some("logs") | Some("log") =>` means "logs" has alias "log".
func populateRustAliases(t *testing.T, root string, cmds []SurfaceCommand) {
	t.Helper()
	libPath := filepath.Join(root, "crates", "forge-cli", "src", "lib.rs")
	data, err := os.ReadFile(libPath)
	if err != nil {
		t.Logf("warning: could not read lib.rs for alias extraction: %v", err)
		return
	}
	content := string(data)

	// Build an index of commands by name for fast lookup.
	cmdIndex := make(map[string]int, len(cmds))
	for i, c := range cmds {
		cmdIndex[c.Name] = i
	}

	// Parse match arms like: Some("logs") | Some("log") =>
	for _, line := range strings.Split(content, "\n") {
		trimmed := strings.TrimSpace(line)
		if !strings.Contains(trimmed, "Some(\"") || !strings.Contains(trimmed, "=>") {
			continue
		}
		// Extract all Some("...") values.
		var names []string
		rest := trimmed
		for {
			idx := strings.Index(rest, "Some(\"")
			if idx < 0 {
				break
			}
			rest = rest[idx+6:]
			end := strings.Index(rest, "\")")
			if end < 0 {
				break
			}
			names = append(names, rest[:end])
			rest = rest[end+2:]
		}
		if len(names) < 2 {
			continue
		}
		// First name is the primary; rest are aliases.
		primary := names[0]
		if i, ok := cmdIndex[primary]; ok {
			for _, alias := range names[1:] {
				if alias != primary {
					cmds[i].Aliases = append(cmds[i].Aliases, alias)
				}
			}
		}
	}
}

func cleanRustEnv() []string {
	var env []string
	for _, e := range os.Environ() {
		// Strip GOROOT/GOTOOLDIR to avoid Go interference.
		if strings.HasPrefix(e, "GOROOT=") || strings.HasPrefix(e, "GOTOOLDIR=") {
			continue
		}
		env = append(env, e)
	}
	return env
}

func writeJSON(t *testing.T, path string, v interface{}) {
	t.Helper()
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		t.Fatalf("marshal json: %v", err)
	}
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}
