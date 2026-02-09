package parity

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
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
	assertExists(t, filepath.Join(out, "normalized", "drift-report.json"))
	assertExists(t, filepath.Join(out, "normalized", "drift-triage.md"))
	assertExists(t, filepath.Join(out, "normalized", "parity-alert-routing.json"))
	assertExists(t, filepath.Join(out, "normalized", "parity-alert-routing.md"))
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

	var drift struct {
		SchemaVersion string `json:"schema_version"`
		Summary       struct {
			Total           int  `json:"total"`
			MissingExpected int  `json:"missing_expected"`
			Mismatched      int  `json:"mismatched"`
			Unexpected      int  `json:"unexpected"`
			HasDrift        bool `json:"has_drift"`
		} `json:"summary"`
		Items []struct {
			Priority  string `json:"priority"`
			DriftType string `json:"drift_type"`
			Path      string `json:"path"`
			Owner     string `json:"owner"`
		} `json:"items"`
	}
	driftBody, err := os.ReadFile(filepath.Join(out, "normalized", "drift-report.json"))
	if err != nil {
		t.Fatalf("read drift report: %v", err)
	}
	if err := json.Unmarshal(driftBody, &drift); err != nil {
		t.Fatalf("unmarshal drift report: %v", err)
	}
	if drift.SchemaVersion != "parity.drift.v1" {
		t.Fatalf("unexpected drift schema: %q", drift.SchemaVersion)
	}
	if !drift.Summary.HasDrift || drift.Summary.Total != 2 || drift.Summary.Mismatched != 1 || drift.Summary.Unexpected != 1 {
		t.Fatalf("unexpected drift summary: %+v", drift.Summary)
	}
	if len(drift.Items) != 2 {
		t.Fatalf("unexpected drift item count: %d", len(drift.Items))
	}
	if drift.Items[0].Owner != "parity-infra" || drift.Items[1].Owner != "parity-infra" {
		t.Fatalf("expected parity-infra owners, got %+v", drift.Items)
	}

	mdBody, err := os.ReadFile(filepath.Join(out, "normalized", "drift-triage.md"))
	if err != nil {
		t.Fatalf("read drift triage: %v", err)
	}
	text := string(mdBody)
	for _, want := range []string{
		"# Parity Drift Triage",
		"| Priority | Drift type | Path | Owner | Root cause | Action | Tracking issue |",
		"`b.txt`",
		"`extra.txt`",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("drift triage missing %q", want)
		}
	}

	routingBody, err := os.ReadFile(filepath.Join(out, "normalized", "parity-alert-routing.md"))
	if err != nil {
		t.Fatalf("read routing md: %v", err)
	}
	if !strings.Contains(string(routingBody), "| parity-infra | 2 |") {
		t.Fatalf("routing markdown missing parity-infra route: %s", routingBody)
	}
}

func TestWriteDiffArtifactsAssignsOwnersByPath(t *testing.T) {
	t.Parallel()

	expected := t.TempDir()
	actual := t.TempDir()
	out := t.TempDir()

	mustWriteFile(t, filepath.Join(expected, "forge", "root", "help.txt"), "expected-cli\n")
	mustWriteFile(t, filepath.Join(actual, "forge", "root", "help.txt"), "actual-cli\n")
	mustWriteFile(t, filepath.Join(expected, "forged", "proto-wire", "summary.json"), "{\"ok\":true}\n")
	mustWriteFile(t, filepath.Join(actual, "forged", "proto-wire", "summary.json"), "{\"ok\":false}\n")

	report, err := WriteDiffArtifacts(expected, actual, out)
	if err != nil {
		t.Fatalf("write diff artifacts: %v", err)
	}
	if !report.HasDrift() {
		t.Fatalf("expected drift report, got %+v", report)
	}

	var drift struct {
		Items []struct {
			Path  string `json:"path"`
			Owner string `json:"owner"`
		} `json:"items"`
	}
	driftBody, err := os.ReadFile(filepath.Join(out, "normalized", "drift-report.json"))
	if err != nil {
		t.Fatalf("read drift report: %v", err)
	}
	if err := json.Unmarshal(driftBody, &drift); err != nil {
		t.Fatalf("unmarshal drift report: %v", err)
	}

	ownersByPath := map[string]string{}
	for _, item := range drift.Items {
		ownersByPath[item.Path] = item.Owner
	}
	if got := ownersByPath["forge/root/help.txt"]; got != "forge-cli" {
		t.Fatalf("forge/root owner mismatch: got %q", got)
	}
	if got := ownersByPath["forged/proto-wire/summary.json"]; got != "forge-daemon" {
		t.Fatalf("forged/proto-wire owner mismatch: got %q", got)
	}
}

func assertExists(t *testing.T, path string) {
	t.Helper()
	if _, err := os.Stat(path); err != nil {
		t.Fatalf("missing artifact %s: %v", path, err)
	}
}
