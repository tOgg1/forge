package paritydash

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestBuildSummarizesOutcomes(t *testing.T) {
	now := time.Date(2026, 2, 9, 12, 0, 0, 0, time.UTC)

	d, err := Build(Input{
		Run: RunInfo{Workflow: "CI", SHA: "deadbeef"},
		Checks: []InputCheck{
			{ID: "oracle", Name: "Oracle", Outcome: "success"},
			{ID: "schema", Name: "Schema", Outcome: "failure"},
			{ID: "diff", Name: "Diff", Outcome: "skipped"},
			{ID: "misc", Name: "Misc", Outcome: ""},
		},
	}, now)
	if err != nil {
		t.Fatalf("build: %v", err)
	}

	if d.SchemaVersion != "paritydash.v1" {
		t.Fatalf("schema_version: %q", d.SchemaVersion)
	}
	if d.GeneratedAt != now.Format(time.RFC3339) {
		t.Fatalf("generated_at: %q", d.GeneratedAt)
	}

	if d.Summary.Total != 4 || d.Summary.Passed != 1 || d.Summary.Failed != 1 || d.Summary.Skipped != 1 || d.Summary.Unknown != 1 {
		t.Fatalf("summary: %+v", d.Summary)
	}
	if d.Summary.Status != "fail" {
		t.Fatalf("status: %q", d.Summary.Status)
	}
}

func TestWriteFilesWritesJSONAndMarkdown(t *testing.T) {
	now := time.Date(2026, 2, 9, 12, 0, 0, 0, time.UTC)
	d, err := Build(Input{
		Checks: []InputCheck{
			{ID: "oracle", Outcome: "success"},
		},
	}, now)
	if err != nil {
		t.Fatalf("build: %v", err)
	}

	dir := t.TempDir()
	out := filepath.Join(dir, "dash")
	if err := WriteFiles(out, d, true); err != nil {
		t.Fatalf("write: %v", err)
	}

	jb, err := os.ReadFile(filepath.Join(out, "parity-dashboard.json"))
	if err != nil {
		t.Fatalf("read json: %v", err)
	}
	var got Dashboard
	if err := json.Unmarshal(jb, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if got.Summary.Status != "pass" || got.Summary.Passed != 1 {
		t.Fatalf("unexpected json content: %+v", got.Summary)
	}

	if _, err := os.Stat(filepath.Join(out, "parity-dashboard.md")); err != nil {
		t.Fatalf("stat md: %v", err)
	}
}

