package parity

import (
	"os"
	"path/filepath"
	"testing"
)

func TestCompareBytesNormalizesTextDriftNoise(t *testing.T) {
	t.Parallel()

	expected := []byte("at 2026-02-09T12:00:00Z id 20260209-120000-1234 path /Users/alex/work/repo/file.txt\n")
	actual := []byte("at 2026-02-09T12:00:09Z id 20260209-120009-9999 path /Users/trmd/Code/oss--forge/repos/forge/file.txt\n")

	got, err := CompareBytes(expected, actual, DefaultCompareOptions(FormatText))
	if err != nil {
		t.Fatalf("compare bytes: %v", err)
	}
	if !got.Equal {
		t.Fatalf("expected normalized text parity, got mismatch\nexpected=%s\nactual=%s", got.NormalizedExpected, got.NormalizedActual)
	}
}

func TestCompareBytesCanonicalizesJSONOrder(t *testing.T) {
	t.Parallel()

	expected := []byte(`{"items":[{"name":"a","n":1},{"name":"b","n":2}]}`)
	actual := []byte(`{"items":[{"n":2,"name":"b"},{"n":1,"name":"a"}]}`)

	got, err := CompareBytes(expected, actual, DefaultCompareOptions(FormatJSON))
	if err != nil {
		t.Fatalf("compare bytes: %v", err)
	}
	if !got.Equal {
		t.Fatalf("expected normalized json parity, got mismatch\nexpected=%s\nactual=%s", got.NormalizedExpected, got.NormalizedActual)
	}
}

func TestRunFixtureSetGoldenSelfCheck(t *testing.T) {
	t.Parallel()

	report, err := RunFixtureSet("testdata/golden/selfcheck", DefaultCompareOptions(FormatJSON))
	if err != nil {
		t.Fatalf("run fixture set: %v", err)
	}
	if report.HasDrift() {
		t.Fatalf("expected golden self-check parity, got drift: %+v", report.Comparisons)
	}
	if len(report.Comparisons) == 0 {
		t.Fatalf("expected at least one fixture comparison")
	}
}

func TestLoadFixturePair(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	mustWriteFixture(t, filepath.Join(root, "expected", "sample.txt"), "expected")
	mustWriteFixture(t, filepath.Join(root, "actual", "sample.txt"), "actual")

	expected, actual, err := LoadFixturePair(root, "sample.txt")
	if err != nil {
		t.Fatalf("load fixture pair: %v", err)
	}
	if string(expected) != "expected" || string(actual) != "actual" {
		t.Fatalf("unexpected fixture payloads: expected=%q actual=%q", expected, actual)
	}
}

func mustWriteFixture(t *testing.T, path, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}
}
