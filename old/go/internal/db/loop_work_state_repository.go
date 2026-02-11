package db

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/tOgg1/forge/internal/models"
)

var (
	ErrLoopWorkStateNotFound = errors.New("loop work state not found")
)

type LoopWorkStateRepository struct {
	db *DB
}

func NewLoopWorkStateRepository(db *DB) *LoopWorkStateRepository {
	return &LoopWorkStateRepository{db: db}
}

// SetCurrent upserts (loop_id, task_id) and marks it as current.
// Clears current marker from other tasks for the loop.
func (r *LoopWorkStateRepository) SetCurrent(ctx context.Context, state *models.LoopWorkState) error {
	if state == nil {
		return fmt.Errorf("state is required")
	}
	state.LoopID = strings.TrimSpace(state.LoopID)
	state.AgentID = strings.TrimSpace(state.AgentID)
	state.TaskID = strings.TrimSpace(state.TaskID)
	state.Status = strings.TrimSpace(state.Status)
	if state.Status == "" {
		state.Status = "in_progress"
	}
	if err := state.Validate(); err != nil {
		return fmt.Errorf("invalid loop work state: %w", err)
	}

	now := time.Now().UTC()

	return r.db.Transaction(ctx, func(tx *sql.Tx) error {
		// Clear current from other tasks in same loop.
		if _, err := tx.ExecContext(ctx, `
			UPDATE loop_work_state
			SET is_current = 0
			WHERE loop_id = ? AND is_current = 1
		`, state.LoopID); err != nil {
			return fmt.Errorf("failed to clear current loop work state: %w", err)
		}

		// Try update first (common path).
		result, err := tx.ExecContext(ctx, `
			UPDATE loop_work_state
			SET agent_id = ?, status = ?, detail = ?, loop_iteration = ?, is_current = 1
			WHERE loop_id = ? AND task_id = ?
		`,
			state.AgentID,
			state.Status,
			nullableString(state.Detail),
			state.LoopIteration,
			state.LoopID,
			state.TaskID,
		)
		if err != nil {
			return fmt.Errorf("failed to update loop work state: %w", err)
		}
		rows, _ := result.RowsAffected()
		if rows > 0 {
			state.IsCurrent = true
			state.UpdatedAt = now
			return nil
		}

		// Insert new.
		if state.ID == "" {
			state.ID = uuid.New().String()
		}
		state.CreatedAt = now
		state.UpdatedAt = now
		state.IsCurrent = true

		_, err = tx.ExecContext(ctx, `
			INSERT INTO loop_work_state (
				id, loop_id, agent_id, task_id, status, detail, loop_iteration, is_current,
				created_at, updated_at
			) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
		`,
			state.ID,
			state.LoopID,
			state.AgentID,
			state.TaskID,
			state.Status,
			nullableString(state.Detail),
			state.LoopIteration,
			1,
			state.CreatedAt.Format(time.RFC3339),
			state.UpdatedAt.Format(time.RFC3339),
		)
		if err != nil {
			// Race: inserted elsewhere after our update attempt; fall back to update.
			if isUniqueConstraintError(err) {
				_, err2 := tx.ExecContext(ctx, `
					UPDATE loop_work_state
					SET agent_id = ?, status = ?, detail = ?, loop_iteration = ?, is_current = 1
					WHERE loop_id = ? AND task_id = ?
				`,
					state.AgentID,
					state.Status,
					nullableString(state.Detail),
					state.LoopIteration,
					state.LoopID,
					state.TaskID,
				)
				if err2 == nil {
					return nil
				}
			}
			return fmt.Errorf("failed to insert loop work state: %w", err)
		}

		return nil
	})
}

func (r *LoopWorkStateRepository) ClearCurrent(ctx context.Context, loopID string) error {
	loopID = strings.TrimSpace(loopID)
	if loopID == "" {
		return fmt.Errorf("loopID is required")
	}
	_, err := r.db.ExecContext(ctx, `
		UPDATE loop_work_state
		SET is_current = 0
		WHERE loop_id = ? AND is_current = 1
	`, loopID)
	if err != nil {
		return fmt.Errorf("failed to clear current loop work state: %w", err)
	}
	return nil
}

func (r *LoopWorkStateRepository) GetCurrent(ctx context.Context, loopID string) (*models.LoopWorkState, error) {
	row := r.db.QueryRowContext(ctx, `
		SELECT id, loop_id, agent_id, task_id, status, detail, loop_iteration, is_current, created_at, updated_at
		FROM loop_work_state
		WHERE loop_id = ? AND is_current = 1
		ORDER BY updated_at DESC, id DESC
		LIMIT 1
	`, strings.TrimSpace(loopID))
	return r.scanLoopWorkState(row)
}

func (r *LoopWorkStateRepository) ListByLoop(ctx context.Context, loopID string, limit int) ([]*models.LoopWorkState, error) {
	if limit <= 0 {
		limit = 200
	}
	rows, err := r.db.QueryContext(ctx, `
		SELECT id, loop_id, agent_id, task_id, status, detail, loop_iteration, is_current, created_at, updated_at
		FROM loop_work_state
		WHERE loop_id = ?
		ORDER BY is_current DESC, updated_at DESC, id DESC
		LIMIT ?
	`, strings.TrimSpace(loopID), limit)
	if err != nil {
		return nil, fmt.Errorf("failed to query loop work state: %w", err)
	}
	defer rows.Close()

	out := make([]*models.LoopWorkState, 0)
	for rows.Next() {
		item, err := r.scanLoopWorkState(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, item)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating loop work state: %w", err)
	}
	return out, nil
}

func (r *LoopWorkStateRepository) scanLoopWorkState(scanner interface{ Scan(...any) error }) (*models.LoopWorkState, error) {
	var (
		id            string
		loopID        string
		agentID       string
		taskID        string
		status        string
		detail        sql.NullString
		loopIteration int
		isCurrentInt  int
		createdAt     string
		updatedAt     string
	)
	if err := scanner.Scan(
		&id,
		&loopID,
		&agentID,
		&taskID,
		&status,
		&detail,
		&loopIteration,
		&isCurrentInt,
		&createdAt,
		&updatedAt,
	); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrLoopWorkStateNotFound
		}
		return nil, fmt.Errorf("failed to scan loop work state: %w", err)
	}

	item := &models.LoopWorkState{
		ID:            id,
		LoopID:        loopID,
		AgentID:       agentID,
		TaskID:        taskID,
		Status:        status,
		Detail:        detail.String,
		LoopIteration: loopIteration,
		IsCurrent:     isCurrentInt == 1,
	}
	if t, err := time.Parse(time.RFC3339, createdAt); err == nil {
		item.CreatedAt = t
	}
	if t, err := time.Parse(time.RFC3339, updatedAt); err == nil {
		item.UpdatedAt = t
	}
	return item, nil
}
