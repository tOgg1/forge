// Package config provides configuration and context management for Swarm.
package config

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"gopkg.in/yaml.v3"
)

// Context represents the current CLI context (selected workspace/agent).
type Context struct {
	// WorkspaceID is the currently selected workspace.
	WorkspaceID string `yaml:"workspace,omitempty"`
	// WorkspaceName is the human-readable workspace name (for display).
	WorkspaceName string `yaml:"workspace_name,omitempty"`
	// AgentID is the currently selected agent.
	AgentID string `yaml:"agent,omitempty"`
	// AgentName is the human-readable agent identifier (for display).
	AgentName string `yaml:"agent_name,omitempty"`
	// UpdatedAt is when the context was last modified.
	UpdatedAt time.Time `yaml:"updated_at,omitempty"`
}

// IsEmpty returns true if no context is set.
func (c *Context) IsEmpty() bool {
	return c.WorkspaceID == "" && c.AgentID == ""
}

// HasWorkspace returns true if a workspace is set.
func (c *Context) HasWorkspace() bool {
	return c.WorkspaceID != ""
}

// HasAgent returns true if an agent is set.
func (c *Context) HasAgent() bool {
	return c.AgentID != ""
}

// Clear removes all context.
func (c *Context) Clear() {
	c.WorkspaceID = ""
	c.WorkspaceName = ""
	c.AgentID = ""
	c.AgentName = ""
	c.UpdatedAt = time.Now()
}

// SetWorkspace sets the workspace context.
func (c *Context) SetWorkspace(id, name string) {
	c.WorkspaceID = id
	c.WorkspaceName = name
	// Clear agent if workspace changes (agent belongs to workspace)
	c.AgentID = ""
	c.AgentName = ""
	c.UpdatedAt = time.Now()
}

// SetAgent sets the agent context.
func (c *Context) SetAgent(id, name string) {
	c.AgentID = id
	c.AgentName = name
	c.UpdatedAt = time.Now()
}

// String returns a human-readable representation of the context.
func (c *Context) String() string {
	if c.IsEmpty() {
		return "(no context set)"
	}
	var parts []string
	if c.HasWorkspace() {
		name := c.WorkspaceName
		if name == "" {
			name = shortID(c.WorkspaceID)
		}
		parts = append(parts, fmt.Sprintf("workspace:%s", name))
	}
	if c.HasAgent() {
		name := c.AgentName
		if name == "" {
			name = shortID(c.AgentID)
		}
		parts = append(parts, fmt.Sprintf("agent:%s", name))
	}
	if len(parts) == 0 {
		return "(no context set)"
	}
	result := parts[0]
	for i := 1; i < len(parts); i++ {
		result += " " + parts[i]
	}
	return result
}

func shortID(id string) string {
	if len(id) > 8 {
		return id[:8]
	}
	return id
}

// ContextStore manages loading and saving context.
type ContextStore struct {
	path string
	mu   sync.RWMutex
}

// NewContextStore creates a new context store.
// If path is empty, uses the default path (~/.config/swarm/context.yaml).
func NewContextStore(path string) *ContextStore {
	if path == "" {
		homeDir, _ := os.UserHomeDir()
		path = filepath.Join(homeDir, ".config", "swarm", "context.yaml")
	}
	return &ContextStore{path: path}
}

// DefaultContextStore returns a context store using the default path.
func DefaultContextStore() *ContextStore {
	return NewContextStore("")
}

// Path returns the context file path.
func (s *ContextStore) Path() string {
	return s.path
}

// Load reads the context from disk.
// Returns an empty context if the file doesn't exist.
func (s *ContextStore) Load() (*Context, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	ctx := &Context{}

	data, err := os.ReadFile(s.path)
	if err != nil {
		if os.IsNotExist(err) {
			return ctx, nil
		}
		return nil, fmt.Errorf("failed to read context file: %w", err)
	}

	if err := yaml.Unmarshal(data, ctx); err != nil {
		return nil, fmt.Errorf("failed to parse context file: %w", err)
	}

	return ctx, nil
}

// Save writes the context to disk.
func (s *ContextStore) Save(ctx *Context) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Ensure directory exists
	dir := filepath.Dir(s.path)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create context directory: %w", err)
	}

	data, err := yaml.Marshal(ctx)
	if err != nil {
		return fmt.Errorf("failed to serialize context: %w", err)
	}

	if err := os.WriteFile(s.path, data, 0644); err != nil {
		return fmt.Errorf("failed to write context file: %w", err)
	}

	return nil
}

// Clear removes the context file.
func (s *ContextStore) Clear() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if err := os.Remove(s.path); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("failed to remove context file: %w", err)
	}
	return nil
}
