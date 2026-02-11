package models

import (
	"strings"
	"time"
)

// LoopWorkState is a task-tech-agnostic "what am I working on" pointer per loop.
// task_id is an opaque external id (sv-..., jira-..., markdown filename, etc).
type LoopWorkState struct {
	ID            string    `json:"id"`
	LoopID        string    `json:"loop_id"`
	AgentID       string    `json:"agent_id"`
	TaskID        string    `json:"task_id"`
	Status        string    `json:"status"`
	Detail        string    `json:"detail,omitempty"`
	LoopIteration int       `json:"loop_iteration"`
	IsCurrent     bool      `json:"is_current"`
	CreatedAt     time.Time `json:"created_at"`
	UpdatedAt     time.Time `json:"updated_at"`
}

func (s *LoopWorkState) Validate() error {
	validation := &ValidationErrors{}
	if strings.TrimSpace(s.LoopID) == "" {
		validation.AddMessage("loop_id", "loop_id is required")
	}
	if strings.TrimSpace(s.AgentID) == "" {
		validation.AddMessage("agent_id", "agent_id is required")
	}
	if strings.TrimSpace(s.TaskID) == "" {
		validation.AddMessage("task_id", "task_id is required")
	}
	if strings.TrimSpace(s.Status) == "" {
		validation.AddMessage("status", "status is required")
	}
	return validation.Err()
}
