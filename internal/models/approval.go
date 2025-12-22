package models

import (
	"encoding/json"
	"time"
)

// ApprovalStatus represents the lifecycle state of an approval.
type ApprovalStatus string

const (
	ApprovalStatusPending  ApprovalStatus = "pending"
	ApprovalStatusApproved ApprovalStatus = "approved"
	ApprovalStatusDenied   ApprovalStatus = "denied"
	ApprovalStatusExpired  ApprovalStatus = "expired"
)

// ApprovalRequestType describes the kind of approval requested.
type ApprovalRequestType string

// Approval captures an approval request from an agent.
type Approval struct {
	// ID is the unique identifier for the approval.
	ID string `json:"id"`

	// AgentID references the agent that requested approval.
	AgentID string `json:"agent_id"`

	// RequestType categorizes the approval request.
	RequestType ApprovalRequestType `json:"request_type"`

	// RequestDetails contains request-specific data.
	RequestDetails json.RawMessage `json:"request_details"`

	// Status is the current approval status.
	Status ApprovalStatus `json:"status"`

	// CreatedAt is when the approval was created.
	CreatedAt time.Time `json:"created_at"`

	// ResolvedAt is when the approval was resolved (approved/denied/expired).
	ResolvedAt *time.Time `json:"resolved_at,omitempty"`

	// ResolvedBy indicates who or what resolved the approval (user/policy).
	ResolvedBy string `json:"resolved_by,omitempty"`
}
