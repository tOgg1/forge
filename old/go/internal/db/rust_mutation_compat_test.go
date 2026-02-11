package db

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/tOgg1/forge/internal/models"
)

func TestGoReadsRustMutatedDB(t *testing.T) {
	if testing.Short() {
		t.Skip("compatibility probe is integration-style; skip in -short")
	}
	if _, err := exec.LookPath("cargo"); err != nil {
		t.Skipf("cargo not available: %v", err)
	}

	dbPath := filepath.Join(t.TempDir(), "rust-mutated.sqlite")

	cmd := exec.Command("cargo", "test", "-p", "forge-db", "--test", "go_db_compat_seed_test", "--", "--exact", "seed_compat_db")
	cmd.Dir = filepath.Join(repoRootFromDBTestFile(t), "rust")
	cmd.Env = append(os.Environ(), "FORGE_RUST_DB_COMPAT_OUT="+dbPath)
	seedOut, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("seed rust-mutated db: %v\n%s", err, string(seedOut))
	}

	cfg := DefaultConfig()
	cfg.Path = dbPath
	database, err := Open(cfg)
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	defer func() { _ = database.Close() }()

	repo := NewLoopRepository(database)
	loops, err := repo.List(context.Background())
	if err != nil {
		t.Fatalf("go list loops from rust-mutated db: %v", err)
	}
	if len(loops) != 1 {
		t.Fatalf("expected exactly 1 loop, got %d", len(loops))
	}

	loop := loops[0]
	if loop.Name != "rust-compat-loop" {
		t.Fatalf("name mismatch: got %q", loop.Name)
	}
	if loop.RepoPath != "/tmp/rust-compat-repo" {
		t.Fatalf("repo_path mismatch: got %q", loop.RepoPath)
	}
	if loop.State != models.LoopStateWaiting {
		t.Fatalf("state mismatch: got %q", loop.State)
	}
	if loop.LastError != "waiting-for-go-read" {
		t.Fatalf("last_error mismatch: got %q", loop.LastError)
	}
	if loop.BasePromptPath != "/tmp/prompt.md" {
		t.Fatalf("base_prompt_path mismatch: got %q", loop.BasePromptPath)
	}
	if loop.IntervalSeconds != 42 || loop.MaxIterations != 7 || loop.MaxRuntimeSeconds != 120 {
		t.Fatalf(
			"limits mismatch: interval=%d max_iterations=%d max_runtime=%d",
			loop.IntervalSeconds,
			loop.MaxIterations,
			loop.MaxRuntimeSeconds,
		)
	}
	if len(loop.Tags) != 2 || loop.Tags[0] != "rust" || loop.Tags[1] != "compat" {
		t.Fatalf("tags mismatch: %#v", loop.Tags)
	}

	source, ok := loop.Metadata["source"].(string)
	if !ok || source != "rust" {
		t.Fatalf("metadata.source mismatch: %#v", loop.Metadata["source"])
	}
	version, ok := loop.Metadata["version"].(float64)
	if !ok || version != 1 {
		t.Fatalf("metadata.version mismatch: %#v", loop.Metadata["version"])
	}
}

func repoRootFromDBTestFile(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("resolve test file path")
	}
	return filepath.Clean(filepath.Join(filepath.Dir(file), "..", ".."))
}
