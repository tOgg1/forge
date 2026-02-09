package cli

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"testing"

	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/loop"
	"github.com/tOgg1/forge/internal/models"
)

func TestLoopLifecycleOracleScenarioMatchesFixture(t *testing.T) {
	repo := t.TempDir()
	if err := os.WriteFile(filepath.Join(repo, "PROMPT.md"), []byte("prompt"), 0o644); err != nil {
		t.Fatalf("write PROMPT.md: %v", err)
	}

	cleanupConfig := withTempConfig(t, repo)
	defer cleanupConfig()

	withWorkingDir(t, repo, func() {
		restoreGlobals := snapshotLoopLifecycleGlobals()
		defer restoreGlobals()

		originalStart := startLoopRunnerFunc
		startLoopRunnerFunc = func(string, string, loopSpawnOwner) (loopRunnerStartResult, error) {
			return loopRunnerStartResult{Owner: loopSpawnOwnerLocal}, nil
		}
		defer func() { startLoopRunnerFunc = originalStart }()

		yesFlag = true
		nonInteractive = true
		jsonOutput = true
		jsonlOutput = false
		quiet = false
		noColor = true

		summary := map[string]any{}

		resetLoopUpFlags()
		loopUpCount = 1
		loopUpName = "oracle-main"
		loopUpSpawnOwner = string(loopSpawnOwnerLocal)
		upOut, err := captureStdout(func() error { return loopUpCmd.RunE(loopUpCmd, nil) })
		if err != nil {
			t.Fatalf("loop up: %v", err)
		}
		upRows := decodeJSONArray(t, upOut)
		summary["up_created"] = len(upRows)

		database, err := openDatabase()
		if err != nil {
			t.Fatalf("open database: %v", err)
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		queueRepo := db.NewLoopQueueRepository(database)
		mainLoop, err := loopRepo.GetByName(context.Background(), "oracle-main")
		if err != nil {
			t.Fatalf("get oracle-main: %v", err)
		}
		summary["up_state"] = string(mainLoop.State)

		loopPsRepo = ""
		loopPsPool = ""
		loopPsProfile = ""
		loopPsState = ""
		loopPsTag = ""
		psOut, err := captureStdout(func() error { return loopPsCmd.RunE(loopPsCmd, nil) })
		if err != nil {
			t.Fatalf("loop ps: %v", err)
		}
		psRows := decodeJSONArray(t, psOut)
		summary["ps_count"] = len(psRows)
		psNames := make([]string, 0, len(psRows))
		for _, row := range psRows {
			psNames = append(psNames, toStringValue(row["name"]))
		}
		sort.Strings(psNames)
		summary["ps_names"] = psNames

		logPath := mainLoop.LogPath
		if logPath == "" {
			logPath = loop.LogPath(GetConfig().Global.DataDir, mainLoop.Name, mainLoop.ID)
		}
		if err := os.MkdirAll(filepath.Dir(logPath), 0o755); err != nil {
			t.Fatalf("mkdir log dir: %v", err)
		}
		if err := os.WriteFile(logPath, []byte("[2026-01-01T00:00:00Z] first\n[2026-01-01T00:00:01Z] second\n"), 0o644); err != nil {
			t.Fatalf("write log: %v", err)
		}

		jsonOutput = false
		logsFollow = false
		logsLines = 1
		logsSince = ""
		logsAll = false
		logOut, err := captureStdout(func() error { return logsCmd.RunE(logsCmd, []string{"oracle-main"}) })
		if err != nil {
			t.Fatalf("loop logs: %v", err)
		}
		summary["logs_has_header"] = strings.Contains(logOut, "==> oracle-main <==")
		summary["logs_has_tail"] = strings.Contains(logOut, "second")
		jsonOutput = true

		resetLoopMsgFlags()
		msgOut1, err := captureStdout(func() error { return loopMsgCmd.RunE(loopMsgCmd, []string{"oracle-main", "hello-1"}) })
		if err != nil {
			t.Fatalf("loop msg first: %v", err)
		}
		msgResult1 := decodeJSONMap(t, msgOut1)
		summary["msg_first_loops"] = intValue(msgResult1["loops"])

		resetLoopMsgFlags()
		if _, err := captureStdout(func() error { return loopMsgCmd.RunE(loopMsgCmd, []string{"oracle-main", "hello-2"}) }); err != nil {
			t.Fatalf("loop msg second: %v", err)
		}

		items, err := queueRepo.List(context.Background(), mainLoop.ID)
		if err != nil {
			t.Fatalf("queue list after msg: %v", err)
		}
		summary["msg_pending_after_two"] = pendingCount(items)

		queueAll = false
		queueTo = "front"
		queueOut, err := captureStdout(func() error { return loopQueueListCmd.RunE(loopQueueListCmd, []string{"oracle-main"}) })
		if err != nil {
			t.Fatalf("queue ls: %v", err)
		}
		queueRows := decodeJSONArray(t, queueOut)
		summary["queue_ls_count"] = len(queueRows)
		firstID := toStringValue(queueRows[0]["id"])

		queueTo = "back"
		moveOut, err := captureStdout(func() error { return loopQueueMoveCmd.RunE(loopQueueMoveCmd, []string{"oracle-main", firstID}) })
		if err != nil {
			t.Fatalf("queue move: %v", err)
		}
		moveResult := decodeJSONMap(t, moveOut)
		summary["queue_move_to_back"] = toStringValue(moveResult["to"])

		queueOut, err = captureStdout(func() error { return loopQueueListCmd.RunE(loopQueueListCmd, []string{"oracle-main"}) })
		if err != nil {
			t.Fatalf("queue ls after move: %v", err)
		}
		queueRows = decodeJSONArray(t, queueOut)
		removeID := toStringValue(queueRows[0]["id"])
		rmQueueOut, err := captureStdout(func() error { return loopQueueRemoveCmd.RunE(loopQueueRemoveCmd, []string{"oracle-main", removeID}) })
		if err != nil {
			t.Fatalf("queue rm: %v", err)
		}
		rmQueueResult := decodeJSONMap(t, rmQueueOut)
		summary["queue_rm_removed"] = toStringValue(rmQueueResult["removed"]) == removeID

		clearOut, err := captureStdout(func() error { return loopQueueClearCmd.RunE(loopQueueClearCmd, []string{"oracle-main"}) })
		if err != nil {
			t.Fatalf("queue clear: %v", err)
		}
		clearResult := decodeJSONMap(t, clearOut)
		summary["queue_clear_count"] = intValue(clearResult["cleared"])

		resetLoopStopFlags()
		stopOut, err := captureStdout(func() error { return loopStopCmd.RunE(loopStopCmd, []string{"oracle-main"}) })
		if err != nil {
			t.Fatalf("loop stop: %v", err)
		}
		stopResult := decodeJSONMap(t, stopOut)
		summary["stop_action"] = toStringValue(stopResult["action"])

		resetLoopKillFlags()
		killOut, err := captureStdout(func() error { return loopKillCmd.RunE(loopKillCmd, []string{"oracle-main"}) })
		if err != nil {
			t.Fatalf("loop kill: %v", err)
		}
		killResult := decodeJSONMap(t, killOut)
		summary["kill_action"] = toStringValue(killResult["action"])

		mainLoop, err = loopRepo.GetByName(context.Background(), "oracle-main")
		if err != nil {
			t.Fatalf("reload oracle-main after kill: %v", err)
		}
		summary["kill_state"] = string(mainLoop.State)

		loopResumeSpawnOwner = string(loopSpawnOwnerLocal)
		resumeOut, err := captureStdout(func() error { return loopResumeCmd.RunE(loopResumeCmd, []string{"oracle-main"}) })
		if err != nil {
			t.Fatalf("loop resume: %v", err)
		}
		resumeResult := decodeJSONMap(t, resumeOut)
		summary["resume_resumed"] = boolValue(resumeResult["resumed"])

		err = loopRunOnceCmd.RunE(loopRunOnceCmd, []string{"does-not-exist"})
		summary["run_missing_error"] = err != nil

		resetLoopScaleFlags()
		loopScaleCount = 2
		loopScaleNamePrefix = "oracle-scale"
		loopScaleInitialWait = "30s"
		loopScaleSpawnOwner = string(loopSpawnOwnerLocal)
		scaleOut, err := captureStdout(func() error { return loopScaleCmd.RunE(loopScaleCmd, nil) })
		if err != nil {
			t.Fatalf("loop scale: %v", err)
		}
		scaleResult := decodeJSONMap(t, scaleOut)
		summary["scale_target"] = intValue(scaleResult["target"])
		summary["scale_current_before"] = intValue(scaleResult["current"])

		scaledLoop, err := loopRepo.GetByName(context.Background(), "oracle-scale-1")
		if err != nil {
			t.Fatalf("get scaled loop: %v", err)
		}
		scaledItems, err := queueRepo.List(context.Background(), scaledLoop.ID)
		if err != nil {
			t.Fatalf("list scaled queue: %v", err)
		}
		pauseCount := 0
		for _, item := range scaledItems {
			if item.Type == models.LoopQueueItemPause {
				pauseCount++
			}
		}
		summary["scale_pause_items"] = pauseCount

		resetLoopRmFlags()
		loopRmForce = true
		rmOut, err := captureStdout(func() error { return loopRmCmd.RunE(loopRmCmd, []string{"oracle-scale-1"}) })
		if err != nil {
			t.Fatalf("loop rm: %v", err)
		}
		rmResult := decodeJSONMap(t, rmOut)
		summary["rm_removed"] = intValue(rmResult["removed"])

		loopCleanRepo = ""
		loopCleanPool = ""
		loopCleanProfile = ""
		loopCleanTag = ""
		cleanOut, err := captureStdout(func() error { return loopCleanCmd.RunE(loopCleanCmd, nil) })
		if err != nil {
			t.Fatalf("loop clean: %v", err)
		}
		cleanResult := decodeJSONMap(t, cleanOut)
		summary["clean_removed"] = intValue(cleanResult["removed"])

		loops, err := loopRepo.List(context.Background())
		if err != nil {
			t.Fatalf("list final loops: %v", err)
		}
		summary["final_loop_count"] = len(loops)

		wantFixture := decodeJSONMap(t, readLoopLifecycleFixture(t))
		wantJSON := prettyJSON(t, wantFixture)
		gotJSON := prettyJSON(t, summary)
		if gotJSON != wantJSON {
			t.Fatalf("loop lifecycle fixture drift\nwant:\n%s\ngot:\n%s", wantJSON, gotJSON)
		}
	})
}

func snapshotLoopLifecycleGlobals() func() {
	prev := struct {
		yesFlag             bool
		nonInteractive      bool
		jsonOutput          bool
		jsonlOutput         bool
		quiet               bool
		noColor             bool
		loopResumeSpawnOwner string
	}{
		yesFlag:             yesFlag,
		nonInteractive:      nonInteractive,
		jsonOutput:          jsonOutput,
		jsonlOutput:         jsonlOutput,
		quiet:               quiet,
		noColor:             noColor,
		loopResumeSpawnOwner: loopResumeSpawnOwner,
	}
	return func() {
		yesFlag = prev.yesFlag
		nonInteractive = prev.nonInteractive
		jsonOutput = prev.jsonOutput
		jsonlOutput = prev.jsonlOutput
		quiet = prev.quiet
		noColor = prev.noColor
		loopResumeSpawnOwner = prev.loopResumeSpawnOwner
	}
}

func resetLoopUpFlags() {
	loopUpCount = 1
	loopUpName = ""
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
	loopUpSpawnOwner = string(loopSpawnOwnerAuto)
	loopUpQuantStopCmd = ""
	loopUpQuantStopEvery = 1
	loopUpQuantStopWhen = "before"
	loopUpQuantStopDecision = "stop"
	loopUpQuantStopExitCodes = ""
	loopUpQuantStopExitInvert = false
	loopUpQuantStopStdoutMode = "any"
	loopUpQuantStopStderrMode = "any"
	loopUpQuantStopStdoutRe = ""
	loopUpQuantStopStderrRe = ""
	loopUpQuantStopTimeout = ""
	loopUpQualStopEvery = 0
	loopUpQualStopPrompt = ""
	loopUpQualStopPromptMsg = ""
	loopUpQualStopOnInvalid = "continue"
}

func resetLoopMsgFlags() {
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
}

func resetLoopStopFlags() {
	loopStopAll = false
	loopStopRepo = ""
	loopStopPool = ""
	loopStopProfile = ""
	loopStopState = ""
	loopStopTag = ""
}

func resetLoopKillFlags() {
	loopKillAll = false
	loopKillRepo = ""
	loopKillPool = ""
	loopKillProfile = ""
	loopKillState = ""
	loopKillTag = ""
}

func resetLoopScaleFlags() {
	loopScaleCount = 1
	loopScalePool = ""
	loopScaleProfile = ""
	loopScalePrompt = ""
	loopScalePromptMsg = ""
	loopScaleInterval = ""
	loopScaleInitialWait = ""
	loopScaleMaxRuntime = ""
	loopScaleMaxIterations = 0
	loopScaleTags = ""
	loopScaleNamePrefix = ""
	loopScaleKill = false
	loopScaleSpawnOwner = string(loopSpawnOwnerAuto)
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
}

func resetLoopRmFlags() {
	loopRmAll = false
	loopRmRepo = ""
	loopRmPool = ""
	loopRmProfile = ""
	loopRmState = ""
	loopRmTag = ""
	loopRmForce = false
}

func decodeJSONArray(t *testing.T, out string) []map[string]any {
	t.Helper()
	var rows []map[string]any
	if err := json.Unmarshal([]byte(strings.TrimSpace(out)), &rows); err != nil {
		t.Fatalf("decode json array: %v\noutput:\n%s", err, out)
	}
	return rows
}

func decodeJSONMap(t *testing.T, out string) map[string]any {
	t.Helper()
	var row map[string]any
	if err := json.Unmarshal([]byte(strings.TrimSpace(out)), &row); err != nil {
		t.Fatalf("decode json map: %v\noutput:\n%s", err, out)
	}
	return row
}

func pendingCount(items []*models.LoopQueueItem) int {
	count := 0
	for _, item := range items {
		if item.Status == models.LoopQueueStatusPending {
			count++
		}
	}
	return count
}

func intValue(v any) int {
	switch value := v.(type) {
	case float64:
		return int(value)
	case int:
		return value
	default:
		return 0
	}
}

func boolValue(v any) bool {
	switch value := v.(type) {
	case bool:
		return value
	default:
		return false
	}
}

func toStringValue(v any) string {
	switch value := v.(type) {
	case string:
		return value
	default:
		return ""
	}
}

func readLoopLifecycleFixture(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve caller path")
	}
	base := filepath.Dir(file)
	path := filepath.Join(base, "..", "parity", "testdata", "oracle", "expected", "forge", "loop-lifecycle", "summary.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read lifecycle fixture: %v", err)
	}
	return string(data)
}

func prettyJSON(t *testing.T, value any) string {
	t.Helper()
	out, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		t.Fatalf("marshal json: %v", err)
	}
	return string(out)
}
