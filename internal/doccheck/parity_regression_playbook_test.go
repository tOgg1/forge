package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestParityRegressionPlaybookPinsDriftTriageArtifacts(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	playbook := readFile(t, filepath.Join(root, "docs/parity-regression-playbook.md"))
	ciWorkflow := readFile(t, filepath.Join(root, ".github/workflows/ci.yml"))
	nightlyWorkflow := readFile(t, filepath.Join(root, ".github/workflows/parity-nightly.yml"))

	for _, want := range []string{
		"cmd/parity-artifacts",
		"normalized/report.json",
		"normalized/drift-report.json",
		"normalized/drift-triage.md",
		"Owner",
		"Root cause",
		"Action",
		"Tracking issue",
	} {
		if !strings.Contains(playbook, want) {
			t.Fatalf("parity-regression-playbook.md drift: missing %q", want)
		}
	}

	for _, workflow := range []struct {
		name string
		body string
	}{
		{name: "ci.yml", body: ciWorkflow},
		{name: "parity-nightly.yml", body: nightlyWorkflow},
	} {
		if !strings.Contains(workflow.body, "cmd/parity-artifacts") {
			t.Fatalf("%s drift: missing cmd/parity-artifacts execution", workflow.name)
		}
		if !strings.Contains(workflow.body, "name: parity-diff") {
			t.Fatalf("%s drift: missing parity-diff artifact upload", workflow.name)
		}
	}
}
