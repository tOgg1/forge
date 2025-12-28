package beads

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestParseIssues(t *testing.T) {
	data := strings.Join([]string{
		`{"id":"forge-1","title":"First","description":"desc","status":"open","priority":2,"issue_type":"task","created_at":"2025-12-22T10:17:12.905606892+01:00","updated_at":"2025-12-22T10:18:12.905606892+01:00"}`,
		"",
		`{"id":"forge-2","title":"Second","description":"desc","status":"closed","priority":0,"issue_type":"bug","created_at":"2025-12-22T09:17:12.905606892+01:00","updated_at":"2025-12-22T09:18:12.905606892+01:00","closed_at":"2025-12-22T09:19:12.905606892+01:00","close_reason":"Done"}`,
	}, "\n")

	issues, err := ParseIssues(strings.NewReader(data))
	if err != nil {
		t.Fatalf("ParseIssues error: %v", err)
	}
	if len(issues) != 2 {
		t.Fatalf("expected 2 issues, got %d", len(issues))
	}
	if issues[0].ID != "forge-1" {
		t.Fatalf("expected first id forge-1, got %q", issues[0].ID)
	}
	if issues[0].Priority != 2 {
		t.Fatalf("expected priority 2, got %d", issues[0].Priority)
	}
	if issues[0].IssueType != "task" {
		t.Fatalf("expected issue type task, got %q", issues[0].IssueType)
	}

	wantUpdated, err := time.Parse(time.RFC3339Nano, "2025-12-22T10:18:12.905606892+01:00")
	if err != nil {
		t.Fatalf("parse time: %v", err)
	}
	if !issues[0].UpdatedAt.Equal(wantUpdated) {
		t.Fatalf("updated_at mismatch: %v", issues[0].UpdatedAt)
	}

	summaries := Summaries(issues)
	if len(summaries) != 2 {
		t.Fatalf("expected 2 summaries, got %d", len(summaries))
	}
	if summaries[1].Status != "closed" {
		t.Fatalf("expected summary status closed, got %q", summaries[1].Status)
	}
}

func TestParseIssuesInvalidJSON(t *testing.T) {
	_, err := ParseIssues(strings.NewReader("{not-json}\n"))
	if err == nil {
		t.Fatal("expected error")
	}
	if !strings.Contains(err.Error(), "line 1") {
		t.Fatalf("expected line number in error, got %v", err)
	}
}

func TestHasBeadsDir(t *testing.T) {
	repoPath := t.TempDir()

	ok, err := HasBeadsDir(repoPath)
	if err != nil {
		t.Fatalf("HasBeadsDir error: %v", err)
	}
	if ok {
		t.Fatal("expected false when .beads is missing")
	}

	beadsFile := filepath.Join(repoPath, BeadsDirName)
	if err := os.WriteFile(beadsFile, []byte("nope"), 0o644); err != nil {
		t.Fatalf("write file: %v", err)
	}

	ok, err = HasBeadsDir(repoPath)
	if err != nil {
		t.Fatalf("HasBeadsDir error: %v", err)
	}
	if ok {
		t.Fatal("expected false when .beads is a file")
	}

	if err := os.Remove(beadsFile); err != nil {
		t.Fatalf("remove file: %v", err)
	}
	if err := os.Mkdir(filepath.Join(repoPath, BeadsDirName), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}

	ok, err = HasBeadsDir(repoPath)
	if err != nil {
		t.Fatalf("HasBeadsDir error: %v", err)
	}
	if !ok {
		t.Fatal("expected true when .beads directory exists")
	}
}
