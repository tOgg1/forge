package models

import (
	"encoding/json"
	"time"
)

// QueueItemType specifies the type of queue item.
type QueueItemType string

const (
	QueueItemTypeMessage     QueueItemType = "message"
	QueueItemTypePause       QueueItemType = "pause"
	QueueItemTypeConditional QueueItemType = "conditional"
)

// QueueItemStatus represents the status of a queue item.
type QueueItemStatus string

const (
	QueueItemStatusPending    QueueItemStatus = "pending"
	QueueItemStatusDispatched QueueItemStatus = "dispatched"
	QueueItemStatusCompleted  QueueItemStatus = "completed"
	QueueItemStatusFailed     QueueItemStatus = "failed"
	QueueItemStatusSkipped    QueueItemStatus = "skipped"
)

// QueueItem represents an item in an agent's message queue.
type QueueItem struct {
	// ID is the unique identifier for the queue item.
	ID string `json:"id"`

	// AgentID references the agent this item belongs to.
	AgentID string `json:"agent_id"`

	// Type specifies the item type.
	Type QueueItemType `json:"type"`

	// Position is the order in the queue (lower = earlier).
	Position int `json:"position"`

	// Status is the current item status.
	Status QueueItemStatus `json:"status"`

	// Payload contains the item data (type-specific).
	Payload json.RawMessage `json:"payload"`

	// CreatedAt is when the item was queued.
	CreatedAt time.Time `json:"created_at"`

	// DispatchedAt is when the item was sent (if dispatched).
	DispatchedAt *time.Time `json:"dispatched_at,omitempty"`

	// CompletedAt is when the item completed (if finished).
	CompletedAt *time.Time `json:"completed_at,omitempty"`

	// Error contains error details (if failed).
	Error string `json:"error,omitempty"`
}

// MessagePayload is the payload for message queue items.
type MessagePayload struct {
	// Text is the message content to send.
	Text string `json:"text"`
}

// PausePayload is the payload for pause queue items.
type PausePayload struct {
	// Duration is how long to pause in seconds.
	DurationSeconds int `json:"duration_seconds"`

	// Reason explains why the pause was inserted.
	Reason string `json:"reason,omitempty"`
}

// ConditionType specifies the type of condition gate.
type ConditionType string

const (
	ConditionTypeWhenIdle         ConditionType = "when_idle"
	ConditionTypeAfterCooldown    ConditionType = "after_cooldown"
	ConditionTypeAfterPrevious    ConditionType = "after_previous"
	ConditionTypeCustomExpression ConditionType = "custom"
)

// ConditionalPayload is the payload for conditional queue items.
type ConditionalPayload struct {
	// ConditionType specifies the gate type.
	ConditionType ConditionType `json:"condition_type"`

	// Expression is a custom condition expression (for custom type).
	Expression string `json:"expression,omitempty"`

	// Message is the message to send when condition is satisfied.
	Message string `json:"message"`
}

// Validate checks if the queue item is valid.
func (q *QueueItem) Validate() error {
	if len(q.Payload) == 0 {
		return ErrInvalidQueueItem
	}
	return nil
}

// GetMessagePayload extracts the message payload.
func (q *QueueItem) GetMessagePayload() (*MessagePayload, error) {
	if q.Type != QueueItemTypeMessage {
		return nil, ErrInvalidQueueItem
	}
	var payload MessagePayload
	if err := json.Unmarshal(q.Payload, &payload); err != nil {
		return nil, err
	}
	return &payload, nil
}

// GetPausePayload extracts the pause payload.
func (q *QueueItem) GetPausePayload() (*PausePayload, error) {
	if q.Type != QueueItemTypePause {
		return nil, ErrInvalidQueueItem
	}
	var payload PausePayload
	if err := json.Unmarshal(q.Payload, &payload); err != nil {
		return nil, err
	}
	return &payload, nil
}

// GetConditionalPayload extracts the conditional payload.
func (q *QueueItem) GetConditionalPayload() (*ConditionalPayload, error) {
	if q.Type != QueueItemTypeConditional {
		return nil, ErrInvalidQueueItem
	}
	var payload ConditionalPayload
	if err := json.Unmarshal(q.Payload, &payload); err != nil {
		return nil, err
	}
	return &payload, nil
}
