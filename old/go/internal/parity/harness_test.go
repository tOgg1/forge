package parity

import (
	"os"
	"path/filepath"
	"testing"
)

func TestOracleFixtureParity(t *testing.T) {
	t.Parallel()

	report, err := CompareTrees("testdata/oracle/expected", "testdata/oracle/actual")
	if err != nil {
		t.Fatalf("compare trees: %v", err)
	}
	if report.HasDrift() {
		t.Fatalf("expected no drift, got %+v", report)
	}
}

func TestCompareTreesDetectsSyntheticDrift(t *testing.T) {
	t.Parallel()

	expected := t.TempDir()
	actual := t.TempDir()

	mustWriteFile(t, filepath.Join(expected, "forge/help.txt"), "forge help\ncmd: up\n")
	mustWriteFile(t, filepath.Join(actual, "forge/help.txt"), "forge help\ncmd: down\n")

	report, err := CompareTrees(expected, actual)
	if err != nil {
		t.Fatalf("compare trees: %v", err)
	}

	if len(report.Mismatched) != 1 || report.Mismatched[0] != "forge/help.txt" {
		t.Fatalf("expected one mismatch for synthetic drift, got %+v", report)
	}
}

func TestCompareTreesDetectsMissingAndUnexpected(t *testing.T) {
	t.Parallel()

	expected := t.TempDir()
	actual := t.TempDir()

	mustWriteFile(t, filepath.Join(expected, "forge/help.txt"), "forge help\n")
	mustWriteFile(t, filepath.Join(actual, "extra.txt"), "unexpected\n")

	report, err := CompareTrees(expected, actual)
	if err != nil {
		t.Fatalf("compare trees: %v", err)
	}

	if len(report.MissingExpected) != 1 || report.MissingExpected[0] != "forge/help.txt" {
		t.Fatalf("expected missing file drift, got %+v", report)
	}
	if len(report.Unexpected) != 1 || report.Unexpected[0] != "extra.txt" {
		t.Fatalf("expected unexpected file drift, got %+v", report)
	}
}

func mustWriteFile(t *testing.T, path, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("write file: %v", err)
	}
}
