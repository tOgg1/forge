package cli

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
	"github.com/tOgg1/forge/internal/queue"
	"github.com/tOgg1/forge/internal/testutil"
)

func TestSendInjectOracleScenarioMatchesFixture(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	ws := createTestWorkspaceForWait(t, database, "ws_send_inject_oracle")
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	queueService := queue.NewService(queueRepo)

	agentEntry := &models.Agent{
		ID:          "agent_send_inject_oracle",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "oracle:0.0",
		State:       models.AgentStateIdle,
	}
	if err := agentRepo.Create(ctx, agentEntry); err != nil {
		t.Fatalf("create agent: %v", err)
	}

	first := enqueueMessage(ctx, queueService, queueRepo, agentEntry, "send-default", queueOptions{})
	second := enqueueMessage(
		ctx,
		queueService,
		queueRepo,
		agentEntry,
		"send-when-idle",
		queueOptions{WhenIdle: true},
	)
	third := enqueueMessage(
		ctx,
		queueService,
		queueRepo,
		agentEntry,
		"send-front",
		queueOptions{Front: true},
	)

	items, err := queueRepo.List(ctx, agentEntry.ID)
	if err != nil {
		t.Fatalf("queue list: %v", err)
	}

	queueSnapshot := make([]string, 0, len(items))
	for index, item := range items {
		switch item.Type {
		case models.QueueItemTypeMessage:
			var payload models.MessagePayload
			if err := json.Unmarshal(item.Payload, &payload); err != nil {
				t.Fatalf("decode message payload: %v", err)
			}
			queueSnapshot = append(queueSnapshot, itemSnapshot(index+1, item.Type, payload.Text, ""))
		case models.QueueItemTypeConditional:
			var payload models.ConditionalPayload
			if err := json.Unmarshal(item.Payload, &payload); err != nil {
				t.Fatalf("decode conditional payload: %v", err)
			}
			queueSnapshot = append(
				queueSnapshot,
				itemSnapshot(index+1, item.Type, payload.Message, payload.ConditionType),
			)
		default:
			t.Fatalf("unexpected queue item type: %s", item.Type)
		}
	}

	summary := map[string]any{
		"send_results": []map[string]any{
			{
				"position":  first.Position,
				"item_type": first.ItemType,
			},
			{
				"position":  second.Position,
				"item_type": second.ItemType,
			},
			{
				"position":  third.Position,
				"item_type": third.ItemType,
			},
		},
		"queue_snapshot": queueSnapshot,
		"inject_readiness": map[string]bool{
			"idle":              isAgentReadyForInject(&models.Agent{State: models.AgentStateIdle}),
			"working":           isAgentReadyForInject(&models.Agent{State: models.AgentStateWorking}),
			"awaiting_approval": isAgentReadyForInject(&models.Agent{State: models.AgentStateAwaitingApproval}),
			"paused":            isAgentReadyForInject(&models.Agent{State: models.AgentStatePaused}),
		},
	}

	want := decodeJSONMap(t, readSendInjectFixture(t))
	if prettyJSON(t, summary) != prettyJSON(t, want) {
		t.Fatalf("send/inject fixture drift\nwant:\n%s\ngot:\n%s", prettyJSON(t, want), prettyJSON(t, summary))
	}
}

func itemSnapshot(position int, itemType models.QueueItemType, message string, condition models.ConditionType) string {
	if condition == "" {
		return formatQueueSnapshot(position, itemType, message, "")
	}
	return formatQueueSnapshot(position, itemType, message, string(condition))
}

func formatQueueSnapshot(position int, itemType models.QueueItemType, message, condition string) string {
	if condition == "" {
		return fmt.Sprintf("%d|%s|%s", position, itemType, message)
	}
	return fmt.Sprintf("%d|%s|%s|%s", position, itemType, message, condition)
}

func readSendInjectFixture(t *testing.T) string {
	t.Helper()

	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve test path")
	}
	path := filepath.Join(filepath.Dir(file), "testdata", "oracle", "send_inject.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}
	return string(data)
}
