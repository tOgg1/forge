package cli

import (
	"context"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"testing"

	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/logging"
	"github.com/tOgg1/forge/internal/models"
)

type memWorkOracleReport struct {
	Steps []memWorkOracleStep `json:"steps"`
}

type memWorkOracleStep struct {
	Name   string          `json:"name"`
	Stdout string          `json:"stdout,omitempty"`
	Stderr string          `json:"stderr,omitempty"`
	State  memWorkDBState  `json:"state"`
}

type memWorkDBState struct {
	MemKeys     []string           `json:"mem_keys,omitempty"`
	MemSnapshot map[string]string  `json:"mem_snapshot,omitempty"`
	WorkCurrent *memWorkCurrent    `json:"work_current,omitempty"`
	WorkHistory []memWorkHistory   `json:"work_history,omitempty"`
}

type memWorkCurrent struct {
	Agent  string `json:"agent"`
	Task   string `json:"task"`
	Status string `json:"status"`
	Detail string `json:"detail,omitempty"`
	Iter   int    `json:"iter"`
}

type memWorkHistory struct {
	Current bool   `json:"current"`
	Agent   string `json:"agent"`
	Task    string `json:"task"`
	Status  string `json:"status"`
	Detail  string `json:"detail,omitempty"`
	Iter    int    `json:"iter"`
}

func TestLoopMemWorkOracleScenarioMatchesFixture(t *testing.T) {
	repo := t.TempDir()
	cleanupConfig := withTempConfig(t, repo)
	defer cleanupConfig()

	withWorkingDir(t, repo, func() {
		restore := snapshotMemWorkGlobals(t)
		defer restore()

		restoreLogging := silenceLogging(t)
		defer restoreLogging()

		// Avoid side effects from fmail topic broadcasts during fixture capture.
		// Clear FORGE_LOOP_ID so default loop-ref resolution uses the local fixture loop.
		restoreEnv := withoutEnv(t, []string{"FMAIL_AGENT", "FORGE_LOOP_ID"})
		defer restoreEnv()

		if err := os.Setenv("FORGE_LOOP_NAME", "oracle-loop"); err != nil {
			t.Fatalf("set FORGE_LOOP_NAME: %v", err)
		}
		t.Cleanup(func() { _ = os.Unsetenv("FORGE_LOOP_NAME") })

		// Stable JSON output + no prompts.
		jsonOutput = true
		jsonlOutput = false
		noColor = true
		quiet = false
		yesFlag = true
		nonInteractive = true

		seedOracleLoop(t, repo, "oracle-loop")

		workAgentID = "oracle-agent"
		workStatus = "in_progress"
		workDetail = ""

		var report memWorkOracleReport

		// mem: empty -> set -> get -> ls -> rm -> ls
		memLoopRef = "oracle-loop"
		stdout, stderr, err := captureStdoutStderr(func() error { return memListCmd.RunE(memListCmd, nil) })
		assertNoErr(t, "mem ls (empty)", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "mem ls (empty)",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		stdout, stderr, err = captureStdoutStderr(func() error { return memSetCmd.RunE(memSetCmd, []string{"blocked_on", "agent-b"}) })
		assertNoErr(t, "mem set", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "mem set blocked_on",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		stdout, stderr, err = captureStdoutStderr(func() error { return memGetCmd.RunE(memGetCmd, []string{"blocked_on"}) })
		assertNoErr(t, "mem get", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "mem get blocked_on",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		stdout, stderr, err = captureStdoutStderr(func() error { return memListCmd.RunE(memListCmd, nil) })
		assertNoErr(t, "mem ls", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "mem ls (1 item)",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		stdout, stderr, err = captureStdoutStderr(func() error { return memRmCmd.RunE(memRmCmd, []string{"blocked_on"}) })
		assertNoErr(t, "mem rm", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "mem rm blocked_on",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		stdout, stderr, err = captureStdoutStderr(func() error { return memListCmd.RunE(memListCmd, nil) })
		assertNoErr(t, "mem ls (empty after rm)", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "mem ls (empty after rm)",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		// work: current (none) -> set -> current -> ls -> clear -> current (none)
		workLoopRef = "oracle-loop"
		stdout, stderr, err = captureStdoutStderr(func() error { return workCurrentCmd.RunE(workCurrentCmd, nil) })
		assertNoErr(t, "work current (none)", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "work current (none)",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		workStatus = "in_progress"
		workDetail = "port mem/work fixtures"
		stdout, stderr, err = captureStdoutStderr(func() error { return workSetCmd.RunE(workSetCmd, []string{"sv-123"}) })
		assertNoErr(t, "work set", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "work set sv-123",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		stdout, stderr, err = captureStdoutStderr(func() error { return workCurrentCmd.RunE(workCurrentCmd, nil) })
		assertNoErr(t, "work current", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "work current",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		stdout, stderr, err = captureStdoutStderr(func() error { return workListCmd.RunE(workListCmd, nil) })
		assertNoErr(t, "work ls", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "work ls",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		stdout, stderr, err = captureStdoutStderr(func() error { return workClearCmd.RunE(workClearCmd, nil) })
		assertNoErr(t, "work clear", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "work clear",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		stdout, stderr, err = captureStdoutStderr(func() error { return workCurrentCmd.RunE(workCurrentCmd, nil) })
		assertNoErr(t, "work current (none after clear)", err, stderr)
		report.Steps = append(report.Steps, memWorkOracleStep{
			Name:   "work current (none after clear)",
			Stdout: normalizeJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotMemWorkState(t, "oracle-loop"),
		})

		got := mustMarshalJSON(t, report)
		goldenPath := memWorkGoldenPath(t)

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
			t.Fatalf("mem/work fixture drift: %s (set FORGE_UPDATE_GOLDENS=1 to regenerate)\n--- want\n%s\n--- got\n%s", goldenPath, want, got)
		}
	})
}

func memWorkGoldenPath(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve test file path")
	}
	base := filepath.Dir(file)
	return filepath.Join(base, "testdata", "oracle", "mem_work.json")
}

func seedOracleLoop(t *testing.T, repoDir, loopName string) {
	t.Helper()
	database, err := openDatabase()
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	defer database.Close()

	loopRepo := db.NewLoopRepository(database)
	entry := &models.Loop{
		ID:              "oracle-loop-id",
		Name:            loopName,
		RepoPath:        repoDir,
		IntervalSeconds: 30,
		State:           models.LoopStateStopped,
	}
	if err := loopRepo.Create(context.Background(), entry); err != nil {
		t.Fatalf("create loop: %v", err)
	}
}

func snapshotMemWorkState(t *testing.T, loopName string) memWorkDBState {
	t.Helper()

	database, err := openDatabase()
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	defer database.Close()

	loopRepo := db.NewLoopRepository(database)
	loopEntry, err := loopRepo.GetByName(context.Background(), loopName)
	if err != nil {
		t.Fatalf("get loop: %v", err)
	}

	kvRepo := db.NewLoopKVRepository(database)
	kvs, err := kvRepo.ListByLoop(context.Background(), loopEntry.ID)
	if err != nil {
		t.Fatalf("list kv: %v", err)
	}

	state := memWorkDBState{}
	if len(kvs) > 0 {
		sort.Slice(kvs, func(i, j int) bool { return kvs[i].Key < kvs[j].Key })
		state.MemSnapshot = map[string]string{}
		for _, it := range kvs {
			state.MemKeys = append(state.MemKeys, it.Key)
			state.MemSnapshot[it.Key] = it.Value
		}
	}

	workRepo := db.NewLoopWorkStateRepository(database)
	cur, err := workRepo.GetCurrent(context.Background(), loopEntry.ID)
	if err == nil && cur != nil {
		state.WorkCurrent = &memWorkCurrent{
			Agent:  cur.AgentID,
			Task:   cur.TaskID,
			Status: cur.Status,
			Detail: cur.Detail,
			Iter:   cur.LoopIteration,
		}
	}

	history, err := workRepo.ListByLoop(context.Background(), loopEntry.ID, 10)
	if err == nil && len(history) > 0 {
		sort.Slice(history, func(i, j int) bool {
			if history[i].IsCurrent != history[j].IsCurrent {
				return history[i].IsCurrent
			}
			return history[i].UpdatedAt.After(history[j].UpdatedAt)
		})
		for _, it := range history {
			state.WorkHistory = append(state.WorkHistory, memWorkHistory{
				Current: it.IsCurrent,
				Agent:   it.AgentID,
				Task:    it.TaskID,
				Status:  it.Status,
				Detail:  it.Detail,
				Iter:    it.LoopIteration,
			})
		}
	}

	return state
}

func snapshotMemWorkGlobals(t *testing.T) func() {
	t.Helper()
	prev := struct {
		jsonOutput     bool
		jsonlOutput    bool
		quiet          bool
		noColor        bool
		yesFlag        bool
		nonInteractive bool

		memLoopRef string
		workLoopRef string
		workAgentID string
		workStatus  string
		workDetail  string
	}{
		jsonOutput:     jsonOutput,
		jsonlOutput:    jsonlOutput,
		quiet:          quiet,
		noColor:        noColor,
		yesFlag:        yesFlag,
		nonInteractive: nonInteractive,
		memLoopRef:     memLoopRef,
		workLoopRef:    workLoopRef,
		workAgentID:    workAgentID,
		workStatus:     workStatus,
		workDetail:     workDetail,
	}
	return func() {
		jsonOutput = prev.jsonOutput
		jsonlOutput = prev.jsonlOutput
		quiet = prev.quiet
		noColor = prev.noColor
		yesFlag = prev.yesFlag
		nonInteractive = prev.nonInteractive
		memLoopRef = prev.memLoopRef
		workLoopRef = prev.workLoopRef
		workAgentID = prev.workAgentID
		workStatus = prev.workStatus
		workDetail = prev.workDetail
	}
}

func withoutEnv(t *testing.T, keys []string) func() {
	t.Helper()
	type kv struct {
		key string
		val string
		had bool
	}
	prev := make([]kv, 0, len(keys))
	for _, key := range keys {
		val, had := os.LookupEnv(key)
		prev = append(prev, kv{key: key, val: val, had: had})
		_ = os.Unsetenv(key)
	}
	return func() {
		for _, it := range prev {
			if it.had {
				_ = os.Setenv(it.key, it.val)
			} else {
				_ = os.Unsetenv(it.key)
			}
		}
	}
}

func assertNoErr(t *testing.T, label string, err error, stderr string) {
	t.Helper()
	if err != nil {
		t.Fatalf("%s: %v\nstderr:\n%s", label, err, stderr)
	}
}

func silenceLogging(t *testing.T) func() {
	t.Helper()
	prev := logging.Logger
	logging.Init(logging.Config{Level: "error", Format: "console", Output: io.Discard})
	return func() { logging.Logger = prev }
}
