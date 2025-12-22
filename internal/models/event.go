package models

import (
	"encoding/json"
	"time"
)

// EventType categorizes events in the system.
type EventType string

const (
	// Node events
	EventTypeNodeOnline  EventType = "node.online"
	EventTypeNodeOffline EventType = "node.offline"
	EventTypeNodeAdded   EventType = "node.added"
	EventTypeNodeRemoved EventType = "node.removed"

	// Workspace events
	EventTypeWorkspaceCreated   EventType = "workspace.created"
	EventTypeWorkspaceImported  EventType = "workspace.imported"
	EventTypeWorkspaceDestroyed EventType = "workspace.destroyed"
	EventTypeWorkspaceUnmanaged EventType = "workspace.unmanaged"

	// Agent events
	EventTypeAgentSpawned      EventType = "agent.spawned"
	EventTypeAgentStateChanged EventType = "agent.state_changed"
	EventTypeAgentRestarted    EventType = "agent.restarted"
	EventTypeAgentTerminated   EventType = "agent.terminated"
	EventTypeAgentPaused       EventType = "agent.paused"
	EventTypeAgentResumed      EventType = "agent.resumed"

	// Message events
	EventTypeMessageQueued     EventType = "message.queued"
	EventTypeMessageDispatched EventType = "message.dispatched"
	EventTypeMessageCompleted  EventType = "message.completed"
	EventTypeMessageFailed     EventType = "message.failed"

	// Approval events
	EventTypeApprovalRequested EventType = "approval.requested"
	EventTypeApprovalApproved  EventType = "approval.approved"
	EventTypeApprovalDenied    EventType = "approval.denied"

	// Account events
	EventTypeRateLimitDetected EventType = "rate_limit.detected"
	EventTypeCooldownStarted   EventType = "cooldown.started"
	EventTypeCooldownEnded     EventType = "cooldown.ended"
	EventTypeAccountRotated    EventType = "account.rotated"

	// System events
	EventTypeError   EventType = "error"
	EventTypeWarning EventType = "warning"
)

// EntityType identifies the type of entity an event relates to.
type EntityType string

const (
	EntityTypeNode      EntityType = "node"
	EntityTypeWorkspace EntityType = "workspace"
	EntityTypeAgent     EntityType = "agent"
	EntityTypeQueue     EntityType = "queue"
	EntityTypeAccount   EntityType = "account"
	EntityTypeSystem    EntityType = "system"
)

// Event represents an append-only log entry.
type Event struct {
	// ID is the unique identifier for the event.
	ID string `json:"id"`

	// Timestamp is when the event occurred.
	Timestamp time.Time `json:"timestamp"`

	// Type categorizes the event.
	Type EventType `json:"type"`

	// EntityType identifies what kind of entity this event relates to.
	EntityType EntityType `json:"entity_type"`

	// EntityID is the ID of the related entity.
	EntityID string `json:"entity_id"`

	// Payload contains event-specific data.
	Payload json.RawMessage `json:"payload,omitempty"`

	// Metadata contains additional context.
	Metadata map[string]string `json:"metadata,omitempty"`
}

// StateChangedPayload is the payload for agent.state_changed events.
type StateChangedPayload struct {
	OldState   AgentState      `json:"old_state"`
	NewState   AgentState      `json:"new_state"`
	Confidence StateConfidence `json:"confidence"`
	Reason     string          `json:"reason"`
}

// MessageDispatchedPayload is the payload for message.dispatched events.
type MessageDispatchedPayload struct {
	QueueItemID string `json:"queue_item_id"`
	Message     string `json:"message"`
}

// RateLimitPayload is the payload for rate_limit.detected events.
type RateLimitPayload struct {
	AccountID       string   `json:"account_id"`
	Provider        Provider `json:"provider"`
	CooldownSeconds int      `json:"cooldown_seconds"`
	Reason          string   `json:"reason,omitempty"`
}

// AccountRotatedPayload is the payload for account.rotated events.
type AccountRotatedPayload struct {
	AgentID      string `json:"agent_id"`
	OldAccountID string `json:"old_account_id"`
	NewAccountID string `json:"new_account_id"`
	Reason       string `json:"reason"`
}

// ErrorPayload is the payload for error events.
type ErrorPayload struct {
	Error      string `json:"error"`
	StackTrace string `json:"stack_trace,omitempty"`
	Context    string `json:"context,omitempty"`
}
