// Package db provides SQLite database access for Forge.
package db

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"time"

	"github.com/google/uuid"
	"github.com/tOgg1/forge/internal/models"
)

// Approval repository errors.
var (
	ErrApprovalNotFound = errors.New("approval not found")
)

// ApprovalRepository handles approval persistence.
type ApprovalRepository struct {
	db *DB
}

// NewApprovalRepository creates a new ApprovalRepository.
func NewApprovalRepository(db *DB) *ApprovalRepository {
	return &ApprovalRepository{db: db}
}

// Create adds a new approval request to the database.
func (r *ApprovalRepository) Create(ctx context.Context, approval *models.Approval) error {
	if approval.AgentID == "" {
		return fmt.Errorf("approval agent id is required")
	}
	if approval.RequestType == "" {
		return fmt.Errorf("approval request type is required")
	}

	if approval.ID == "" {
		approval.ID = uuid.New().String()
	}

	now := time.Now().UTC()
	approval.CreatedAt = now
	if approval.Status == "" {
		approval.Status = models.ApprovalStatusPending
	}

	_, err := r.db.ExecContext(ctx, `
		INSERT INTO approvals (
			id, agent_id, request_type, request_details_json,
			status, created_at, resolved_at, resolved_by
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
	`,
		approval.ID,
		approval.AgentID,
		string(approval.RequestType),
		string(approval.RequestDetails),
		string(approval.Status),
		approval.CreatedAt.Format(time.RFC3339),
		stringTimePtr(approval.ResolvedAt),
		approval.ResolvedBy,
	)
	if err != nil {
		return fmt.Errorf("failed to insert approval: %w", err)
	}

	return nil
}

// ListPendingByAgent lists pending approvals for a single agent.
func (r *ApprovalRepository) ListPendingByAgent(ctx context.Context, agentID string) ([]*models.Approval, error) {
	rows, err := r.db.QueryContext(ctx, `
		SELECT 
			id, agent_id, request_type, request_details_json,
			status, created_at, resolved_at, resolved_by
		FROM approvals
		WHERE agent_id = ? AND status = 'pending'
		ORDER BY created_at
	`, agentID)
	if err != nil {
		return nil, fmt.Errorf("failed to query approvals: %w", err)
	}
	defer rows.Close()

	return r.scanApprovals(rows)
}

// UpdateStatus updates the status of an approval.
func (r *ApprovalRepository) UpdateStatus(ctx context.Context, id string, status models.ApprovalStatus, resolvedBy string) error {
	if id == "" {
		return fmt.Errorf("approval id is required")
	}
	if status == "" {
		return fmt.Errorf("approval status is required")
	}

	now := time.Now().UTC().Format(time.RFC3339)

	result, err := r.db.ExecContext(ctx, `
		UPDATE approvals
		SET status = ?, resolved_at = ?, resolved_by = ?
		WHERE id = ?
	`, string(status), now, resolvedBy, id)
	if err != nil {
		return fmt.Errorf("failed to update approval: %w", err)
	}

	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("failed to get rows affected: %w", err)
	}
	if rowsAffected == 0 {
		return ErrApprovalNotFound
	}

	return nil
}

func (r *ApprovalRepository) scanApprovals(rows *sql.Rows) ([]*models.Approval, error) {
	var approvals []*models.Approval
	for rows.Next() {
		approval, err := r.scanApprovalFromRows(rows)
		if err != nil {
			return nil, err
		}
		approvals = append(approvals, approval)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating approvals: %w", err)
	}
	return approvals, nil
}

func (r *ApprovalRepository) scanApprovalFromRows(rows *sql.Rows) (*models.Approval, error) {
	var approval models.Approval
	var requestType string
	var requestDetails sql.NullString
	var status string
	var createdAt string
	var resolvedAt sql.NullString
	var resolvedBy sql.NullString

	if err := rows.Scan(
		&approval.ID,
		&approval.AgentID,
		&requestType,
		&requestDetails,
		&status,
		&createdAt,
		&resolvedAt,
		&resolvedBy,
	); err != nil {
		return nil, fmt.Errorf("failed to scan approval: %w", err)
	}

	approval.RequestType = models.ApprovalRequestType(requestType)
	approval.Status = models.ApprovalStatus(status)

	if requestDetails.Valid && requestDetails.String != "" {
		approval.RequestDetails = json.RawMessage(requestDetails.String)
	}

	createdParsed, err := time.Parse(time.RFC3339, createdAt)
	if err != nil {
		return nil, fmt.Errorf("failed to parse created_at: %w", err)
	}
	approval.CreatedAt = createdParsed

	if resolvedAt.Valid && resolvedAt.String != "" {
		parsed, err := time.Parse(time.RFC3339, resolvedAt.String)
		if err != nil {
			return nil, fmt.Errorf("failed to parse resolved_at: %w", err)
		}
		approval.ResolvedAt = &parsed
	}

	if resolvedBy.Valid {
		approval.ResolvedBy = resolvedBy.String
	}

	return &approval, nil
}
