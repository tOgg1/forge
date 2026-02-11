// Package config provides context persistence tests.
package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestContext_IsEmpty(t *testing.T) {
	tests := []struct {
		name string
		ctx  Context
		want bool
	}{
		{
			name: "empty context",
			ctx:  Context{},
			want: true,
		},
		{
			name: "with workspace only",
			ctx:  Context{WorkspaceID: "ws_123"},
			want: false,
		},
		{
			name: "with agent only",
			ctx:  Context{AgentID: "agent_123"},
			want: false,
		},
		{
			name: "with both",
			ctx:  Context{WorkspaceID: "ws_123", AgentID: "agent_123"},
			want: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.ctx.IsEmpty(); got != tt.want {
				t.Errorf("Context.IsEmpty() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestContext_HasWorkspace(t *testing.T) {
	tests := []struct {
		name string
		ctx  Context
		want bool
	}{
		{
			name: "empty",
			ctx:  Context{},
			want: false,
		},
		{
			name: "with workspace",
			ctx:  Context{WorkspaceID: "ws_123"},
			want: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.ctx.HasWorkspace(); got != tt.want {
				t.Errorf("Context.HasWorkspace() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestContext_HasAgent(t *testing.T) {
	tests := []struct {
		name string
		ctx  Context
		want bool
	}{
		{
			name: "empty",
			ctx:  Context{},
			want: false,
		},
		{
			name: "with agent",
			ctx:  Context{AgentID: "agent_123"},
			want: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.ctx.HasAgent(); got != tt.want {
				t.Errorf("Context.HasAgent() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestContext_String(t *testing.T) {
	tests := []struct {
		name string
		ctx  Context
		want string
	}{
		{
			name: "empty",
			ctx:  Context{},
			want: "(none)",
		},
		{
			name: "workspace only with name",
			ctx:  Context{WorkspaceID: "ws_123", WorkspaceName: "my-project"},
			want: "my-project",
		},
		{
			name: "workspace only without name",
			ctx:  Context{WorkspaceID: "ws_123"},
			want: "ws_123",
		},
		{
			name: "agent only with name",
			ctx:  Context{AgentID: "agent_123", AgentName: "agent_12"},
			want: ":agent_12",
		},
		{
			name: "both with names",
			ctx:  Context{WorkspaceID: "ws_123", WorkspaceName: "my-project", AgentID: "agent_123", AgentName: "agent_12"},
			want: "my-project:agent_12",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.ctx.String(); got != tt.want {
				t.Errorf("Context.String() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestContext_SetWorkspace(t *testing.T) {
	ctx := &Context{}
	ctx.SetWorkspace("ws_123", "my-project")

	if ctx.WorkspaceID != "ws_123" {
		t.Errorf("WorkspaceID = %v, want ws_123", ctx.WorkspaceID)
	}
	if ctx.WorkspaceName != "my-project" {
		t.Errorf("WorkspaceName = %v, want my-project", ctx.WorkspaceName)
	}
}

func TestContext_SetAgent(t *testing.T) {
	ctx := &Context{}
	ctx.SetAgent("agent_123", "agent_12")

	if ctx.AgentID != "agent_123" {
		t.Errorf("AgentID = %v, want agent_123", ctx.AgentID)
	}
	if ctx.AgentName != "agent_12" {
		t.Errorf("AgentName = %v, want agent_12", ctx.AgentName)
	}
}

func TestContext_ClearAgent(t *testing.T) {
	ctx := &Context{
		WorkspaceID:   "ws_123",
		WorkspaceName: "my-project",
		AgentID:       "agent_123",
		AgentName:     "agent_12",
	}

	ctx.ClearAgent()

	if ctx.AgentID != "" {
		t.Errorf("AgentID = %v, want empty", ctx.AgentID)
	}
	if ctx.AgentName != "" {
		t.Errorf("AgentName = %v, want empty", ctx.AgentName)
	}
	// Workspace should be preserved
	if ctx.WorkspaceID != "ws_123" {
		t.Errorf("WorkspaceID = %v, want ws_123", ctx.WorkspaceID)
	}
}

func TestContextStore_SaveLoad(t *testing.T) {
	tmpDir := t.TempDir()
	store := NewContextStore(filepath.Join(tmpDir, "context.yaml"))

	ctx := &Context{
		WorkspaceID:   "ws_abc123",
		WorkspaceName: "test-project",
		AgentID:       "agent_xyz789",
		AgentName:     "agent_xy",
	}

	// Save
	if err := store.Save(ctx); err != nil {
		t.Fatalf("Save() error = %v", err)
	}

	// Load
	loaded, err := store.Load()
	if err != nil {
		t.Fatalf("Load() error = %v", err)
	}

	if loaded.WorkspaceID != ctx.WorkspaceID {
		t.Errorf("WorkspaceID = %v, want %v", loaded.WorkspaceID, ctx.WorkspaceID)
	}
	if loaded.WorkspaceName != ctx.WorkspaceName {
		t.Errorf("WorkspaceName = %v, want %v", loaded.WorkspaceName, ctx.WorkspaceName)
	}
	if loaded.AgentID != ctx.AgentID {
		t.Errorf("AgentID = %v, want %v", loaded.AgentID, ctx.AgentID)
	}
	if loaded.AgentName != ctx.AgentName {
		t.Errorf("AgentName = %v, want %v", loaded.AgentName, ctx.AgentName)
	}
}

func TestContextStore_LoadEmpty(t *testing.T) {
	tmpDir := t.TempDir()
	store := NewContextStore(filepath.Join(tmpDir, "context.yaml"))

	// Load non-existent file should return empty context
	loaded, err := store.Load()
	if err != nil {
		t.Fatalf("Load() error = %v", err)
	}

	if !loaded.IsEmpty() {
		t.Error("Load() should return empty context for non-existent file")
	}
}

func TestContextStore_Clear(t *testing.T) {
	tmpDir := t.TempDir()
	contextPath := filepath.Join(tmpDir, "context.yaml")
	store := NewContextStore(contextPath)

	ctx := &Context{
		WorkspaceID:   "ws_abc123",
		WorkspaceName: "test-project",
	}

	// Save first
	if err := store.Save(ctx); err != nil {
		t.Fatalf("Save() error = %v", err)
	}

	// Verify file exists
	if _, err := os.Stat(contextPath); os.IsNotExist(err) {
		t.Fatal("context file should exist after save")
	}

	// Clear
	if err := store.Clear(); err != nil {
		t.Fatalf("Clear() error = %v", err)
	}

	// Verify file is removed
	if _, err := os.Stat(contextPath); !os.IsNotExist(err) {
		t.Error("context file should be removed after clear")
	}

	// Load after clear should return empty
	loaded, err := store.Load()
	if err != nil {
		t.Fatalf("Load() after Clear() error = %v", err)
	}
	if !loaded.IsEmpty() {
		t.Error("Load() after Clear() should return empty context")
	}
}
