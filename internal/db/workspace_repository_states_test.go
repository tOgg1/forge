package db

import (
	"context"
	"testing"

	"github.com/opencode-ai/swarm/internal/models"
)

func TestWorkspaceRepository_GetAgentStateCounts(t *testing.T) {
	db := setupTestDB(t)
	defer db.Close()

	ws := createTestWorkspace(t, db)
	agentRepo := NewAgentRepository(db)

	agents := []*models.Agent{
		{WorkspaceID: ws.ID, Type: models.AgentTypeOpenCode, TmuxPane: "session:0.1", State: models.AgentStateWorking},
		{WorkspaceID: ws.ID, Type: models.AgentTypeOpenCode, TmuxPane: "session:0.2", State: models.AgentStateIdle},
		{WorkspaceID: ws.ID, Type: models.AgentTypeOpenCode, TmuxPane: "session:0.3", State: models.AgentStateError},
		{WorkspaceID: ws.ID, Type: models.AgentTypeOpenCode, TmuxPane: "session:0.4", State: models.AgentStateWorking},
	}

	for _, agent := range agents {
		if err := agentRepo.Create(context.Background(), agent); err != nil {
			t.Fatalf("create agent: %v", err)
		}
	}

	repo := NewWorkspaceRepository(db)
	counts, err := repo.GetAgentStateCounts(context.Background(), ws.ID)
	if err != nil {
		t.Fatalf("GetAgentStateCounts failed: %v", err)
	}

	if counts[models.AgentStateWorking] != 2 {
		t.Fatalf("expected 2 working, got %d", counts[models.AgentStateWorking])
	}
	if counts[models.AgentStateIdle] != 1 {
		t.Fatalf("expected 1 idle, got %d", counts[models.AgentStateIdle])
	}
	if counts[models.AgentStateError] != 1 {
		t.Fatalf("expected 1 error, got %d", counts[models.AgentStateError])
	}
}
