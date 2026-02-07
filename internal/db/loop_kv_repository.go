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
	ErrLoopKVNotFound = errors.New("loop kv not found")
)

type LoopKVRepository struct {
	db *DB
}

func NewLoopKVRepository(db *DB) *LoopKVRepository {
	return &LoopKVRepository{db: db}
}

func (r *LoopKVRepository) Set(ctx context.Context, loopID, key, value string) error {
	loopID = strings.TrimSpace(loopID)
	key = strings.TrimSpace(key)
	if loopID == "" {
		return fmt.Errorf("loopID is required")
	}
	if key == "" {
		return fmt.Errorf("key is required")
	}
	if value == "" {
		return fmt.Errorf("value is required")
	}

	now := time.Now().UTC()

	// Prefer UPDATE then INSERT to avoid relying on newer SQLite upsert syntax.
	result, err := r.db.ExecContext(ctx, `
		UPDATE loop_kv
		SET value = ?, updated_at = ?
		WHERE loop_id = ? AND key = ?
	`, value, now.Format(time.RFC3339), loopID, key)
	if err != nil {
		return fmt.Errorf("failed to update loop kv: %w", err)
	}
	rows, _ := result.RowsAffected()
	if rows > 0 {
		return nil
	}

	entry := &models.LoopKV{
		ID:        uuid.New().String(),
		LoopID:    loopID,
		Key:       key,
		Value:     value,
		CreatedAt: now,
		UpdatedAt: now,
	}
	if err := entry.Validate(); err != nil {
		return fmt.Errorf("invalid loop kv: %w", err)
	}

	_, err = r.db.ExecContext(ctx, `
		INSERT INTO loop_kv (id, loop_id, key, value, created_at, updated_at)
		VALUES (?, ?, ?, ?, ?, ?)
	`,
		entry.ID,
		entry.LoopID,
		entry.Key,
		entry.Value,
		entry.CreatedAt.Format(time.RFC3339),
		entry.UpdatedAt.Format(time.RFC3339),
	)
	if err != nil {
		if isUniqueConstraintError(err) {
			// Race: key inserted after our UPDATE check; retry UPDATE.
			_, err2 := r.db.ExecContext(ctx, `
				UPDATE loop_kv
				SET value = ?, updated_at = ?
				WHERE loop_id = ? AND key = ?
			`, value, now.Format(time.RFC3339), loopID, key)
			if err2 == nil {
				return nil
			}
		}
		return fmt.Errorf("failed to insert loop kv: %w", err)
	}

	return nil
}

func (r *LoopKVRepository) Get(ctx context.Context, loopID, key string) (*models.LoopKV, error) {
	row := r.db.QueryRowContext(ctx, `
		SELECT id, loop_id, key, value, created_at, updated_at
		FROM loop_kv
		WHERE loop_id = ? AND key = ?
	`, strings.TrimSpace(loopID), strings.TrimSpace(key))
	return r.scanLoopKV(row)
}

func (r *LoopKVRepository) ListByLoop(ctx context.Context, loopID string) ([]*models.LoopKV, error) {
	rows, err := r.db.QueryContext(ctx, `
		SELECT id, loop_id, key, value, created_at, updated_at
		FROM loop_kv
		WHERE loop_id = ?
		ORDER BY key
	`, strings.TrimSpace(loopID))
	if err != nil {
		return nil, fmt.Errorf("failed to query loop kv: %w", err)
	}
	defer rows.Close()

	out := make([]*models.LoopKV, 0)
	for rows.Next() {
		entry, err := r.scanLoopKV(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, entry)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating loop kv: %w", err)
	}
	return out, nil
}

func (r *LoopKVRepository) Delete(ctx context.Context, loopID, key string) error {
	result, err := r.db.ExecContext(ctx, `
		DELETE FROM loop_kv WHERE loop_id = ? AND key = ?
	`, strings.TrimSpace(loopID), strings.TrimSpace(key))
	if err != nil {
		return fmt.Errorf("failed to delete loop kv: %w", err)
	}
	rows, _ := result.RowsAffected()
	if rows == 0 {
		return ErrLoopKVNotFound
	}
	return nil
}

func (r *LoopKVRepository) scanLoopKV(scanner interface{ Scan(...any) error }) (*models.LoopKV, error) {
	var (
		id        string
		loopID    string
		key       string
		value     string
		createdAt string
		updatedAt string
	)
	if err := scanner.Scan(&id, &loopID, &key, &value, &createdAt, &updatedAt); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrLoopKVNotFound
		}
		return nil, fmt.Errorf("failed to scan loop kv: %w", err)
	}

	entry := &models.LoopKV{
		ID:     id,
		LoopID: loopID,
		Key:    key,
		Value:  value,
	}
	if t, err := time.Parse(time.RFC3339, createdAt); err == nil {
		entry.CreatedAt = t
	}
	if t, err := time.Parse(time.RFC3339, updatedAt); err == nil {
		entry.UpdatedAt = t
	}
	return entry, nil
}
