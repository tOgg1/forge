package cli

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"testing"

	forgedv1 "github.com/tOgg1/forge/gen/forged/v1"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/loop"
	"github.com/tOgg1/forge/internal/models"
)

type oracleLifecycleReport struct {
	Steps []oracleLifecycleStep `json:"steps"`
}

type oracleLifecycleStep struct {
	Name   string             `json:"name"`
	Stdout string             `json:"stdout,omitempty"`
	Stderr string             `json:"stderr,omitempty"`
	State  oracleLifecycleDB  `json:"state"`
}

type oracleLifecycleDB struct {
	Loops  []oracleLifecycleLoop             `json:"loops"`
	Queues map[string][]oracleLifecycleQueue `json:"queues,omitempty"`
}

type oracleLifecycleLoop struct {
	Name    string                 `json:"name"`
	State   models.LoopState       `json:"state"`
	Profile string                 `json:"profile,omitempty"`
	Pool    string                 `json:"pool,omitempty"`
	Tags    []string               `json:"tags,omitempty"`
	Meta    map[string]any         `json:"meta,omitempty"`
	Error   string                 `json:"last_error,omitempty"`
}

type oracleLifecycleQueue struct {
	Type     models.LoopQueueItemType   `json:"type"`
	Status   models.LoopQueueItemStatus `json:"status"`
	Position int                        `json:"position"`
	Payload  map[string]any             `json:"payload,omitempty"`
}

func TestOracleLoopLifecycleFixtures(t *testing.T) {
	if testing.Short() {
		t.Skip("oracle fixtures are integration-style; skip in -short")
	}

	repo := t.TempDir()
	cleanupConfig := withTempConfig(t, repo)
	defer cleanupConfig()

	withWorkingDir(t, repo, func() {
		restoreGlobals := snapshotCLIFlags()
		defer restoreGlobals()

		// Keep outputs deterministic.
		jsonOutput = true
		jsonlOutput = false
		noColor = true
		quiet = true
		yesFlag = true
		nonInteractive = true

		// Avoid any forged daemon probing in ps.
		prevDaemonLister := listDaemonRunnersFunc
		listDaemonRunnersFunc = func(context.Context) (map[string]*forgedv1.LoopRunner, bool) { return nil, false }
		t.Cleanup(func() { listDaemonRunnersFunc = prevDaemonLister })

		// Prevent spawning real runners.
		prevStart := startLoopRunnerFunc
		startLoopRunnerFunc = func(string, string, loopSpawnOwner) (loopRunnerStartResult, error) {
			return loopRunnerStartResult{Owner: loopSpawnOwnerLocal, InstanceID: "oracle-instance"}, nil
		}
		t.Cleanup(func() { startLoopRunnerFunc = prevStart })

		createOracleProfile(t)

		var report oracleLifecycleReport

		// 1) up
		resetLoopUpFlags()
		loopUpCount = 1
		loopUpName = "oracle-loop"
		loopUpProfile = "oracle-profile"
		loopUpSpawnOwner = string(loopSpawnOwnerLocal)
		stdout, stderr, runErr := captureStdoutStderr(func() error { return loopUpCmd.RunE(loopUpCmd, nil) })
		if runErr != nil {
			t.Fatalf("up: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "up",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})

		// 2) ps
		resetLoopPsFlags()
		stdout, stderr, runErr = captureStdoutStderr(func() error { return loopPsCmd.RunE(loopPsCmd, nil) })
		if runErr != nil {
			t.Fatalf("ps: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "ps",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})

		// 3) logs (non-json)
		jsonOutput = false
		writeOracleLog(t, "oracle-loop")
		resetLogsFlags()
		logsLines = 3
		stdout, stderr, runErr = captureStdoutStderr(func() error { return logsCmd.RunE(logsCmd, []string{"oracle-loop"}) })
		if runErr != nil {
			t.Fatalf("logs: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "logs",
			Stdout: stdout,
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})
		jsonOutput = true

		// 4) msg
		resetLoopMsgFlags()
		stdout, stderr, runErr = captureStdoutStderr(func() error {
			return loopMsgCmd.RunE(loopMsgCmd, []string{"oracle-loop", "hello from oracle"})
		})
		if runErr != nil {
			t.Fatalf("msg: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "msg",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})

		// 5) queue ls
		resetQueueFlags()
		stdout, stderr, runErr = captureStdoutStderr(func() error { return loopQueueListCmd.RunE(loopQueueListCmd, []string{"oracle-loop"}) })
		if runErr != nil {
			t.Fatalf("queue ls: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "queue ls",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})

		// 6) stop
		resetLoopStopFlags()
		stdout, stderr, runErr = captureStdoutStderr(func() error { return loopStopCmd.RunE(loopStopCmd, []string{"oracle-loop"}) })
		if runErr != nil {
			t.Fatalf("stop: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "stop",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})

		// 7) kill
		resetLoopKillFlags()
		stdout, stderr, runErr = captureStdoutStderr(func() error { return loopKillCmd.RunE(loopKillCmd, []string{"oracle-loop"}) })
		if runErr != nil {
			t.Fatalf("kill: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "kill",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})

		// 8) resume
		loopResumeSpawnOwner = string(loopSpawnOwnerLocal)
		stdout, stderr, runErr = captureStdoutStderr(func() error { return loopResumeCmd.RunE(loopResumeCmd, []string{"oracle-loop"}) })
		if runErr != nil {
			t.Fatalf("resume: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "resume",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})

		// 9) run
		jsonOutput = false
		stdout, stderr, runErr = captureStdoutStderr(func() error { return loopRunOnceCmd.RunE(loopRunOnceCmd, []string{"oracle-loop"}) })
		if runErr != nil {
			t.Fatalf("run: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "run",
			Stdout: stdout,
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})
		jsonOutput = true

		// 10) rm
		resetRmFlags()
		stdout, stderr, runErr = captureStdoutStderr(func() error { return loopRmCmd.RunE(loopRmCmd, []string{"oracle-loop"}) })
		if runErr != nil {
			t.Fatalf("rm: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "rm",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})

		// 11) clean (set up a cleanable loop, then clean)
		resetLoopUpFlags()
		loopUpCount = 1
		loopUpName = "oracle-clean-loop"
		loopUpProfile = "oracle-profile"
		loopUpSpawnOwner = string(loopSpawnOwnerLocal)
		_, _, runErr = captureStdoutStderr(func() error { return loopUpCmd.RunE(loopUpCmd, nil) })
		if runErr != nil {
			t.Fatalf("up(clean): %v", runErr)
		}
		resetCleanFlags()
		stdout, stderr, runErr = captureStdoutStderr(func() error { return loopCleanCmd.RunE(loopCleanCmd, nil) })
		if runErr != nil {
			t.Fatalf("clean: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "clean",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})

		// 12) scale (from zero -> 2)
		resetLoopScaleFlags()
		loopScaleCount = 2
		loopScaleNamePrefix = "oracle-scaled"
		loopScaleProfile = "oracle-profile"
		loopScaleSpawnOwner = string(loopSpawnOwnerLocal)
		stdout, stderr, runErr = captureStdoutStderr(func() error { return loopScaleCmd.RunE(loopScaleCmd, nil) })
		if runErr != nil {
			t.Fatalf("scale: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleLifecycleStep{
			Name:   "scale",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotLifecycleState(t),
		})

		got := mustMarshalJSON(t, report)
		goldenPath := oracleLifecycleGoldenPath(t)

		if os.Getenv("FORGE_UPDATE_GOLDENS") == "1" {
			if err := os.MkdirAll(filepath.Dir(goldenPath), 0o755); err != nil {
				t.Fatalf("mkdir golden dir: %v", err)
			}
			if err := os.WriteFile(goldenPath, []byte(got), 0o644); err != nil {
				t.Fatalf("write golden: %v", err)
			}
			return
		}

		wantBytes, err := os.ReadFile(goldenPath)
		if err != nil {
			t.Fatalf("read golden: %v (set FORGE_UPDATE_GOLDENS=1 to generate)", err)
		}
		want := string(wantBytes)

		if normalizeGolden(want) != normalizeGolden(got) {
			t.Fatalf("oracle fixture drift: %s (set FORGE_UPDATE_GOLDENS=1 to regenerate)\n--- want\n%s\n--- got\n%s", goldenPath, want, got)
		}
	})
}

func snapshotCLIFlags() func() {
	prev := struct {
		jsonOutput     bool
		jsonlOutput    bool
		quiet          bool
		noColor        bool
		yesFlag        bool
		nonInteractive bool
	}{jsonOutput, jsonlOutput, quiet, noColor, yesFlag, nonInteractive}
	return func() {
		jsonOutput = prev.jsonOutput
		jsonlOutput = prev.jsonlOutput
		quiet = prev.quiet
		noColor = prev.noColor
		yesFlag = prev.yesFlag
		nonInteractive = prev.nonInteractive
	}
}

func captureStdoutStderr(fn func() error) (string, string, error) {
	stdoutR, stdoutW, err := os.Pipe()
	if err != nil {
		return "", "", err
	}
	stderrR, stderrW, err := os.Pipe()
	if err != nil {
		_ = stdoutR.Close()
		_ = stdoutW.Close()
		return "", "", err
	}

	origOut := os.Stdout
	origErr := os.Stderr
	os.Stdout = stdoutW
	os.Stderr = stderrW

	runErr := fn()

	_ = stdoutW.Close()
	_ = stderrW.Close()
	os.Stdout = origOut
	os.Stderr = origErr

	var outBuf bytes.Buffer
	var errBuf bytes.Buffer
	_, _ = io.Copy(&outBuf, stdoutR)
	_, _ = io.Copy(&errBuf, stderrR)
	_ = stdoutR.Close()
	_ = stderrR.Close()

	return outBuf.String(), errBuf.String(), runErr
}

func normalizeJSONText(t *testing.T, raw string) string {
	t.Helper()

	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}

	var v any
	if err := json.Unmarshal([]byte(raw), &v); err != nil {
		t.Fatalf("unmarshal json: %v\nraw:\n%s", err, raw)
	}
	v = normalizeJSONValue(v)
	out, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		t.Fatalf("marshal json: %v", err)
	}
	return string(out) + "\n"
}

func normalizeJSONValue(v any) any {
	switch vv := v.(type) {
	case map[string]any:
		out := make(map[string]any, len(vv))
		for k, val := range vv {
			switch k {
			case "id":
				out[k] = "<ID>"
			case "loop_id":
				out[k] = "<LOOP_ID>"
			case "short_id":
				out[k] = "<SHORT_ID>"
			case "profile_id":
				out[k] = "<PROFILE_ID>"
			case "pool_id":
				out[k] = "<POOL_ID>"
			case "created_at", "updated_at", "last_run_at", "dispatched_at", "completed_at":
				out[k] = "<TIME>"
			case "repo_path":
				out[k] = "<REPO_PATH>"
			case "log_path":
				out[k] = "<LOG_PATH>"
			case "ledger_path":
				out[k] = "<LEDGER_PATH>"
			case "runner_instance_id":
				out[k] = "<RUNNER_INSTANCE_ID>"
			case "loop_ids":
				if arr, ok := val.([]any); ok {
					repl := make([]any, 0, len(arr))
					for range arr {
						repl = append(repl, "<LOOP_ID>")
					}
					out[k] = repl
				} else {
					out[k] = normalizeJSONValue(val)
				}
			default:
				out[k] = normalizeJSONValue(val)
			}
		}
		return out
	case []any:
		out := make([]any, 0, len(vv))
		for _, item := range vv {
			out = append(out, normalizeJSONValue(item))
		}
		return out
	default:
		return v
	}
}

func snapshotLifecycleState(t *testing.T) oracleLifecycleDB {
	t.Helper()
	var state oracleLifecycleDB
	withDB(t, func(database *db.DB) {
		ctx := context.Background()

		loopRepo := db.NewLoopRepository(database)
		queueRepo := db.NewLoopQueueRepository(database)

		loops, err := loopRepo.List(ctx)
		if err != nil {
			t.Fatalf("list loops: %v", err)
		}
		sort.Slice(loops, func(i, j int) bool { return loops[i].Name < loops[j].Name })

		state = oracleLifecycleDB{Queues: map[string][]oracleLifecycleQueue{}}
		for _, loopEntry := range loops {
			entry := oracleLifecycleLoop{
				Name:    loopEntry.Name,
				State:   loopEntry.State,
				Profile: loopEntry.ProfileID,
				Pool:    loopEntry.PoolID,
				Tags:    loopEntry.Tags,
				Error:   loopEntry.LastError,
			}
			if loopEntry.Metadata != nil {
				entry.Meta = map[string]any{}
				for k, v := range loopEntry.Metadata {
					switch k {
					case loopMetadataRunnerOwnerKey:
						entry.Meta[k] = v
					case loopMetadataRunnerInstanceIDKey:
						entry.Meta[k] = "<RUNNER_INSTANCE_ID>"
					default:
						// omit volatile metadata
					}
				}
				if len(entry.Meta) == 0 {
					entry.Meta = nil
				}
			}
			state.Loops = append(state.Loops, entry)

			items, err := queueRepo.List(ctx, loopEntry.ID)
			if err != nil {
				t.Fatalf("list queue: %v", err)
			}
			filtered := make([]oracleLifecycleQueue, 0, len(items))
			for _, item := range items {
				if item.Status != models.LoopQueueStatusPending {
					continue
				}
				row := oracleLifecycleQueue{
					Type:     item.Type,
					Status:   item.Status,
					Position: item.Position,
				}
				var payload map[string]any
				if err := json.Unmarshal(item.Payload, &payload); err == nil && len(payload) > 0 {
					row.Payload = payload
				}
				filtered = append(filtered, row)
			}
			if len(filtered) > 0 {
				state.Queues[loopEntry.Name] = filtered
			}
		}
		if len(state.Queues) == 0 {
			state.Queues = nil
		}
	})

	return state
}

func writeOracleLog(t *testing.T, loopName string) {
	t.Helper()
	withDB(t, func(database *db.DB) {
		ctx := context.Background()
		loopRepo := db.NewLoopRepository(database)
		loopEntry, err := loopRepo.GetByName(ctx, loopName)
		if err != nil {
			t.Fatalf("get loop: %v", err)
		}
		logPath := loopEntry.LogPath
		if logPath == "" {
			logPath = loop.LogPath(GetConfig().Global.DataDir, loopEntry.Name, loopEntry.ID)
		}
		if err := os.MkdirAll(filepath.Dir(logPath), 0o755); err != nil {
			t.Fatalf("mkdir log dir: %v", err)
		}
		body := strings.Join([]string{
			"[2025-01-01T00:00:00Z] loop started",
			"[2025-01-01T00:00:01Z] runner-ok",
			"[2025-01-01T00:00:02Z] loop stopped",
			"",
		}, "\n")
		if err := os.WriteFile(logPath, []byte(body), 0o644); err != nil {
			t.Fatalf("write log: %v", err)
		}
	})
}

func oracleLifecycleGoldenPath(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve test file path")
	}
	base := filepath.Dir(file)
	return filepath.Join(base, "testdata", "oracle", "loop_lifecycle.json")
}

func mustMarshalJSON(t *testing.T, v any) string {
	t.Helper()
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	return string(data) + "\n"
}

func normalizeGolden(s string) string {
	return strings.TrimSpace(strings.ReplaceAll(s, "\r\n", "\n"))
}

func resetLoopPsFlags() {
	loopPsRepo = ""
	loopPsPool = ""
	loopPsProfile = ""
	loopPsState = ""
	loopPsTag = ""
}

func resetLogsFlags() {
	logsFollow = false
	logsLines = 20
	logsSince = ""
	logsAll = false
}

func resetQueueFlags() {
	queueAll = false
	queueTo = "front"
}

func resetRmFlags() {
	loopRmAll = false
	loopRmRepo = ""
	loopRmPool = ""
	loopRmProfile = ""
	loopRmState = ""
	loopRmTag = ""
	loopRmForce = false
}

func resetCleanFlags() {
	loopCleanRepo = ""
	loopCleanPool = ""
	loopCleanProfile = ""
	loopCleanTag = ""
}

func withDB(t *testing.T, fn func(*db.DB)) {
	t.Helper()
	database, err := openDatabase()
	if err != nil {
		t.Fatalf("open database: %v", err)
	}
	defer database.Close()
	fn(database)
}

func createOracleProfile(t *testing.T) {
	t.Helper()
	withDB(t, func(database *db.DB) {
		ctx := context.Background()
		profileRepo := db.NewProfileRepository(database)
		profile := &models.Profile{
			ID:              "oracle-profile-id",
			Name:            "oracle-profile",
			PromptMode:      models.PromptModeEnv,
			CommandTemplate: "echo runner-ok",
		}
		if err := profileRepo.Create(ctx, profile); err != nil {
			t.Fatalf("create profile: %v", err)
		}
	})
}
