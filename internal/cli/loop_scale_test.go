package cli

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/tOgg1/forge/internal/config"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

func TestLoopCLIScaleInitialWaitQueuesPauseForNewLoops(t *testing.T) {
	tmpDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmpDir, "PROMPT.md"), []byte("prompt"), 0o644); err != nil {
		t.Fatalf("write PROMPT.md: %v", err)
	}

	originalCfg := appConfig
	cfg := config.DefaultConfig()
	cfg.Global.DataDir = filepath.Join(tmpDir, "data")
	cfg.Global.ConfigDir = filepath.Join(tmpDir, "config")
	appConfig = cfg
	defer func() { appConfig = originalCfg }()

	if err := os.MkdirAll(cfg.Global.DataDir, 0o755); err != nil {
		t.Fatalf("mkdir data dir: %v", err)
	}

	originalWd, _ := os.Getwd()
	if err := os.Chdir(tmpDir); err != nil {
		t.Fatalf("chdir: %v", err)
	}
	defer func() { _ = os.Chdir(originalWd) }()

	originalStart := startLoopRunnerFunc
	startLoopRunnerFunc = func(string, string, loopSpawnOwner) (loopRunnerStartResult, error) {
		return loopRunnerStartResult{Owner: loopSpawnOwnerLocal}, nil
	}
	defer func() { startLoopRunnerFunc = originalStart }()

	loopScaleCount = 1
	loopScalePool = ""
	loopScaleProfile = ""
	loopScalePrompt = ""
	loopScalePromptMsg = ""
	loopScaleInterval = ""
	loopScaleInitialWait = "90s"
	loopScaleMaxRuntime = ""
	loopScaleMaxIterations = 0
	loopScaleTags = ""
	loopScaleNamePrefix = "scaled-wait"
	loopScaleKill = false
	loopScaleSpawnOwner = string(loopSpawnOwnerLocal)

	loopScaleQuantStopCmd = ""
	loopScaleQuantStopEvery = 1
	loopScaleQuantStopWhen = "before"
	loopScaleQuantStopDecision = "stop"
	loopScaleQuantStopExitCodes = ""
	loopScaleQuantStopExitInvert = false
	loopScaleQuantStopStdoutMode = "any"
	loopScaleQuantStopStderrMode = "any"
	loopScaleQuantStopStdoutRe = ""
	loopScaleQuantStopStderrRe = ""
	loopScaleQuantStopTimeout = ""
	loopScaleQualStopEvery = 0
	loopScaleQualStopPrompt = ""
	loopScaleQualStopPromptMsg = ""
	loopScaleQualStopOnInvalid = "continue"

	if err := loopScaleCmd.RunE(loopScaleCmd, nil); err != nil {
		t.Fatalf("loop scale: %v", err)
	}

	database, err := openDatabase()
	if err != nil {
		t.Fatalf("open database: %v", err)
	}
	defer database.Close()

	loopRepo := db.NewLoopRepository(database)
	loops, err := loopRepo.List(context.Background())
	if err != nil {
		t.Fatalf("list loops: %v", err)
	}
	if len(loops) != 1 {
		t.Fatalf("expected 1 loop, got %d", len(loops))
	}

	queueRepo := db.NewLoopQueueRepository(database)
	items, err := queueRepo.List(context.Background(), loops[0].ID)
	if err != nil {
		t.Fatalf("list queue: %v", err)
	}
	if len(items) != 1 {
		t.Fatalf("expected 1 queued item, got %d", len(items))
	}
	if items[0].Type != models.LoopQueueItemPause {
		t.Fatalf("expected queued pause, got %s", items[0].Type)
	}

	var payload models.LoopPausePayload
	if err := json.Unmarshal(items[0].Payload, &payload); err != nil {
		t.Fatalf("unmarshal pause payload: %v", err)
	}
	if payload.DurationSeconds != 90 {
		t.Fatalf("expected pause duration 90s, got %ds", payload.DurationSeconds)
	}
}
