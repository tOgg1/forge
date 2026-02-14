package parity

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

type baselineRefreshReport struct {
	ApprovalRef   string `json:"approval_ref"`
	Mode          string `json:"mode"`
	DriftDetected bool   `json:"drift_detected"`
}

func TestBaselineRefreshScriptDryRunPass(t *testing.T) {
	scriptPath := filepath.Join(workspaceRoot(t), "scripts", "rust-baseline-refresh.sh")
	stub := writeSnapshotStub(t)
	outDir := t.TempDir()

	cmd := exec.Command(scriptPath, "--approval", "forge-7sd", "--out-dir", outDir)
	cmd.Env = append(os.Environ(), "RUST_BASELINE_SNAPSHOT_BIN="+stub)
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("run baseline refresh dry-run: %v\n%s", err, output)
	}

	report := readBaselineReport(t, outDir)
	if report.Mode != "dry-run" {
		t.Fatalf("unexpected mode: %s", report.Mode)
	}
	if report.ApprovalRef != "forge-7sd" {
		t.Fatalf("unexpected approval ref: %s", report.ApprovalRef)
	}
	if report.DriftDetected {
		t.Fatal("expected no drift")
	}
}

func TestBaselineRefreshScriptDryRunFailAndAllowDrift(t *testing.T) {
	scriptPath := filepath.Join(workspaceRoot(t), "scripts", "rust-baseline-refresh.sh")
	stub := writeSnapshotStub(t)
	outDir := t.TempDir()

	cmd := exec.Command(scriptPath, "--approval", "forge-7sd", "--out-dir", outDir)
	cmd.Env = append(os.Environ(),
		"RUST_BASELINE_SNAPSHOT_BIN="+stub,
		"FAKE_SNAPSHOT_FAIL=1",
	)
	output, err := cmd.CombinedOutput()
	if err == nil {
		t.Fatalf("expected dry-run failure\n%s", output)
	}

	report := readBaselineReport(t, outDir)
	if !report.DriftDetected {
		t.Fatal("expected drift in report")
	}

	outDirAllow := t.TempDir()
	cmd = exec.Command(scriptPath, "--approval", "forge-7sd", "--allow-drift", "--out-dir", outDirAllow)
	cmd.Env = append(os.Environ(),
		"RUST_BASELINE_SNAPSHOT_BIN="+stub,
		"FAKE_SNAPSHOT_FAIL=1",
	)
	output, err = cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("expected allow-drift success: %v\n%s", err, output)
	}

	report = readBaselineReport(t, outDirAllow)
	if !report.DriftDetected {
		t.Fatal("expected drift in allow-drift report")
	}
}

func TestBaselineRefreshScriptRejectsInvalidApproval(t *testing.T) {
	scriptPath := filepath.Join(workspaceRoot(t), "scripts", "rust-baseline-refresh.sh")
	stub := writeSnapshotStub(t)
	outDir := t.TempDir()

	cmd := exec.Command(scriptPath, "--approval", "bad ref", "--out-dir", outDir)
	cmd.Env = append(os.Environ(), "RUST_BASELINE_SNAPSHOT_BIN="+stub)
	output, err := cmd.CombinedOutput()
	if err == nil {
		t.Fatalf("expected invalid approval failure\n%s", output)
	}
}

func writeSnapshotStub(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "snapshot-stub.sh")
	script := `#!/usr/bin/env bash
set -euo pipefail
out_dir="${1:-}"
mkdir -p "$out_dir"
echo "stub" > "$out_dir/stub.txt"
if [[ "${FAKE_SNAPSHOT_FAIL:-0}" == "1" && "${2:-}" == "--check" ]]; then
  exit 1
fi
`
	if err := os.WriteFile(path, []byte(script), 0o755); err != nil {
		t.Fatalf("write snapshot stub: %v", err)
	}
	return path
}

func readBaselineReport(t *testing.T, outDir string) baselineRefreshReport {
	t.Helper()
	path := filepath.Join(outDir, "baseline-refresh-report.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read report: %v", err)
	}

	var report baselineRefreshReport
	if err := json.Unmarshal(data, &report); err != nil {
		t.Fatalf("decode report: %v", err)
	}
	return report
}
