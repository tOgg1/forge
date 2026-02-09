package paritydash

import (
	"fmt"
	"sort"
	"strings"
)

func MarkdownSummary(d Dashboard) string {
	var b strings.Builder

	fmt.Fprintf(&b, "## Parity Dashboard\n\n")
	fmt.Fprintf(&b, "- Status: %s\n", strings.ToUpper(d.Summary.Status))
	fmt.Fprintf(&b, "- Checks: %d total (%d pass, %d fail, %d skipped, %d unknown)\n\n",
		d.Summary.Total, d.Summary.Passed, d.Summary.Failed, d.Summary.Skipped, d.Summary.Unknown)

	if d.Run.Workflow != "" {
		fmt.Fprintf(&b, "### Run\n\n")
		if d.Run.Repository != "" {
			fmt.Fprintf(&b, "- Repo: %s\n", d.Run.Repository)
		}
		fmt.Fprintf(&b, "- Workflow: %s\n", d.Run.Workflow)
		if d.Run.Ref != "" {
			fmt.Fprintf(&b, "- Ref: %s\n", d.Run.Ref)
		}
		if d.Run.SHA != "" {
			fmt.Fprintf(&b, "- SHA: %s\n", d.Run.SHA)
		}
		if d.Run.RunURL != "" {
			fmt.Fprintf(&b, "- Run: %s\n", d.Run.RunURL)
		}
		fmt.Fprintf(&b, "\n")
	}

	checks := append([]Check(nil), d.Checks...)
	sort.SliceStable(checks, func(i, j int) bool {
		if checks[i].Status == checks[j].Status {
			return checks[i].ID < checks[j].ID
		}
		// FAIL first, then UNKNOWN, then SKIPPED, then PASS.
		return statusRank(checks[i].Status) < statusRank(checks[j].Status)
	})

	fmt.Fprintf(&b, "### Checks\n\n")
	fmt.Fprintf(&b, "| Status | ID | Name |\n")
	fmt.Fprintf(&b, "|---|---|---|\n")
	for _, c := range checks {
		status := strings.ToUpper(c.Status)
		name := c.Name
		if c.URL != "" {
			// Don't attempt markdown links; some consumers treat it as plain text.
			name = fmt.Sprintf("%s (%s)", c.Name, c.URL)
		}
		fmt.Fprintf(&b, "| %s | %s | %s |\n", status, c.ID, escapePipes(name))
	}

	return strings.TrimSpace(b.String())
}

func statusRank(status string) int {
	switch status {
	case "fail":
		return 0
	case "unknown":
		return 1
	case "skipped":
		return 2
	case "pass":
		return 3
	default:
		return 4
	}
}

func escapePipes(s string) string {
	return strings.ReplaceAll(s, "|", "\\|")
}

