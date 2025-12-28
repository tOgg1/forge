package db

import (
	"context"
	"encoding/json"
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/models"
)

func TestApprovalRepository_CreateListUpdate(t *testing.T) {
	db := setupTestDB(t)
	defer db.Close()

	repo := NewApprovalRepository(db)
	ctx := context.Background()

	nodeRepo := NewNodeRepository(db)
	wsRepo := NewWorkspaceRepository(db)
	agentRepo := NewAgentRepository(db)

	node := &models.Node{
		Name:       "local",
		Status:     models.NodeStatusOnline,
		IsLocal:    true,
		SSHBackend: models.SSHBackendAuto,
	}
	if err := nodeRepo.Create(ctx, node); err != nil {
		t.Fatalf("Create node failed: %v", err)
	}

	workspace := &models.Workspace{
		Name:        "ws",
		NodeID:      node.ID,
		RepoPath:    "/tmp/repo",
		TmuxSession: "ws-session",
		Status:      models.WorkspaceStatusActive,
	}
	if err := wsRepo.Create(ctx, workspace); err != nil {
		t.Fatalf("Create workspace failed: %v", err)
	}

	agent := &models.Agent{
		WorkspaceID: workspace.ID,
		Type:        models.AgentTypeOpenCode,
		TmuxPane:    "ws-session:0.0",
		State:       models.AgentStateIdle,
		StateInfo: models.StateInfo{
			State:      models.AgentStateIdle,
			Confidence: models.StateConfidenceHigh,
			Reason:     "ready",
			DetectedAt: time.Now().UTC(),
		},
	}
	if err := agentRepo.Create(ctx, agent); err != nil {
		t.Fatalf("Create agent failed: %v", err)
	}

	approval := &models.Approval{
		AgentID:        agent.ID,
		RequestType:    "file_write",
		RequestDetails: json.RawMessage(`{"path":"/tmp/test.txt"}`),
		Status:         models.ApprovalStatusPending,
	}

	if err := repo.Create(ctx, approval); err != nil {
		t.Fatalf("Create failed: %v", err)
	}

	pending, err := repo.ListPendingByAgent(ctx, agent.ID)
	if err != nil {
		t.Fatalf("ListPendingByAgent failed: %v", err)
	}
	if len(pending) != 1 {
		t.Fatalf("expected 1 pending approval, got %d", len(pending))
	}

	if err := repo.UpdateStatus(ctx, approval.ID, models.ApprovalStatusApproved, "user"); err != nil {
		t.Fatalf("UpdateStatus failed: %v", err)
	}

	pending, err = repo.ListPendingByAgent(ctx, agent.ID)
	if err != nil {
		t.Fatalf("ListPendingByAgent after update failed: %v", err)
	}
	if len(pending) != 0 {
		t.Fatalf("expected 0 pending approvals after update, got %d", len(pending))
	}
}
