package cli

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/opencode-ai/swarm/internal/db"
	"github.com/opencode-ai/swarm/internal/models"
	"github.com/opencode-ai/swarm/internal/testutil"
)

// createTestWorkspaceForWait creates a workspace with all required fields for testing.
func createTestWorkspaceForWait(t *testing.T, database *db.DB, wsID string) *models.Workspace {
	t.Helper()

	ctx := context.Background()
	nodeRepo := db.NewNodeRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	node := &models.Node{
		Name:       "test-node-" + wsID,
		SSHBackend: models.SSHBackendAuto,
		Status:     models.NodeStatusUnknown,
		IsLocal:    true,
	}
	if err := nodeRepo.Create(ctx, node); err != nil {
		t.Fatalf("failed to create node: %v", err)
	}

	ws := &models.Workspace{
		ID:          wsID,
		NodeID:      node.ID,
		Name:        "Test Workspace",
		RepoPath:    "/test/path/" + wsID,
		TmuxSession: "test-session-" + wsID,
	}
	if err := wsRepo.Create(ctx, ws); err != nil {
		t.Fatalf("failed to create workspace: %v", err)
	}

	return ws
}

func TestCheckWaitCondition_Idle(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_idle_test")

	agent := &models.Agent{
		ID:          "agent_idle_test",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.0",
		State:       models.AgentStateIdle,
	}
	if err := agentRepo.Create(ctx, agent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	// Set global for the test
	waitAgent = agent.ID

	met, status, err := checkWaitCondition(ctx, WaitConditionIdle, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !met {
		t.Errorf("expected condition to be met for idle agent")
	}
	if status != "idle" {
		t.Errorf("expected status 'idle', got %q", status)
	}
}

func TestCheckWaitCondition_NotIdle(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_not_idle_test")

	agent := &models.Agent{
		ID:          "agent_working_test",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.0",
		State:       models.AgentStateWorking,
	}
	if err := agentRepo.Create(ctx, agent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	waitAgent = agent.ID

	met, status, err := checkWaitCondition(ctx, WaitConditionIdle, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if met {
		t.Errorf("expected condition NOT to be met for working agent")
	}
	if status != "state: working" {
		t.Errorf("expected status 'state: working', got %q", status)
	}
}

func TestCheckWaitCondition_QueueEmpty(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_queue_empty_test")

	agent := &models.Agent{
		ID:          "agent_queue_test",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.0",
		State:       models.AgentStateIdle,
	}
	if err := agentRepo.Create(ctx, agent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	waitAgent = agent.ID

	// Test with empty queue
	met, status, err := checkWaitCondition(ctx, WaitConditionQueueEmpty, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !met {
		t.Errorf("expected condition to be met for empty queue")
	}
	if status != "queue empty" {
		t.Errorf("expected status 'queue empty', got %q", status)
	}
}

func TestCheckWaitCondition_QueueNotEmpty(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_queue_not_empty_test")

	agent := &models.Agent{
		ID:          "agent_queue_pending_test",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.0",
		State:       models.AgentStateIdle,
	}
	if err := agentRepo.Create(ctx, agent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	// Add a pending queue item
	item := &models.QueueItem{
		ID:      "qi_test",
		AgentID: agent.ID,
		Type:    models.QueueItemTypeMessage,
		Status:  models.QueueItemStatusPending,
		Payload: []byte(`{"text": "test message"}`),
	}
	if err := queueRepo.Enqueue(ctx, agent.ID, item); err != nil {
		t.Fatalf("failed to enqueue item: %v", err)
	}

	waitAgent = agent.ID

	met, status, err := checkWaitCondition(ctx, WaitConditionQueueEmpty, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if met {
		t.Errorf("expected condition NOT to be met for non-empty queue")
	}
	if status != "queue: 1 pending" {
		t.Errorf("expected status 'queue: 1 pending', got %q", status)
	}
}

func TestCheckWaitCondition_CooldownOver(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_cooldown_over_test")

	// Create account without cooldown
	account := &models.Account{
		ID:          "acc_test",
		Provider:    "anthropic",
		ProfileName: "test",
	}
	if err := accountRepo.Create(ctx, account); err != nil {
		t.Fatalf("failed to create account: %v", err)
	}

	agent := &models.Agent{
		ID:          "agent_cooldown_test",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.0",
		State:       models.AgentStateIdle,
		AccountID:   account.ID,
	}
	if err := agentRepo.Create(ctx, agent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	waitAgent = agent.ID

	met, status, err := checkWaitCondition(ctx, WaitConditionCooldownOver, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !met {
		t.Errorf("expected condition to be met for account without cooldown")
	}
	if status != "no cooldown" {
		t.Errorf("expected status 'no cooldown', got %q", status)
	}
}

func TestCheckWaitCondition_CooldownActive(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_cooldown_active_test")

	// Create account with active cooldown
	cooldownTime := time.Now().Add(5 * time.Minute)
	account := &models.Account{
		ID:            "acc_cooldown",
		Provider:      "anthropic",
		ProfileName:   "test_cooldown",
		CooldownUntil: &cooldownTime,
	}
	if err := accountRepo.Create(ctx, account); err != nil {
		t.Fatalf("failed to create account: %v", err)
	}

	agent := &models.Agent{
		ID:          "agent_cooldown_active_test",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.0",
		State:       models.AgentStateIdle,
		AccountID:   account.ID,
	}
	if err := agentRepo.Create(ctx, agent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	waitAgent = agent.ID

	met, status, err := checkWaitCondition(ctx, WaitConditionCooldownOver, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if met {
		t.Errorf("expected condition NOT to be met for account with active cooldown")
	}
	// Status should contain "cooldown" and "remaining"
	if status == "" {
		t.Errorf("expected non-empty status for active cooldown")
	}
}

func TestCheckWaitCondition_AllIdle(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_all_idle_test")

	// Create multiple idle agents
	for i := 0; i < 3; i++ {
		agent := &models.Agent{
			ID:          "agent_all_idle_" + string(rune('a'+i)),
			WorkspaceID: ws.ID,
			Type:        models.AgentTypeOpenCode,
			TmuxPane:    fmt.Sprintf("test:0.%d", i),
			State:       models.AgentStateIdle,
		}
		if err := agentRepo.Create(ctx, agent); err != nil {
			t.Fatalf("failed to create agent: %v", err)
		}
	}

	waitWorkspace = ws.ID

	met, status, err := checkWaitCondition(ctx, WaitConditionAllIdle, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !met {
		t.Errorf("expected condition to be met when all agents are idle")
	}
	if status != "all idle" {
		t.Errorf("expected status 'all idle', got %q", status)
	}
}

func TestCheckWaitCondition_NotAllIdle(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_not_all_idle_test")

	// Create mixed state agents
	idleAgent := &models.Agent{
		ID:          "agent_idle_mixed",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.0",
		State:       models.AgentStateIdle,
	}
	if err := agentRepo.Create(ctx, idleAgent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	workingAgent := &models.Agent{
		ID:          "agent_working_mixed",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.1",
		State:       models.AgentStateWorking,
	}
	if err := agentRepo.Create(ctx, workingAgent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	waitWorkspace = ws.ID

	met, status, err := checkWaitCondition(ctx, WaitConditionAllIdle, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if met {
		t.Errorf("expected condition NOT to be met when not all agents are idle")
	}
	if status != "1/2 agents not idle" {
		t.Errorf("expected status '1/2 agents not idle', got %q", status)
	}
}

func TestCheckWaitCondition_AnyIdle(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_any_idle_test")

	// Create one idle agent and one working
	idleAgent := &models.Agent{
		ID:          "agent_idle_any",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.0",
		State:       models.AgentStateIdle,
	}
	if err := agentRepo.Create(ctx, idleAgent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	workingAgent := &models.Agent{
		ID:          "agent_working_any",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.1",
		State:       models.AgentStateWorking,
	}
	if err := agentRepo.Create(ctx, workingAgent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	waitWorkspace = ws.ID

	met, _, err := checkWaitCondition(ctx, WaitConditionAnyIdle, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !met {
		t.Errorf("expected condition to be met when at least one agent is idle")
	}
}

func TestCheckWaitCondition_Ready(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_ready_test")

	// Create account without cooldown
	account := &models.Account{
		ID:          "acc_ready",
		Provider:    "anthropic",
		ProfileName: "test_ready",
	}
	if err := accountRepo.Create(ctx, account); err != nil {
		t.Fatalf("failed to create account: %v", err)
	}

	agent := &models.Agent{
		ID:          "agent_ready_test",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.0",
		State:       models.AgentStateIdle,
		AccountID:   account.ID,
	}
	if err := agentRepo.Create(ctx, agent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	waitAgent = agent.ID

	// Agent is idle, queue is empty, no cooldown -> should be ready
	met, status, err := checkWaitCondition(ctx, WaitConditionReady, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !met {
		t.Errorf("expected condition to be met for ready agent")
	}
	if status != "ready" {
		t.Errorf("expected status 'ready', got %q", status)
	}
}

func TestCheckWaitCondition_NotReady_Working(t *testing.T) {
	database, cleanup := testutil.NewTestDB(t)
	defer cleanup()

	ctx := context.Background()
	agentRepo := db.NewAgentRepository(database)
	queueRepo := db.NewQueueRepository(database)
	accountRepo := db.NewAccountRepository(database)
	wsRepo := db.NewWorkspaceRepository(database)

	ws := createTestWorkspaceForWait(t, database, "ws_not_ready_test")

	agent := &models.Agent{
		ID:          "agent_not_ready_test",
		WorkspaceID: ws.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "test:0.0",
		State:       models.AgentStateWorking,
	}
	if err := agentRepo.Create(ctx, agent); err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	waitAgent = agent.ID

	met, status, err := checkWaitCondition(ctx, WaitConditionReady, agentRepo, queueRepo, accountRepo, wsRepo)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if met {
		t.Errorf("expected condition NOT to be met for working agent")
	}
	if status != "state: working" {
		t.Errorf("expected status 'state: working', got %q", status)
	}
}
