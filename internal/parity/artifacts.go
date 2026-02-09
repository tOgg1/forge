package parity

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"github.com/pmezard/go-difflib/difflib"
)

// WriteDiffArtifacts writes expected/actual trees and normalized drift diffs.
func WriteDiffArtifacts(expectedDir, actualDir, outputDir string) (Report, error) {
	report, err := CompareTrees(expectedDir, actualDir)
	if err != nil {
		return report, err
	}

	expectedOut := filepath.Join(outputDir, "expected")
	actualOut := filepath.Join(outputDir, "actual")
	normalizedOut := filepath.Join(outputDir, "normalized")
	diffsOut := filepath.Join(normalizedOut, "diffs")

	for _, dir := range []string{expectedOut, actualOut, normalizedOut, diffsOut} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return report, err
		}
	}

	if err := copyTree(expectedDir, expectedOut); err != nil {
		return report, err
	}
	if err := copyTree(actualDir, actualOut); err != nil {
		return report, err
	}

	if err := writeManifest(filepath.Join(normalizedOut, "report.json"), report); err != nil {
		return report, err
	}
	if err := writeDriftReport(normalizedOut, report); err != nil {
		return report, err
	}

	expectedFiles, err := listFiles(expectedDir)
	if err != nil {
		return report, err
	}
	actualFiles, err := listFiles(actualDir)
	if err != nil {
		return report, err
	}

	for _, rel := range report.Mismatched {
		if err := writeMismatchDiff(diffsOut, rel, expectedFiles[rel], actualFiles[rel]); err != nil {
			return report, err
		}
	}
	for _, rel := range report.MissingExpected {
		if err := os.WriteFile(filepath.Join(diffsOut, slug(rel)+".missing.diff"), []byte("missing actual file: "+rel+"\n"), 0o644); err != nil {
			return report, err
		}
	}
	for _, rel := range report.Unexpected {
		if err := os.WriteFile(filepath.Join(diffsOut, slug(rel)+".unexpected.diff"), []byte("unexpected actual file: "+rel+"\n"), 0o644); err != nil {
			return report, err
		}
	}

	return report, nil
}

func writeMismatchDiff(outDir, rel, expectedPath, actualPath string) error {
	expectedBytes, err := os.ReadFile(expectedPath)
	if err != nil {
		return err
	}
	actualBytes, err := os.ReadFile(actualPath)
	if err != nil {
		return err
	}

	expectedLines := difflib.SplitLines(string(normalize(expectedBytes)))
	actualLines := difflib.SplitLines(string(normalize(actualBytes)))
	ud := difflib.UnifiedDiff{
		A:        expectedLines,
		B:        actualLines,
		FromFile: "expected/" + rel,
		ToFile:   "actual/" + rel,
		Context:  3,
	}
	text, err := difflib.GetUnifiedDiffString(ud)
	if err != nil {
		return err
	}

	return os.WriteFile(filepath.Join(outDir, slug(rel)+".diff"), []byte(text), 0o644)
}

func writeManifest(path string, report Report) error {
	data := struct {
		MissingExpected []string `json:"missing_expected"`
		Mismatched      []string `json:"mismatched"`
		Unexpected      []string `json:"unexpected"`
	}{
		MissingExpected: report.MissingExpected,
		Mismatched:      report.Mismatched,
		Unexpected:      report.Unexpected,
	}

	out, err := json.MarshalIndent(data, "", "  ")
	if err != nil {
		return err
	}
	out = append(out, '\n')
	return os.WriteFile(path, out, 0o644)
}

type driftReport struct {
	SchemaVersion string            `json:"schema_version"`
	Summary       driftSummary      `json:"summary"`
	TriageFormat  triageFormat      `json:"triage_format"`
	Items         []driftReportItem `json:"items"`
}

type driftSummary struct {
	Total           int    `json:"total"`
	MissingExpected int    `json:"missing_expected"`
	Mismatched      int    `json:"mismatched"`
	Unexpected      int    `json:"unexpected"`
	HasDrift        bool   `json:"has_drift"`
	Text            string `json:"text"`
}

type triageFormat struct {
	RequiredFields []string `json:"required_fields"`
}

type driftReportItem struct {
	Priority      string `json:"priority"`
	DriftType     string `json:"drift_type"`
	Path          string `json:"path"`
	Owner         string `json:"owner"`
	RootCause     string `json:"root_cause"`
	Action        string `json:"action"`
	TrackingIssue string `json:"tracking_issue"`
}

type alertRoutingReport struct {
	SchemaVersion string              `json:"schema_version"`
	Summary       alertRoutingSummary `json:"summary"`
	Routes        []alertOwnerRoute   `json:"routes"`
}

type alertRoutingSummary struct {
	TotalAlerts int  `json:"total_alerts"`
	Owners      int  `json:"owners"`
	HasUnowned  bool `json:"has_unowned"`
}

type alertOwnerRoute struct {
	Owner string   `json:"owner"`
	Count int      `json:"count"`
	Paths []string `json:"paths"`
}

func writeDriftReport(outDir string, report Report) error {
	items := make([]driftReportItem, 0, len(report.MissingExpected)+len(report.Mismatched)+len(report.Unexpected))
	appendItems := func(paths []string, priority, driftType string) {
		for _, rel := range paths {
			items = append(items, driftReportItem{
				Priority:      priority,
				DriftType:     driftType,
				Path:          rel,
				Owner:         ownerForDriftPath(rel),
				RootCause:     "TODO",
				Action:        "TODO",
				TrackingIssue: "TODO",
			})
		}
	}
	appendItems(report.MissingExpected, "P0", "missing_expected")
	appendItems(report.Mismatched, "P1", "mismatched")
	appendItems(report.Unexpected, "P1", "unexpected")
	slices.SortFunc(items, func(a, b driftReportItem) int {
		if a.Priority == b.Priority {
			if a.DriftType == b.DriftType {
				return strings.Compare(a.Path, b.Path)
			}
			return strings.Compare(a.DriftType, b.DriftType)
		}
		return strings.Compare(a.Priority, b.Priority)
	})

	rep := driftReport{
		SchemaVersion: "parity.drift.v1",
		Summary: driftSummary{
			Total:           len(items),
			MissingExpected: len(report.MissingExpected),
			Mismatched:      len(report.Mismatched),
			Unexpected:      len(report.Unexpected),
			HasDrift:        report.HasDrift(),
			Text:            DriftSummary(report),
		},
		TriageFormat: triageFormat{
			RequiredFields: []string{
				"priority",
				"drift_type",
				"path",
				"owner",
				"root_cause",
				"action",
				"tracking_issue",
			},
		},
		Items: items,
	}

	jb, err := json.MarshalIndent(rep, "", "  ")
	if err != nil {
		return err
	}
	jb = append(jb, '\n')
	if err := os.WriteFile(filepath.Join(outDir, "drift-report.json"), jb, 0o644); err != nil {
		return err
	}

	mb := []byte(renderDriftTriageMarkdown(rep))
	if err := os.WriteFile(filepath.Join(outDir, "drift-triage.md"), mb, 0o644); err != nil {
		return err
	}
	if err := writeAlertRoutingReport(outDir, items); err != nil {
		return err
	}

	return nil
}

func renderDriftTriageMarkdown(rep driftReport) string {
	var b strings.Builder
	fmt.Fprintf(&b, "# Parity Drift Triage\n\n")
	fmt.Fprintf(&b, "- Summary: %s\n", rep.Summary.Text)
	fmt.Fprintf(&b, "- Drift items: %d\n\n", rep.Summary.Total)

	if len(rep.Items) == 0 {
		fmt.Fprintf(&b, "_No drift detected._\n")
		return b.String()
	}

	fmt.Fprintf(&b, "## Queue\n\n")
	fmt.Fprintf(&b, "| Priority | Drift type | Path | Owner | Root cause | Action | Tracking issue |\n")
	fmt.Fprintf(&b, "|---|---|---|---|---|---|---|\n")
	for _, item := range rep.Items {
		fmt.Fprintf(&b, "| %s | %s | `%s` | %s | %s | %s | %s |\n",
			item.Priority,
			item.DriftType,
			escapeMarkdownCell(item.Path),
			item.Owner,
			item.RootCause,
			item.Action,
			item.TrackingIssue,
		)
	}

	fmt.Fprintf(&b, "\n## Fill Rules\n\n")
	fmt.Fprintf(&b, "- Set owner + root cause + action + tracking issue before closing parity incident.\n")
	fmt.Fprintf(&b, "- Keep one row per drift path; split follow-up fixes into separate linked tasks.\n")
	return b.String()
}

func writeAlertRoutingReport(outDir string, items []driftReportItem) error {
	owners := make(map[string][]string)
	hasUnowned := false
	for _, item := range items {
		owner := item.Owner
		if strings.TrimSpace(owner) == "" || strings.EqualFold(owner, "unassigned") {
			owner = "unassigned"
			hasUnowned = true
		}
		owners[owner] = append(owners[owner], item.Path)
	}

	keys := make([]string, 0, len(owners))
	for owner := range owners {
		keys = append(keys, owner)
	}
	slices.Sort(keys)

	routes := make([]alertOwnerRoute, 0, len(keys))
	for _, owner := range keys {
		paths := owners[owner]
		slices.Sort(paths)
		routes = append(routes, alertOwnerRoute{
			Owner: owner,
			Count: len(paths),
			Paths: paths,
		})
	}

	rep := alertRoutingReport{
		SchemaVersion: "parity.alert-routing.v1",
		Summary: alertRoutingSummary{
			TotalAlerts: len(items),
			Owners:      len(routes),
			HasUnowned:  hasUnowned,
		},
		Routes: routes,
	}

	jb, err := json.MarshalIndent(rep, "", "  ")
	if err != nil {
		return err
	}
	jb = append(jb, '\n')
	if err := os.WriteFile(filepath.Join(outDir, "parity-alert-routing.json"), jb, 0o644); err != nil {
		return err
	}

	mb := []byte(renderAlertRoutingMarkdown(rep))
	if err := os.WriteFile(filepath.Join(outDir, "parity-alert-routing.md"), mb, 0o644); err != nil {
		return err
	}
	return nil
}

func renderAlertRoutingMarkdown(rep alertRoutingReport) string {
	var b strings.Builder
	fmt.Fprintf(&b, "# Parity Alert Routing\n\n")
	if rep.Summary.TotalAlerts == 0 {
		fmt.Fprintf(&b, "_No parity drift alerts._\n")
		return b.String()
	}

	fmt.Fprintf(&b, "- Drift alerts: %d\n", rep.Summary.TotalAlerts)
	fmt.Fprintf(&b, "- Owners notified: %d\n\n", rep.Summary.Owners)

	fmt.Fprintf(&b, "## Owner Routes\n\n")
	fmt.Fprintf(&b, "| Owner | Drift items | Paths |\n")
	fmt.Fprintf(&b, "|---|---:|---|\n")
	for _, route := range rep.Routes {
		paths := make([]string, 0, len(route.Paths))
		for _, p := range route.Paths {
			paths = append(paths, "`"+escapeMarkdownCell(p)+"`")
		}
		fmt.Fprintf(&b, "| %s | %d | %s |\n", route.Owner, route.Count, strings.Join(paths, ", "))
	}
	if rep.Summary.HasUnowned {
		fmt.Fprintf(&b, "\n- Warning: one or more paths are unassigned.\n")
	}
	return b.String()
}

func ownerForDriftPath(path string) string {
	p := strings.TrimSpace(strings.ToLower(path))
	switch {
	case strings.HasPrefix(p, "forged/"):
		return "forge-daemon"
	case strings.HasPrefix(p, "forge/loop-lifecycle/"):
		return "forge-loop"
	case strings.HasPrefix(p, "forge/operational/"),
		strings.HasPrefix(p, "forge/root/"),
		strings.HasPrefix(p, "forge/send-inject/"),
		strings.HasPrefix(p, "forge/help"):
		return "forge-cli"
	case strings.HasPrefix(p, "schema/"), strings.Contains(p, "schema-fingerprint"):
		return "forge-db"
	case strings.HasPrefix(p, "fmail/"):
		return "fmail-core"
	default:
		return "parity-infra"
	}
}

func escapeMarkdownCell(s string) string {
	return strings.ReplaceAll(s, "|", "\\|")
}

func copyTree(srcRoot, dstRoot string) error {
	files, err := listFiles(srcRoot)
	if err != nil {
		return err
	}
	for rel, src := range files {
		dst := filepath.Join(dstRoot, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
			return err
		}
		if err := copyFile(src, dst); err != nil {
			return err
		}
	}
	return nil
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()

	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()

	if _, err := io.Copy(out, in); err != nil {
		return err
	}
	return out.Close()
}

func slug(path string) string {
	replacer := strings.NewReplacer("/", "__", "\\", "__", " ", "_", ":", "_")
	return replacer.Replace(path)
}

// DriftSummary returns a compact text summary for CI logs.
func DriftSummary(report Report) string {
	return fmt.Sprintf("mismatched=%d missing=%d unexpected=%d", len(report.Mismatched), len(report.MissingExpected), len(report.Unexpected))
}
