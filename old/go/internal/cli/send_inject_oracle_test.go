package cli

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/tOgg1/forge/internal/agent"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

type sendInjectOracleCall struct {
	AgentID       string `json:"agent_id"`
	Message       string `json:"message"`
	SkipIdleCheck bool   `json:"skip_idle_check"`
}

type sendInjectOracleMockSender struct {
	stateByAgent map[string]models.AgentState
	calls        []sendInjectOracleCall
}

func (m *sendInjectOracleMockSender) SendMessage(_ context.Context, agentID string, message string, opts *agent.SendMessageOptions) error {
	skipIdle := opts != nil && opts.SkipIdleCheck
	m.calls = append(m.calls, sendInjectOracleCall{
		AgentID:       agentID,
		Message:       message,
		SkipIdleCheck: skipIdle,
	})

	if state, ok := m.stateByAgent[agentID]; ok && state != models.AgentStateIdle && !skipIdle {
		return agent.ErrAgentNotIdle
	}
	return nil
}

func TestSendInjectOracleScenarioMatchesFixture(t *testing.T) {
	repo := t.TempDir()
	cleanupConfig := withTempConfig(t, repo)
	defer cleanupConfig()
	t.Setenv("FMAIL_AGENT", "")

	withWorkingDir(t, repo, func() {
		restore := snapshotSendInjectGlobals()
		defer restore()

		jsonOutput = true
		jsonlOutput = false
		quiet = true
		noColor = true
		yesFlag = true
		nonInteractive = true

		ids := seedSendInjectOracleDB(t, repo)

		mockSender := &sendInjectOracleMockSender{
			stateByAgent: map[string]models.AgentState{
				ids.idleAgentID: models.AgentStateIdle,
				ids.busyAgentID: models.AgentStateWorking,
			},
		}
		prevSender := newAgentMessageSender
		newAgentMessageSender = func(*db.DB) agentMessageSender { return mockSender }
		t.Cleanup(func() { newAgentMessageSender = prevSender })

		resetSendOracleFlags()
		sendNormalOut, err := captureStdout(func() error {
			return sendCmd.RunE(sendCmd, []string{ids.idleAgentID, "normal queued"})
		})
		if err != nil {
			t.Fatalf("send normal: %v", err)
		}
		sendNormalResult := firstSendResult(t, decodeJSONMap(t, sendNormalOut))
		firstItemID := toStringValue(sendNormalResult["item_id"])

		resetSendOracleFlags()
		sendFront = true
		sendFrontOut, err := captureStdout(func() error {
			return sendCmd.RunE(sendCmd, []string{ids.idleAgentID, "front queued"})
		})
		if err != nil {
			t.Fatalf("send front: %v", err)
		}
		sendFrontResult := firstSendResult(t, decodeJSONMap(t, sendFrontOut))

		resetSendOracleFlags()
		sendWhenIdle = true
		sendWhenIdleOut, err := captureStdout(func() error {
			return sendCmd.RunE(sendCmd, []string{ids.idleAgentID, "idle queued"})
		})
		if err != nil {
			t.Fatalf("send when-idle: %v", err)
		}
		sendWhenIdleResult := firstSendResult(t, decodeJSONMap(t, sendWhenIdleOut))

		resetSendOracleFlags()
		sendAfter = firstItemID
		sendAfterOut, err := captureStdout(func() error {
			return sendCmd.RunE(sendCmd, []string{ids.idleAgentID, "after queued"})
		})
		if err != nil {
			t.Fatalf("send after: %v", err)
		}
		sendAfterResult := firstSendResult(t, decodeJSONMap(t, sendAfterOut))

		queueBefore := snapshotAgentQueue(t, ids.idleAgentID)

		resetSendOracleFlags()
		sendImmediate = true
		immediateBlockedOut, err := captureStdout(func() error {
			return sendCmd.RunE(sendCmd, []string{ids.busyAgentID, "immediate blocked"})
		})
		if err != nil {
			t.Fatalf("send immediate blocked: %v", err)
		}
		immediateBlockedResult := firstSendResult(t, decodeJSONMap(t, immediateBlockedOut))

		resetSendOracleFlags()
		sendImmediate = true
		sendSkipIdle = true
		immediateForcedOut, err := captureStdout(func() error {
			return sendCmd.RunE(sendCmd, []string{ids.busyAgentID, "immediate forced"})
		})
		if err != nil {
			t.Fatalf("send immediate forced: %v", err)
		}
		immediateForcedResult := firstSendResult(t, decodeJSONMap(t, immediateForcedOut))

		resetInjectOracleFlags()
		injectErr := injectCmd.RunE(injectCmd, []string{ids.busyAgentID, "inject blocked"})

		resetInjectOracleFlags()
		injectForce = true
		injectForcedOut, err := captureStdout(func() error {
			return injectCmd.RunE(injectCmd, []string{ids.busyAgentID, "inject forced"})
		})
		if err != nil {
			t.Fatalf("inject forced: %v", err)
		}
		injectForcedResult := decodeJSONMap(t, injectForcedOut)

		queueAfter := snapshotAgentQueue(t, ids.idleAgentID)

		summary := map[string]any{
			"send_queue": map[string]any{
				"normal": map[string]any{
					"position": intValue(sendNormalResult["position"]),
					"type":     toStringValue(sendNormalResult["item_type"]),
				},
				"front": map[string]any{
					"position": intValue(sendFrontResult["position"]),
					"type":     toStringValue(sendFrontResult["item_type"]),
				},
				"when_idle": map[string]any{
					"position": intValue(sendWhenIdleResult["position"]),
					"type":     toStringValue(sendWhenIdleResult["item_type"]),
				},
				"after": map[string]any{
					"position": intValue(sendAfterResult["position"]),
					"type":     toStringValue(sendAfterResult["item_type"]),
				},
				"pending_before_dispatch": len(queueBefore),
				"pending_after_dispatch":  len(queueAfter),
				"order_before_dispatch":   queueBefore,
				"order_after_dispatch":    queueAfter,
			},
			"send_immediate": map[string]any{
				"blocked_error": toStringValue(immediateBlockedResult["error"]),
				"forced_error":  toStringValue(immediateForcedResult["error"]),
			},
			"inject": map[string]any{
				"blocked_error":          errorString(injectErr),
				"forced_injected":        boolValue(injectForcedResult["injected"]),
				"forced_bypassed_queue":  boolValue(injectForcedResult["bypassed_queue"]),
				"forced_agent_state":     toStringValue(injectForcedResult["agent_state"]),
				"forced_message":         toStringValue(injectForcedResult["message"]),
				"forced_target_agent_id": toStringValue(injectForcedResult["agent_id"]),
			},
			"mock_calls": mockSender.calls,
		}

		got := prettyJSON(t, summary)
		if maybeUpdateSendInjectFixture(t, got) {
			return
		}
		want := readSendInjectFixture(t)
		if got != want {
			t.Fatalf("send/inject oracle fixture drift\nwant:\n%s\ngot:\n%s", want, got)
		}
	})
}

type sendInjectOracleIDs struct {
	idleAgentID string
	busyAgentID string
}

func seedSendInjectOracleDB(t *testing.T, repo string) sendInjectOracleIDs {
	t.Helper()

	database, err := openDatabase()
	if err != nil {
		t.Fatalf("open database: %v", err)
	}
	defer database.Close()

	ctx := context.Background()
	nodeRepo := db.NewNodeRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)
	agentRepo := db.NewAgentRepository(database)

	node := &models.Node{
		Name:       "oracle-send-inject-node",
		SSHBackend: models.SSHBackendAuto,
		Status:     models.NodeStatusUnknown,
		IsLocal:    true,
	}
	if err := nodeRepo.Create(ctx, node); err != nil {
		t.Fatalf("create node: %v", err)
	}

	ws := &models.Workspace{
		ID:          "ws-send-inject-oracle",
		NodeID:      node.ID,
		Name:        "ws-send-inject-oracle",
		RepoPath:    repo,
		TmuxSession: "oracle-send-inject",
	}
	if err := wsRepo.Create(ctx, ws); err != nil {
		t.Fatalf("create workspace: %v", err)
	}

	idle := &models.Agent{
		ID:          "agent-send-inject-idle",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "oracle-send-inject:0.0",
		State:       models.AgentStateIdle,
	}
	if err := agentRepo.Create(ctx, idle); err != nil {
		t.Fatalf("create idle agent: %v", err)
	}

	busy := &models.Agent{
		ID:          "agent-send-inject-busy",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "oracle-send-inject:0.1",
		State:       models.AgentStateWorking,
	}
	if err := agentRepo.Create(ctx, busy); err != nil {
		t.Fatalf("create busy agent: %v", err)
	}

	return sendInjectOracleIDs{
		idleAgentID: idle.ID,
		busyAgentID: busy.ID,
	}
}

func snapshotAgentQueue(t *testing.T, agentID string) []map[string]any {
	t.Helper()

	database, err := openDatabase()
	if err != nil {
		t.Fatalf("open database for queue snapshot: %v", err)
	}
	defer database.Close()

	queueRepo := db.NewQueueRepository(database)
	items, err := queueRepo.List(context.Background(), agentID)
	if err != nil {
		t.Fatalf("list queue: %v", err)
	}

	rows := make([]map[string]any, 0, len(items))
	for _, item := range items {
		row := map[string]any{
			"type":     string(item.Type),
			"position": item.Position,
		}
		switch item.Type {
		case models.QueueItemTypeMessage:
			payload, err := item.GetMessagePayload()
			if err != nil {
				t.Fatalf("decode message payload: %v", err)
			}
			row["text"] = payload.Text
		case models.QueueItemTypeConditional:
			payload, err := item.GetConditionalPayload()
			if err != nil {
				t.Fatalf("decode conditional payload: %v", err)
			}
			row["text"] = payload.Message
			row["condition_type"] = string(payload.ConditionType)
		}
		rows = append(rows, row)
	}
	return rows
}

func firstSendResult(t *testing.T, output map[string]any) map[string]any {
	t.Helper()
	raw, ok := output["results"].([]any)
	if !ok || len(raw) == 0 {
		t.Fatalf("send output missing results: %#v", output)
	}
	row, ok := raw[0].(map[string]any)
	if !ok {
		t.Fatalf("send result has unexpected shape: %#v", raw[0])
	}
	return row
}

func errorString(err error) string {
	if err == nil {
		return ""
	}
	return err.Error()
}

func snapshotSendInjectGlobals() func() {
	prev := struct {
		jsonOutput          bool
		jsonlOutput         bool
		quiet               bool
		noColor             bool
		yesFlag             bool
		nonInteractive      bool
		newAgentMessageFunc func(*db.DB) agentMessageSender
	}{
		jsonOutput:          jsonOutput,
		jsonlOutput:         jsonlOutput,
		quiet:               quiet,
		noColor:             noColor,
		yesFlag:             yesFlag,
		nonInteractive:      nonInteractive,
		newAgentMessageFunc: newAgentMessageSender,
	}

	return func() {
		jsonOutput = prev.jsonOutput
		jsonlOutput = prev.jsonlOutput
		quiet = prev.quiet
		noColor = prev.noColor
		yesFlag = prev.yesFlag
		nonInteractive = prev.nonInteractive
		newAgentMessageSender = prev.newAgentMessageFunc
		resetSendOracleFlags()
		resetInjectOracleFlags()
	}
}

func resetSendOracleFlags() {
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

func resetInjectOracleFlags() {
	injectForce = false
	injectFile = ""
	injectStdin = false
	injectEditor = false
}

func maybeUpdateSendInjectFixture(t *testing.T, body string) bool {
	t.Helper()
	if os.Getenv("FORGE_UPDATE_GOLDENS") != "1" {
		return false
	}

	for _, path := range sendInjectFixturePaths(t) {
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatalf("mkdir fixture dir: %v", err)
		}
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			t.Fatalf("write fixture %s: %v", path, err)
		}
	}
	return true
}

func readSendInjectFixture(t *testing.T) string {
	t.Helper()
	paths := sendInjectFixturePaths(t)
	data, err := os.ReadFile(paths[0])
	if err != nil {
		t.Fatalf("read send/inject fixture: %v", err)
	}
	return strings.TrimSpace(string(data))
}

func sendInjectFixturePaths(t *testing.T) []string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve caller path")
	}
	base := filepath.Dir(file)
	return []string{
		filepath.Join(base, "..", "parity", "testdata", "oracle", "expected", "forge", "send-inject", "summary.json"),
		filepath.Join(base, "..", "parity", "testdata", "oracle", "actual", "forge", "send-inject", "summary.json"),
	}
}
