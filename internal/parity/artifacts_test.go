package parity

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestWriteDiffArtifactsSchema(t *testing.T) {
	t.Parallel()

	expected := t.TempDir()
	actual := t.TempDir()
	out := t.TempDir()

	mustWriteFile(t, filepath.Join(expected, "a.txt"), "same\n")
	mustWriteFile(t, filepath.Join(expected, "b.txt"), "expected\n")
	mustWriteFile(t, filepath.Join(actual, "a.txt"), "same\n")
	mustWriteFile(t, filepath.Join(actual, "b.txt"), "actual\n")
	mustWriteFile(t, filepath.Join(actual, "extra.txt"), "extra\n")

	report, err := WriteDiffArtifacts(expected, actual, out)
	if err != nil {
		t.Fatalf("write diff artifacts: %v", err)
	}
	if !report.HasDrift() {
		t.Fatalf("expected drift report, got %+v", report)
	}

	assertExists(t, filepath.Join(out, "expected", "a.txt"))
	assertExists(t, filepath.Join(out, "actual", "a.txt"))
	assertExists(t, filepath.Join(out, "normalized", "report.json"))
	assertExists(t, filepath.Join(out, "normalized", "diffs", "b.txt.diff"))
	assertExists(t, filepath.Join(out, "normalized", "diffs", "extra.txt.unexpected.diff"))

	var manifest struct {
		MissingExpected []string `json:"missing_expected"`
		Mismatched      []string `json:"mismatched"`
		Unexpected      []string `json:"unexpected"`
	}
	body, err := os.ReadFile(filepath.Join(out, "normalized", "report.json"))
	if err != nil {
		t.Fatalf("read manifest: %v", err)
	}
	if err := json.Unmarshal(body, &manifest); err != nil {
		t.Fatalf("unmarshal manifest: %v", err)
	}
	if len(manifest.Mismatched) != 1 || manifest.Mismatched[0] != "b.txt" {
		t.Fatalf("unexpected mismatch manifest: %+v", manifest)
	}
	if len(manifest.Unexpected) != 1 || manifest.Unexpected[0] != "extra.txt" {
		t.Fatalf("unexpected unexpected-manifest: %+v", manifest)
	}
}

func assertExists(t *testing.T, path string) {
	t.Helper()
	if _, err := os.Stat(path); err != nil {
		t.Fatalf("missing artifact %s: %v", path, err)
	}
}
