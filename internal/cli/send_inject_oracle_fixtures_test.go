package cli

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/tOgg1/forge/internal/agent"
	"github.com/tOgg1/forge/internal/config"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

type oracleSendInjectReport struct {
	Steps []oracleSendInjectStep `json:"steps"`
}

type oracleSendInjectStep struct {
	Name   string              `json:"name"`
	Stdout string              `json:"stdout,omitempty"`
	Stderr string              `json:"stderr,omitempty"`
	Error  string              `json:"error,omitempty"`
	State  oracleSendInjectDB  `json:"state"`
	Calls  []oracleInjectCall  `json:"inject_calls,omitempty"`
}

type oracleSendInjectDB struct {
	Queues map[string][]oracleQueueItem `json:"queues,omitempty"`
}

type oracleQueueItem struct {
	Type     models.QueueItemType   `json:"type"`
	Status   models.QueueItemStatus `json:"status"`
	Position int                    `json:"position"`
	Payload  map[string]any         `json:"payload,omitempty"`
}

type oracleInjectCall struct {
	AgentID       string `json:"agent_id"`
	Message       string `json:"message"`
	SkipIdleCheck bool   `json:"skip_idle_check"`
}

type agentMessageSenderFunc func(ctx context.Context, agentID string, message string, opts *agent.SendMessageOptions) error

func (f agentMessageSenderFunc) SendMessage(ctx context.Context, agentID string, message string, opts *agent.SendMessageOptions) error {
	return f(ctx, agentID, message, opts)
}

func TestOracleSendInjectFixtures(t *testing.T) {
	if testing.Short() {
		t.Skip("oracle fixtures are integration-style; skip in -short")
	}

	repo := t.TempDir()
	cleanupConfig := withTempConfig(t, repo)
	defer cleanupConfig()

	withWorkingDir(t, repo, func() {
		restoreGlobals := snapshotCLIFlags()
		defer restoreGlobals()

		// Keep outputs deterministic and avoid fmail side-effects.
		t.Setenv("FMAIL_AGENT", "")
		jsonOutput = true
		jsonlOutput = false
		noColor = true
		quiet = true
		yesFlag = true
		nonInteractive = true

		// Ensure config dirs exist (withTempConfig handles mkdirs) and DB is migrated.
		if appConfig == nil {
			cfg := config.DefaultConfig()
			cfg.Global.DataDir = filepath.Join(repo, "data")
			cfg.Global.ConfigDir = filepath.Join(repo, "config")
			appConfig = cfg
		}
		if err := os.MkdirAll(appConfig.Global.DataDir, 0o755); err != nil {
			t.Fatalf("mkdir data dir: %v", err)
		}

		setupSendInjectOracleState(t, repo)

		// Mock injection sender to avoid tmux dependency.
		var calls []oracleInjectCall
		prevSender := newAgentMessageSender
		newAgentMessageSender = func(*db.DB) agentMessageSender {
			return agentMessageSenderFunc(func(_ context.Context, agentID, message string, opts *agent.SendMessageOptions) error {
				call := oracleInjectCall{
					AgentID:       agentID,
					Message:       message,
					SkipIdleCheck: opts != nil && opts.SkipIdleCheck,
				}
				calls = append(calls, call)
				return nil
			})
		}
		t.Cleanup(func() { newAgentMessageSender = prevSender })

		var report oracleSendInjectReport

		// 1) send message (queue)
		resetSendFlags()
		stdout, stderr, runErr := captureStdoutStderr(func() error {
			return sendCmd.RunE(sendCmd, []string{"oracle-agent-idle", "hello from oracle"})
		})
		if runErr != nil {
			t.Fatalf("send: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleSendInjectStep{
			Name:   "send",
			Stdout: normalizeSendInjectJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotSendInjectState(t, "oracle-agent-idle"),
		})

		// 2) send when-idle (conditional queue item)
		resetSendFlags()
		sendWhenIdle = true
		stdout, stderr, runErr = captureStdoutStderr(func() error {
			return sendCmd.RunE(sendCmd, []string{"oracle-agent-idle", "continue when ready"})
		})
		if runErr != nil {
			t.Fatalf("send --when-idle: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleSendInjectStep{
			Name:   "send --when-idle",
			Stdout: normalizeSendInjectJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotSendInjectState(t, "oracle-agent-idle"),
		})

		// 3) send priority high (front insertion)
		resetSendFlags()
		sendPriority = "high"
		stdout, stderr, runErr = captureStdoutStderr(func() error {
			return sendCmd.RunE(sendCmd, []string{"oracle-agent-idle", "urgent"})
		})
		if runErr != nil {
			t.Fatalf("send --priority high: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleSendInjectStep{
			Name:   "send --priority high",
			Stdout: normalizeSendInjectJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotSendInjectState(t, "oracle-agent-idle"),
		})

		// 4) send after stable queue item id.
		withDB(t, func(database *db.DB) {
			queueRepo := db.NewQueueRepository(database)
			payload, _ := json.Marshal(models.MessagePayload{Text: "seed"})
			if err := queueRepo.Enqueue(context.Background(), "oracle-agent-after", &models.QueueItem{
				ID:     "oracle-after-item",
				Type:   models.QueueItemTypeMessage,
				Status: models.QueueItemStatusPending,
				Payload: payload,
			}); err != nil {
				t.Fatalf("enqueue seed: %v", err)
			}
		})
		resetSendFlags()
		sendAfter = "oracle-after-item"
		stdout, stderr, runErr = captureStdoutStderr(func() error {
			return sendCmd.RunE(sendCmd, []string{"oracle-agent-after", "after seed"})
		})
		if runErr != nil {
			t.Fatalf("send --after: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleSendInjectStep{
			Name:   "send --after",
			Stdout: normalizeSendInjectJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotSendInjectState(t, "oracle-agent-after"),
		})

		// 5) inject busy agent without --force in non-interactive mode -> error.
		resetInjectFlags()
		stdout, stderr, runErr = captureStdoutStderr(func() error {
			return injectCmd.RunE(injectCmd, []string{"oracle-agent-busy", "stop"})
		})
		if runErr == nil {
			t.Fatalf("inject busy: expected error")
		}
		report.Steps = append(report.Steps, oracleSendInjectStep{
			Name:   "inject busy (no --force)",
			Stdout: strings.TrimSpace(stdout),
			Stderr: strings.TrimSpace(stderr),
			Error:  runErr.Error(),
			State:  snapshotSendInjectState(t, "oracle-agent-busy"),
			Calls:  append([]oracleInjectCall(nil), calls...),
		})

		// 6) inject idle agent (bypasses queue; sender mocked).
		resetInjectFlags()
		stdout, stderr, runErr = captureStdoutStderr(func() error {
			return injectCmd.RunE(injectCmd, []string{"oracle-agent-idle", "ping"})
		})
		if runErr != nil {
			t.Fatalf("inject idle: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleSendInjectStep{
			Name:   "inject idle",
			Stdout: normalizeSendInjectJSONText(t, stdout),
			Stderr: strings.TrimSpace(stderr),
			State:  snapshotSendInjectState(t, "oracle-agent-idle"),
			Calls:  append([]oracleInjectCall(nil), calls...),
		})

		got := mustMarshalJSON(t, report)
		goldenPath := oracleSendInjectGoldenPath(t)

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

func resetSendFlags() {
	sendPriority = "normal"
	sendAfter = ""
	sendFront = false
	sendWhenIdle = false
	sendAll = false
	sendImmediate = false
	sendSkipIdle = false
	sendFile = ""
	sendStdin = false
	sendEditor = false
}

func resetInjectFlags() {
	injectForce = false
	injectFile = ""
	injectStdin = false
	injectEditor = false
}

func setupSendInjectOracleState(t *testing.T, repo string) {
	t.Helper()
	withDB(t, func(database *db.DB) {
		ctx := context.Background()
		nodeRepo := db.NewNodeRepository(database)
		wsRepo := db.NewWorkspaceRepository(database)
		agentRepo := db.NewAgentRepository(database)

		node := &models.Node{
			ID:         "oracle-node",
			Name:       "oracle-node",
			SSHBackend: models.SSHBackendAuto,
			Status:     models.NodeStatusUnknown,
			IsLocal:    true,
		}
		if err := nodeRepo.Create(ctx, node); err != nil {
			t.Fatalf("create node: %v", err)
		}

		ws := &models.Workspace{
			ID:          "oracle-ws",
			NodeID:      node.ID,
			Name:        "Oracle Workspace",
			RepoPath:    repo,
			TmuxSession: "oracle-session",
		}
		if err := wsRepo.Create(ctx, ws); err != nil {
			t.Fatalf("create workspace: %v", err)
		}

		idle := &models.Agent{
			ID:          "oracle-agent-idle",
			WorkspaceID: ws.ID,
			Type:        models.AgentTypeOpenCode,
			TmuxPane:    "oracle:0.0",
			State:       models.AgentStateIdle,
		}
		if err := agentRepo.Create(ctx, idle); err != nil {
			t.Fatalf("create idle agent: %v", err)
		}

		busy := &models.Agent{
			ID:          "oracle-agent-busy",
			WorkspaceID: ws.ID,
			Type:        models.AgentTypeOpenCode,
			TmuxPane:    "oracle:0.1",
			State:       models.AgentStateWorking,
		}
		if err := agentRepo.Create(ctx, busy); err != nil {
			t.Fatalf("create busy agent: %v", err)
		}

		after := &models.Agent{
			ID:          "oracle-agent-after",
			WorkspaceID: ws.ID,
			Type:        models.AgentTypeOpenCode,
			TmuxPane:    "oracle:0.2",
			State:       models.AgentStateIdle,
		}
		if err := agentRepo.Create(ctx, after); err != nil {
			t.Fatalf("create after agent: %v", err)
		}
	})
}

func snapshotSendInjectState(t *testing.T, agentIDs ...string) oracleSendInjectDB {
	t.Helper()
	state := oracleSendInjectDB{Queues: map[string][]oracleQueueItem{}}
	withDB(t, func(database *db.DB) {
		ctx := context.Background()
		queueRepo := db.NewQueueRepository(database)
		for _, agentID := range agentIDs {
			items, err := queueRepo.List(ctx, agentID)
			if err != nil {
				t.Fatalf("list queue %s: %v", agentID, err)
			}
			out := make([]oracleQueueItem, 0, len(items))
			for _, item := range items {
				var payload map[string]any
				if len(item.Payload) > 0 {
					_ = json.Unmarshal(item.Payload, &payload)
				}
				out = append(out, oracleQueueItem{
					Type:     item.Type,
					Status:   item.Status,
					Position: item.Position,
					Payload:  payload,
				})
			}
			state.Queues[agentID] = out
		}
	})
	return state
}

func normalizeSendInjectJSONText(t *testing.T, raw string) string {
	t.Helper()
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}

	var v any
	if err := json.Unmarshal([]byte(raw), &v); err != nil {
		t.Fatalf("unmarshal json: %v\nraw:\n%s", err, raw)
	}
	v = normalizeSendInjectJSONValue(v)
	out, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		t.Fatalf("marshal json: %v", err)
	}
	return string(out) + "\n"
}

func normalizeSendInjectJSONValue(v any) any {
	switch vv := v.(type) {
	case map[string]any:
		out := make(map[string]any, len(vv))
		for k, val := range vv {
			switch k {
			case "item_id":
				out[k] = "<ITEM_ID>"
			default:
				out[k] = normalizeSendInjectJSONValue(val)
			}
		}
		return out
	case []any:
		out := make([]any, 0, len(vv))
		for _, item := range vv {
			out = append(out, normalizeSendInjectJSONValue(item))
		}
		return out
	default:
		return v
	}
}

func oracleSendInjectGoldenPath(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve test file path")
	}
	base := filepath.Dir(file)
	return filepath.Join(base, "testdata", "oracle", "send_inject.json")
}

