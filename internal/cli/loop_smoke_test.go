package cli

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/tOgg1/forge/internal/config"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/loop"
	"github.com/tOgg1/forge/internal/models"
)

func TestLoopCLIUpMsgLogs(t *testing.T) {
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

	loopUpCount = 1
	loopUpName = "smoke-loop"
	loopUpNamePrefix = ""
	loopUpPool = ""
	loopUpProfile = ""
	loopUpPrompt = ""
	loopUpPromptMsg = ""
	loopUpInterval = ""
	loopUpInitialWait = ""
	loopUpMaxRuntime = "1m"
	loopUpMaxIterations = 1
	loopUpTags = ""

	if err := loopUpCmd.RunE(loopUpCmd, nil); err != nil {
		t.Fatalf("loop up: %v", err)
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
	loopEntry := loops[0]

	logPath := loopEntry.LogPath
	if logPath == "" {
		logPath = loop.LogPath(cfg.Global.DataDir, loopEntry.Name, loopEntry.ID)
	}
	if err := os.MkdirAll(filepath.Dir(logPath), 0o755); err != nil {
		t.Fatalf("mkdir log dir: %v", err)
	}
	if err := os.WriteFile(logPath, []byte("[2025-01-01T00:00:00Z] hello\n"), 0o644); err != nil {
		t.Fatalf("write log: %v", err)
	}

	logsFollow = false
	logsLines = 1
	logsSince = ""
	logsAll = false
	if err := logsCmd.RunE(logsCmd, []string{loopEntry.Name}); err != nil {
		t.Fatalf("loop logs: %v", err)
	}

	msgNow = false
	msgNextPrompt = ""
	msgTemplate = ""
	msgSequence = ""
	msgVars = nil
	msgPool = ""
	msgProfile = ""
	msgState = ""
	msgTag = ""
	msgAll = false
	if err := loopMsgCmd.RunE(loopMsgCmd, []string{loopEntry.Name, "hello"}); err != nil {
		t.Fatalf("loop msg: %v", err)
	}

	queueRepo := db.NewLoopQueueRepository(database)
	items, err := queueRepo.List(context.Background(), loopEntry.ID)
	if err != nil {
		t.Fatalf("list queue: %v", err)
	}
	if len(items) == 0 {
		t.Fatalf("expected queued item")
	}
}

func TestLoopCLIUpAcceptsZeroLimitsAsUnlimited(t *testing.T) {
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

	loopUpCount = 1
	loopUpName = "reject-zero"
	loopUpNamePrefix = ""
	loopUpPool = ""
	loopUpProfile = ""
	loopUpPrompt = ""
	loopUpPromptMsg = ""
	loopUpInterval = ""
	loopUpInitialWait = ""
	loopUpMaxRuntime = ""
	loopUpMaxIterations = 0
	loopUpTags = ""

	if err := loopUpCmd.RunE(loopUpCmd, nil); err != nil {
		t.Fatalf("loop up: %v", err)
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
		t.Fatalf("expected 1 loop created, got %d", len(loops))
	}
	if loops[0].MaxIterations != 0 {
		t.Fatalf("expected MaxIterations=0 (unlimited), got %d", loops[0].MaxIterations)
	}
	if loops[0].MaxRuntimeSeconds != 0 {
		t.Fatalf("expected MaxRuntimeSeconds=0 (unlimited), got %d", loops[0].MaxRuntimeSeconds)
	}
}

func TestLoopCLIUpInitialWaitQueuesPause(t *testing.T) {
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

	loopUpCount = 1
	loopUpName = "wait-loop"
	loopUpNamePrefix = ""
	loopUpPool = ""
	loopUpProfile = ""
	loopUpPrompt = ""
	loopUpPromptMsg = ""
	loopUpInterval = ""
	loopUpInitialWait = "45s"
	loopUpMaxRuntime = ""
	loopUpMaxIterations = 0
	loopUpTags = ""
	loopUpSpawnOwner = string(loopSpawnOwnerLocal)

	if err := loopUpCmd.RunE(loopUpCmd, nil); err != nil {
		t.Fatalf("loop up: %v", err)
	}

	database, err := openDatabase()
	if err != nil {
		t.Fatalf("open database: %v", err)
	}
	defer database.Close()

	loopRepo := db.NewLoopRepository(database)
	loopEntry, err := loopRepo.GetByName(context.Background(), "wait-loop")
	if err != nil {
		t.Fatalf("get loop: %v", err)
	}

	queueRepo := db.NewLoopQueueRepository(database)
	items, err := queueRepo.List(context.Background(), loopEntry.ID)
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
	if payload.DurationSeconds != 45 {
		t.Fatalf("expected pause duration 45s, got %ds", payload.DurationSeconds)
	}
}
