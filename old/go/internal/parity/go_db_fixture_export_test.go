package parity

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

type goCompatProfileSnapshot struct {
	ID             string            `json:"id"`
	Name           string            `json:"name"`
	Harness        string            `json:"harness"`
	PromptMode     string            `json:"prompt_mode"`
	CommandTemplate string           `json:"command_template"`
	Model          string            `json:"model"`
	MaxConcurrency int              `json:"max_concurrency"`
	ExtraArgs      []string          `json:"extra_args"`
	Env            map[string]string `json:"env"`
}

type goCompatLoopSnapshot struct {
	ID              string `json:"id"`
	ShortID         string `json:"short_id"`
	Name            string `json:"name"`
	RepoPath        string `json:"repo_path"`
	IntervalSeconds int    `json:"interval_seconds"`
	ProfileID       string `json:"profile_id"`
	State           string `json:"state"`
}

type goCompatLoopRunSnapshot struct {
	ID           string `json:"id"`
	LoopID       string `json:"loop_id"`
	ProfileID    string `json:"profile_id"`
	Status       string `json:"status"`
	PromptSource string `json:"prompt_source"`
	ExitCode     int    `json:"exit_code"`
	OutputTail   string `json:"output_tail"`
}

type goCompatQueueSnapshot struct {
	ID      string `json:"id"`
	LoopID  string `json:"loop_id"`
	Type    string `json:"type"`
	Status  string `json:"status"`
	Payload string `json:"payload_json"`
}

type goCompatSnapshot struct {
	Profile goCompatProfileSnapshot `json:"profile"`
	Loop    goCompatLoopSnapshot    `json:"loop"`
	Run     goCompatLoopRunSnapshot `json:"loop_run"`
	Queue   goCompatQueueSnapshot   `json:"queue_item"`
}

func TestExportGoDBCompatFixture(t *testing.T) {
	fixtureDir := strings.TrimSpace(os.Getenv("FORGE_GO_DB_FIXTURE_DIR"))
	if fixtureDir == "" {
		t.Skip("FORGE_GO_DB_FIXTURE_DIR not set")
	}
	if err := os.MkdirAll(fixtureDir, 0o755); err != nil {
		t.Fatalf("mkdir fixture dir: %v", err)
	}

	dbPath := filepath.Join(fixtureDir, "forge-go-compat.db")
	snapshotPath := filepath.Join(fixtureDir, "forge-go-compat.snapshot.json")
	_ = os.Remove(dbPath)
	_ = os.Remove(snapshotPath)

	cfg := db.DefaultConfig()
	cfg.Path = dbPath
	database, err := db.Open(cfg)
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	t.Cleanup(func() { _ = database.Close() })

	ctx := context.Background()
	if _, err := database.MigrateUp(ctx); err != nil {
		t.Fatalf("migrate up: %v", err)
	}

	profileRepo := db.NewProfileRepository(database)
	loopRepo := db.NewLoopRepository(database)
	runRepo := db.NewLoopRunRepository(database)
	queueRepo := db.NewLoopQueueRepository(database)

	profile := &models.Profile{
		ID:              "go-compat-profile-1",
		Name:            "go-compat-profile",
		Harness:         models.HarnessCodex,
		PromptMode:      models.PromptModeEnv,
		CommandTemplate: "codex exec --prompt {{.Prompt}}",
		Model:           "gpt-5",
		ExtraArgs:       []string{"--fast"},
		Env:             map[string]string{"TEAM": "parity"},
		MaxConcurrency:  2,
	}
	if err := profileRepo.Create(ctx, profile); err != nil {
		t.Fatalf("create profile: %v", err)
	}

	loop := &models.Loop{
		ID:              "go-compat-loop-1",
		ShortID:         "abc123",
		Name:            "go-compat-loop",
		RepoPath:        "/tmp/go-compat-repo",
		IntervalSeconds: 30,
		ProfileID:       profile.ID,
		State:           models.LoopStateSleeping,
	}
	if err := loopRepo.Create(ctx, loop); err != nil {
		t.Fatalf("create loop: %v", err)
	}

	run := &models.LoopRun{
		ID:           "go-compat-run-1",
		LoopID:       loop.ID,
		ProfileID:    profile.ID,
		Status:       models.LoopRunStatusRunning,
		PromptSource: "cli",
		StartedAt:    time.Date(2026, 2, 10, 12, 0, 0, 0, time.UTC),
	}
	if err := runRepo.Create(ctx, run); err != nil {
		t.Fatalf("create loop run: %v", err)
	}
	zero := 0
	run.Status = models.LoopRunStatusSuccess
	run.ExitCode = &zero
	run.OutputTail = "go-run-tail"
	if err := runRepo.Finish(ctx, run); err != nil {
		t.Fatalf("finish loop run: %v", err)
	}

	queueItem := &models.LoopQueueItem{
		ID:      "go-compat-queue-1",
		Type:    models.LoopQueueItemMessageAppend,
		Payload: json.RawMessage(`{"text":"from-go"}`),
	}
	if err := queueRepo.Enqueue(ctx, loop.ID, queueItem); err != nil {
		t.Fatalf("enqueue queue item: %v", err)
	}

	gotProfile, err := profileRepo.Get(ctx, profile.ID)
	if err != nil {
		t.Fatalf("read profile: %v", err)
	}
	gotLoop, err := loopRepo.Get(ctx, loop.ID)
	if err != nil {
		t.Fatalf("read loop: %v", err)
	}
	gotRuns, err := runRepo.ListByLoop(ctx, loop.ID)
	if err != nil {
		t.Fatalf("list runs: %v", err)
	}
	if len(gotRuns) == 0 {
		t.Fatalf("expected at least one loop run")
	}
	gotQueueItems, err := queueRepo.List(ctx, loop.ID)
	if err != nil {
		t.Fatalf("list queue items: %v", err)
	}
	if len(gotQueueItems) == 0 {
		t.Fatalf("expected at least one queue item")
	}

	exitCode := 0
	if gotRuns[0].ExitCode != nil {
		exitCode = *gotRuns[0].ExitCode
	}

	snapshot := goCompatSnapshot{
		Profile: goCompatProfileSnapshot{
			ID:              gotProfile.ID,
			Name:            gotProfile.Name,
			Harness:         string(gotProfile.Harness),
			PromptMode:      string(gotProfile.PromptMode),
			CommandTemplate: gotProfile.CommandTemplate,
			Model:           gotProfile.Model,
			MaxConcurrency:  gotProfile.MaxConcurrency,
			ExtraArgs:       gotProfile.ExtraArgs,
			Env:             gotProfile.Env,
		},
		Loop: goCompatLoopSnapshot{
			ID:              gotLoop.ID,
			ShortID:         gotLoop.ShortID,
			Name:            gotLoop.Name,
			RepoPath:        gotLoop.RepoPath,
			IntervalSeconds: gotLoop.IntervalSeconds,
			ProfileID:       gotLoop.ProfileID,
			State:           string(gotLoop.State),
		},
		Run: goCompatLoopRunSnapshot{
			ID:           gotRuns[0].ID,
			LoopID:       gotRuns[0].LoopID,
			ProfileID:    gotRuns[0].ProfileID,
			Status:       string(gotRuns[0].Status),
			PromptSource: gotRuns[0].PromptSource,
			ExitCode:     exitCode,
			OutputTail:   gotRuns[0].OutputTail,
		},
		Queue: goCompatQueueSnapshot{
			ID:      gotQueueItems[0].ID,
			LoopID:  gotQueueItems[0].LoopID,
			Type:    string(gotQueueItems[0].Type),
			Status:  string(gotQueueItems[0].Status),
			Payload: string(gotQueueItems[0].Payload),
		},
	}

	data, err := json.MarshalIndent(snapshot, "", "  ")
	if err != nil {
		t.Fatalf("marshal snapshot: %v", err)
	}
	data = append(data, '\n')
	if err := os.WriteFile(snapshotPath, data, 0o644); err != nil {
		t.Fatalf("write snapshot: %v", err)
	}
}
