package testutil

import (
	"context"
	"testing"
	"time"

	"github.com/opencode-ai/swarm/internal/models"
)

// TestTmuxTestEnv_Basic tests the basic tmux test environment.
func TestTmuxTestEnv_Basic(t *testing.T) {
	env := NewTmuxTestEnv(t)
	defer env.Close()

	// Verify session was created
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	hasSession, err := env.Client.HasSession(ctx, env.Session)
	if err != nil {
		t.Fatalf("failed to check session: %v", err)
	}
	if !hasSession {
		t.Fatal("expected session to exist")
	}
}

// TestTmuxTestEnv_SendAndCapture tests sending keys and capturing output.
func TestTmuxTestEnv_SendAndCapture(t *testing.T) {
	env := NewTmuxTestEnv(t)
	defer env.Close()

	// Send echo command
	env.SendKeys("echo 'hello world'", true)

	// Wait for output
	found := env.WaitForContent("hello world", 5*time.Second)
	if !found {
		content := env.Capture()
		t.Fatalf("expected 'hello world' in output, got:\n%s", content)
	}
}

// TestTmuxTestEnv_WaitForStable tests waiting for stable output.
func TestTmuxTestEnv_WaitForStable(t *testing.T) {
	env := NewTmuxTestEnv(t)
	defer env.Close()

	// Send a command that produces output
	env.SendKeys("echo 'stable output'", true)

	// Wait for stable content
	content := env.WaitForStable(5*time.Second, 200*time.Millisecond)
	if content == "" {
		t.Fatal("expected non-empty stable content")
	}
}

// TestTmuxTestEnv_SplitPane tests pane splitting.
func TestTmuxTestEnv_SplitPane(t *testing.T) {
	env := NewTmuxTestEnv(t)
	defer env.Close()

	// Initially should have 1 pane
	panes := env.ListPanes()
	if len(panes) != 1 {
		t.Fatalf("expected 1 pane initially, got %d", len(panes))
	}

	// Split horizontally
	_ = env.SplitPane(true)

	// Should now have 2 panes
	panes = env.ListPanes()
	if len(panes) != 2 {
		t.Fatalf("expected 2 panes after split, got %d", len(panes))
	}
}

// TestDBEnv_Basic tests the database test environment.
func TestDBEnv_Basic(t *testing.T) {
	env := NewTestDBEnv(t)
	defer env.Close()

	ctx := context.Background()

	// First create a node (required for workspace)
	node := &models.Node{
		Name:       "test-node",
		IsLocal:    true,
		SSHBackend: models.SSHBackendAuto,
		Status:     models.NodeStatusOnline,
	}
	err := env.NodeRepo.Create(ctx, node)
	if err != nil {
		t.Fatalf("failed to create node: %v", err)
	}

	// Create a test workspace
	workspace := &models.Workspace{
		NodeID:      node.ID,
		RepoPath:    "/test/workspace",
		Name:        "test-workspace",
		TmuxSession: "test-session",
	}
	err = env.WorkspaceRepo.Create(ctx, workspace)
	if err != nil {
		t.Fatalf("failed to create workspace: %v", err)
	}

	// Verify it was created
	ws, err := env.WorkspaceRepo.Get(ctx, workspace.ID)
	if err != nil {
		t.Fatalf("failed to get workspace: %v", err)
	}
	if ws.Name != "test-workspace" {
		t.Errorf("expected workspace name 'test-workspace', got %q", ws.Name)
	}
}

// TestDBEnv_Agents tests agent creation and retrieval.
func TestDBEnv_Agents(t *testing.T) {
	env := NewTestDBEnv(t)
	defer env.Close()

	ctx := context.Background()

	// Create a node first
	node := &models.Node{
		Name:       "test-node",
		IsLocal:    true,
		SSHBackend: models.SSHBackendAuto,
		Status:     models.NodeStatusOnline,
	}
	err := env.NodeRepo.Create(ctx, node)
	if err != nil {
		t.Fatalf("failed to create node: %v", err)
	}

	// Create a workspace (agent requires workspace)
	workspace := &models.Workspace{
		NodeID:      node.ID,
		RepoPath:    "/test/agent-workspace",
		Name:        "agent-workspace",
		TmuxSession: "agent-session",
	}
	err = env.WorkspaceRepo.Create(ctx, workspace)
	if err != nil {
		t.Fatalf("failed to create workspace: %v", err)
	}

	// Create an agent
	agent := &models.Agent{
		WorkspaceID: workspace.ID,
		TmuxPane:    "%0",
		Type:        models.AgentTypeClaudeCode,
		State:       models.AgentStateIdle,
	}
	err = env.AgentRepo.Create(ctx, agent)
	if err != nil {
		t.Fatalf("failed to create agent: %v", err)
	}

	// Verify it was created
	found, err := env.AgentRepo.Get(ctx, agent.ID)
	if err != nil {
		t.Fatalf("failed to get agent: %v", err)
	}
	if found.Type != models.AgentTypeClaudeCode {
		t.Errorf("expected agent type claude-code, got %v", found.Type)
	}
	if found.State != models.AgentStateIdle {
		t.Errorf("expected agent state idle, got %v", found.State)
	}
}

// TestFixturePath tests the fixture path helper.
func TestFixturePath(t *testing.T) {
	path := FixturePath(t, "transcripts", "claude_code_idle.txt")
	if path == "" {
		t.Fatal("expected non-empty path")
	}
}

// TestReadFixture tests the fixture reader.
func TestReadFixture(t *testing.T) {
	data := ReadFixture(t, "transcripts", "claude_code_idle.txt")
	if len(data) == 0 {
		t.Fatal("expected non-empty fixture data")
	}
}
