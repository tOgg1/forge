package paritydash

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// Input is a minimal machine-readable bundle produced by CI to describe
// which parity checks ran and their outcomes.
type Input struct {
	SchemaVersion string       `json:"schema_version,omitempty"`
	Run           RunInfo      `json:"run,omitempty"`
	Checks        []InputCheck `json:"checks"`
}

type RunInfo struct {
	Workflow    string `json:"workflow,omitempty"`
	RunID       string `json:"run_id,omitempty"`
	RunAttempt  string `json:"run_attempt,omitempty"`
	Ref         string `json:"ref,omitempty"`
	SHA         string `json:"sha,omitempty"`
	Repository  string `json:"repository,omitempty"`
	EventName   string `json:"event_name,omitempty"`
	ServerURL   string `json:"server_url,omitempty"`
	RunURL      string `json:"run_url,omitempty"`
	CompareURL  string `json:"compare_url,omitempty"`
	PullRequest string `json:"pull_request,omitempty"`
}

type InputCheck struct {
	ID      string `json:"id"`
	Name    string `json:"name,omitempty"`
	Outcome string `json:"outcome,omitempty"` // e.g. success|failure|skipped|cancelled
	Details string `json:"details,omitempty"`
	URL     string `json:"url,omitempty"`
}

type Dashboard struct {
	SchemaVersion string  `json:"schema_version"`
	GeneratedAt   string  `json:"generated_at"`
	Run           RunInfo `json:"run,omitempty"`
	Summary       Summary `json:"summary"`
	Checks        []Check `json:"checks"`
}

type Summary struct {
	Total   int    `json:"total"`
	Passed  int    `json:"passed"`
	Failed  int    `json:"failed"`
	Skipped int    `json:"skipped"`
	Unknown int    `json:"unknown"`
	Status  string `json:"status"` // pass|fail
}

type Check struct {
	ID      string `json:"id"`
	Name    string `json:"name"`
	Status  string `json:"status"`  // pass|fail|skipped|unknown
	Outcome string `json:"outcome"` // original outcome string
	Details string `json:"details,omitempty"`
	URL     string `json:"url,omitempty"`
}

func Build(input Input, now time.Time) (Dashboard, error) {
	if len(input.Checks) == 0 {
		return Dashboard{}, errors.New("no checks provided")
	}

	d := Dashboard{
		SchemaVersion: "paritydash.v1",
		GeneratedAt:   now.UTC().Format(time.RFC3339),
		Run:           input.Run,
	}

	for _, in := range input.Checks {
		if strings.TrimSpace(in.ID) == "" {
			return Dashboard{}, errors.New("check missing id")
		}

		name := strings.TrimSpace(in.Name)
		if name == "" {
			name = in.ID
		}

		status := statusFromOutcome(in.Outcome)
		d.Checks = append(d.Checks, Check{
			ID:      in.ID,
			Name:    name,
			Status:  status,
			Outcome: strings.TrimSpace(in.Outcome),
			Details: strings.TrimSpace(in.Details),
			URL:     strings.TrimSpace(in.URL),
		})

		d.Summary.Total++
		switch status {
		case "pass":
			d.Summary.Passed++
		case "fail":
			d.Summary.Failed++
		case "skipped":
			d.Summary.Skipped++
		default:
			d.Summary.Unknown++
		}
	}

	// Fail closed: unknown outcomes mean we cannot trust parity status.
	if d.Summary.Failed > 0 || d.Summary.Unknown > 0 {
		d.Summary.Status = "fail"
	} else {
		d.Summary.Status = "pass"
	}

	return d, nil
}

func statusFromOutcome(outcome string) string {
	switch strings.ToLower(strings.TrimSpace(outcome)) {
	case "success", "passed", "pass", "ok":
		return "pass"
	case "failure", "failed", "fail", "cancelled", "canceled", "timed_out", "timeout":
		return "fail"
	case "skipped", "skip":
		return "skipped"
	case "":
		return "unknown"
	default:
		return "unknown"
	}
}

func WriteFiles(outDir string, d Dashboard, writeMarkdown bool) error {
	if strings.TrimSpace(outDir) == "" {
		return errors.New("out dir is required")
	}

	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return fmt.Errorf("mkdir: %w", err)
	}

	jsonPath := filepath.Join(outDir, "parity-dashboard.json")
	b, err := json.MarshalIndent(d, "", "  ")
	if err != nil {
		return fmt.Errorf("marshal: %w", err)
	}
	b = append(b, '\n')
	if err := os.WriteFile(jsonPath, b, 0o644); err != nil {
		return fmt.Errorf("write json: %w", err)
	}

	if writeMarkdown {
		mdPath := filepath.Join(outDir, "parity-dashboard.md")
		md := []byte(MarkdownSummary(d) + "\n")
		if err := os.WriteFile(mdPath, md, 0o644); err != nil {
			return fmt.Errorf("write md: %w", err)
		}
	}

	return nil
}
