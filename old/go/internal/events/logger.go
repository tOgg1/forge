// Package events provides helper functions for logging Forge events.
package events

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/tOgg1/forge/internal/models"
)

// Repository is the minimal interface needed to write events.
type Repository interface {
	Create(ctx context.Context, event *models.Event) error
}

// LogStateChanged records a state change event for an agent.
func LogStateChanged(ctx context.Context, repo Repository, agentID string, oldState, newState models.AgentState, confidence models.StateConfidence, reason string) error {
	if repo == nil {
		return fmt.Errorf("event repository is required")
	}
	if agentID == "" {
		return fmt.Errorf("agent id is required")
	}

	payload, err := json.Marshal(models.StateChangedPayload{
		OldState:   oldState,
		NewState:   newState,
		Confidence: confidence,
		Reason:     reason,
	})
	if err != nil {
		return fmt.Errorf("failed to marshal state change payload: %w", err)
	}

	event := &models.Event{
		Type:       models.EventTypeAgentStateChanged,
		EntityType: models.EntityTypeAgent,
		EntityID:   agentID,
		Payload:    payload,
	}

	return repo.Create(ctx, event)
}

// LogMessageDispatched records a message dispatch event for an agent.
func LogMessageDispatched(ctx context.Context, repo Repository, agentID, queueItemID, message string) error {
	if repo == nil {
		return fmt.Errorf("event repository is required")
	}
	if agentID == "" {
		return fmt.Errorf("agent id is required")
	}

	payload, err := json.Marshal(models.MessageDispatchedPayload{
		QueueItemID: queueItemID,
		Message:     message,
	})
	if err != nil {
		return fmt.Errorf("failed to marshal dispatch payload: %w", err)
	}

	event := &models.Event{
		Type:       models.EventTypeMessageDispatched,
		EntityType: models.EntityTypeAgent,
		EntityID:   agentID,
		Payload:    payload,
	}

	return repo.Create(ctx, event)
}

// LogAgentStateChanged records a state change event for an agent.
func LogAgentStateChanged(
	ctx context.Context,
	repo Repository,
	agentID string,
	oldState models.AgentState,
	newState models.AgentState,
	confidence models.StateConfidence,
	reason string,
) error {
	if repo == nil {
		return fmt.Errorf("event repository is required")
	}
	if agentID == "" {
		return fmt.Errorf("agent id is required")
	}

	payload, err := json.Marshal(models.StateChangedPayload{
		OldState:   oldState,
		NewState:   newState,
		Confidence: confidence,
		Reason:     reason,
	})
	if err != nil {
		return fmt.Errorf("failed to marshal state change payload: %w", err)
	}

	event := &models.Event{
		Type:       models.EventTypeAgentStateChanged,
		EntityType: models.EntityTypeAgent,
		EntityID:   agentID,
		Payload:    payload,
	}

	return repo.Create(ctx, event)
}
