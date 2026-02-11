// Package db provides SQLite database access for Forge.
package db

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/tOgg1/forge/internal/models"
)

// Event repository errors.
var (
	ErrEventNotFound = errors.New("event not found")
	ErrInvalidEvent  = errors.New("invalid event")
)

// EventRepository handles event persistence.
type EventRepository struct {
	db *DB
}

type eventExecer interface {
	ExecContext(context.Context, string, ...any) (sql.Result, error)
}

// NewEventRepository creates a new EventRepository.
func NewEventRepository(db *DB) *EventRepository {
	return &EventRepository{db: db}
}

// EventQuery defines filters for querying events.
type EventQuery struct {
	Type       *models.EventType  // Filter by event type
	EntityType *models.EntityType // Filter by entity type
	EntityID   *string            // Filter by entity ID
	Since      *time.Time         // Events at or after this time (inclusive)
	Until      *time.Time         // Events before this time (exclusive)
	Cursor     string             // Pagination cursor (event ID)
	Limit      int                // Max results to return
}

// EventPage represents a page of query results.
type EventPage struct {
	Events     []*models.Event
	NextCursor string
}

// Append adds a new event to the event log.
// Returns ErrInvalidEvent if required fields are missing.
func (r *EventRepository) Append(ctx context.Context, event *models.Event) error {
	if event.Type == "" || event.EntityType == "" || event.EntityID == "" {
		return ErrInvalidEvent
	}
	return r.Create(ctx, event)
}

// Create appends a new event to the event log.
func (r *EventRepository) Create(ctx context.Context, event *models.Event) error {
	return r.createWithExecutor(ctx, r.db, event)
}

// CreateWithTx appends a new event using an existing transaction.
func (r *EventRepository) CreateWithTx(ctx context.Context, tx *sql.Tx, event *models.Event) error {
	if tx == nil {
		return fmt.Errorf("transaction is required")
	}
	return r.createWithExecutor(ctx, tx, event)
}

func (r *EventRepository) createWithExecutor(ctx context.Context, execer eventExecer, event *models.Event) error {
	if event.Type == "" {
		return fmt.Errorf("event type is required")
	}
	if event.EntityType == "" {
		return fmt.Errorf("event entity type is required")
	}
	if event.EntityID == "" {
		return fmt.Errorf("event entity id is required")
	}

	if event.ID == "" {
		event.ID = uuid.New().String()
	}
	if event.Timestamp.IsZero() {
		event.Timestamp = time.Now().UTC()
	} else {
		event.Timestamp = event.Timestamp.UTC()
	}

	var payloadJSON *string
	if len(event.Payload) > 0 {
		s := string(event.Payload)
		payloadJSON = &s
	}

	var metadataJSON *string
	if event.Metadata != nil {
		data, err := json.Marshal(event.Metadata)
		if err != nil {
			return fmt.Errorf("failed to marshal metadata: %w", err)
		}
		s := string(data)
		metadataJSON = &s
	}

	_, err := execer.ExecContext(ctx, `
		INSERT INTO events (
			id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
		) VALUES (?, ?, ?, ?, ?, ?, ?)
	`,
		event.ID,
		event.Timestamp.Format(time.RFC3339),
		string(event.Type),
		string(event.EntityType),
		event.EntityID,
		payloadJSON,
		metadataJSON,
	)
	if err != nil {
		return fmt.Errorf("failed to insert event: %w", err)
	}

	return nil
}

// Get retrieves an event by ID.
func (r *EventRepository) Get(ctx context.Context, id string) (*models.Event, error) {
	row := r.db.QueryRowContext(ctx, `
		SELECT id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
		FROM events WHERE id = ?
	`, id)

	return r.scanEvent(row)
}

// Query retrieves events matching the given filters with cursor-based pagination.
func (r *EventRepository) Query(ctx context.Context, q EventQuery) (*EventPage, error) {
	limit := q.Limit
	if limit <= 0 {
		limit = 100
	}

	// Build query dynamically
	query := `SELECT id, timestamp, type, entity_type, entity_id, payload_json, metadata_json FROM events WHERE 1=1`
	args := []any{}

	if q.Type != nil {
		query += ` AND type = ?`
		args = append(args, string(*q.Type))
	}
	if q.EntityType != nil {
		query += ` AND entity_type = ?`
		args = append(args, string(*q.EntityType))
	}
	if q.EntityID != nil {
		query += ` AND entity_id = ?`
		args = append(args, *q.EntityID)
	}
	if q.Since != nil {
		query += ` AND timestamp >= ?`
		args = append(args, q.Since.UTC().Format(time.RFC3339))
	}
	if q.Until != nil {
		query += ` AND timestamp < ?`
		args = append(args, q.Until.UTC().Format(time.RFC3339))
	}
	if q.Cursor != "" {
		// Cursor is the last event ID; fetch events with timestamp >= cursor's timestamp
		// but exclude events with same timestamp and id <= cursor
		query += ` AND (timestamp, id) > (SELECT timestamp, id FROM events WHERE id = ?)`
		args = append(args, q.Cursor)
	}

	query += ` ORDER BY timestamp, id LIMIT ?`
	args = append(args, limit+1) // Fetch one extra to determine if there's a next page

	rows, err := r.db.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("failed to query events: %w", err)
	}
	defer rows.Close()

	var events []*models.Event
	for rows.Next() {
		event, err := r.scanEventFromRows(rows)
		if err != nil {
			return nil, err
		}
		events = append(events, event)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating events: %w", err)
	}

	page := &EventPage{}
	if len(events) > limit {
		// There's a next page
		page.Events = events[:limit]
		page.NextCursor = events[limit-1].ID
	} else {
		page.Events = events
	}

	return page, nil
}

// ListByEntity retrieves events for an entity, ordered by timestamp.
func (r *EventRepository) ListByEntity(ctx context.Context, entityType models.EntityType, entityID string, limit int) ([]*models.Event, error) {
	if limit <= 0 {
		limit = 100
	}

	rows, err := r.db.QueryContext(ctx, `
		SELECT id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
		FROM events
		WHERE entity_type = ? AND entity_id = ?
		ORDER BY timestamp
		LIMIT ?
	`, string(entityType), entityID, limit)
	if err != nil {
		return nil, fmt.Errorf("failed to query events: %w", err)
	}
	defer rows.Close()

	var events []*models.Event
	for rows.Next() {
		event, err := r.scanEventFromRows(rows)
		if err != nil {
			return nil, err
		}
		events = append(events, event)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating events: %w", err)
	}

	return events, nil
}

func (r *EventRepository) scanEvent(row *sql.Row) (*models.Event, error) {
	var event models.Event
	var timestamp, eventType, entityType string
	var payloadJSON sql.NullString
	var metadataJSON sql.NullString

	err := row.Scan(
		&event.ID,
		&timestamp,
		&eventType,
		&entityType,
		&event.EntityID,
		&payloadJSON,
		&metadataJSON,
	)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrEventNotFound
		}
		return nil, fmt.Errorf("failed to scan event: %w", err)
	}

	event.Type = models.EventType(eventType)
	event.EntityType = models.EntityType(entityType)

	if t, err := time.Parse(time.RFC3339, timestamp); err == nil {
		event.Timestamp = t
	}

	if payloadJSON.Valid {
		event.Payload = json.RawMessage(payloadJSON.String)
	}
	if metadataJSON.Valid {
		if err := json.Unmarshal([]byte(metadataJSON.String), &event.Metadata); err != nil {
			r.db.logger.Warn().Err(err).Str("event_id", event.ID).Msg("failed to parse event metadata")
		}
	}

	return &event, nil
}

func (r *EventRepository) scanEventFromRows(rows *sql.Rows) (*models.Event, error) {
	var event models.Event
	var timestamp, eventType, entityType string
	var payloadJSON sql.NullString
	var metadataJSON sql.NullString

	if err := rows.Scan(
		&event.ID,
		&timestamp,
		&eventType,
		&entityType,
		&event.EntityID,
		&payloadJSON,
		&metadataJSON,
	); err != nil {
		return nil, fmt.Errorf("failed to scan event: %w", err)
	}

	event.Type = models.EventType(eventType)
	event.EntityType = models.EntityType(entityType)

	if t, err := time.Parse(time.RFC3339, timestamp); err == nil {
		event.Timestamp = t
	}

	if payloadJSON.Valid {
		event.Payload = json.RawMessage(payloadJSON.String)
	}
	if metadataJSON.Valid {
		if err := json.Unmarshal([]byte(metadataJSON.String), &event.Metadata); err != nil {
			r.db.logger.Warn().Err(err).Str("event_id", event.ID).Msg("failed to parse event metadata")
		}
	}

	return &event, nil
}

// Count returns the total number of events.
func (r *EventRepository) Count(ctx context.Context) (int64, error) {
	var count int64
	err := r.db.QueryRowContext(ctx, `SELECT COUNT(*) FROM events`).Scan(&count)
	if err != nil {
		return 0, fmt.Errorf("failed to count events: %w", err)
	}
	return count, nil
}

// OldestTimestamp returns the timestamp of the oldest event.
func (r *EventRepository) OldestTimestamp(ctx context.Context) (*time.Time, error) {
	var timestamp sql.NullString
	err := r.db.QueryRowContext(ctx, `SELECT MIN(timestamp) FROM events`).Scan(&timestamp)
	if err != nil {
		return nil, fmt.Errorf("failed to get oldest timestamp: %w", err)
	}
	if !timestamp.Valid {
		return nil, nil // No events
	}
	t, err := time.Parse(time.RFC3339, timestamp.String)
	if err != nil {
		return nil, fmt.Errorf("failed to parse oldest timestamp: %w", err)
	}
	return &t, nil
}

// DeleteOlderThan deletes events older than the given timestamp.
// Returns the number of events deleted.
func (r *EventRepository) DeleteOlderThan(ctx context.Context, before time.Time, limit int) (int64, error) {
	if limit <= 0 {
		limit = 1000
	}

	result, err := r.db.ExecContext(ctx, `
		DELETE FROM events WHERE id IN (
			SELECT id FROM events WHERE timestamp < ? ORDER BY timestamp LIMIT ?
		)
	`, before.UTC().Format(time.RFC3339), limit)
	if err != nil {
		return 0, fmt.Errorf("failed to delete old events: %w", err)
	}

	count, err := result.RowsAffected()
	if err != nil {
		return 0, fmt.Errorf("failed to get deleted count: %w", err)
	}
	return count, nil
}

// DeleteExcess deletes the oldest events beyond a maximum count.
// Returns the number of events deleted.
func (r *EventRepository) DeleteExcess(ctx context.Context, maxCount int, limit int) (int64, error) {
	if maxCount <= 0 {
		return 0, nil
	}
	if limit <= 0 {
		limit = 1000
	}

	// Count current events
	total, err := r.Count(ctx)
	if err != nil {
		return 0, err
	}

	excess := total - int64(maxCount)
	if excess <= 0 {
		return 0, nil
	}

	// Limit to batch size
	deleteCount := excess
	if deleteCount > int64(limit) {
		deleteCount = int64(limit)
	}

	result, err := r.db.ExecContext(ctx, `
		DELETE FROM events WHERE id IN (
			SELECT id FROM events ORDER BY timestamp LIMIT ?
		)
	`, deleteCount)
	if err != nil {
		return 0, fmt.Errorf("failed to delete excess events: %w", err)
	}

	count, err := result.RowsAffected()
	if err != nil {
		return 0, fmt.Errorf("failed to get deleted count: %w", err)
	}
	return count, nil
}

// ListOlderThan retrieves events older than the given timestamp, ordered by timestamp.
// Used for archiving before deletion.
func (r *EventRepository) ListOlderThan(ctx context.Context, before time.Time, limit int) ([]*models.Event, error) {
	if limit <= 0 {
		limit = 1000
	}

	rows, err := r.db.QueryContext(ctx, `
		SELECT id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
		FROM events
		WHERE timestamp < ?
		ORDER BY timestamp
		LIMIT ?
	`, before.UTC().Format(time.RFC3339), limit)
	if err != nil {
		return nil, fmt.Errorf("failed to query old events: %w", err)
	}
	defer rows.Close()

	var events []*models.Event
	for rows.Next() {
		event, err := r.scanEventFromRows(rows)
		if err != nil {
			return nil, err
		}
		events = append(events, event)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating old events: %w", err)
	}

	return events, nil
}

// ListOldest retrieves the oldest events up to the given limit.
// Used for archiving excess events before deletion.
func (r *EventRepository) ListOldest(ctx context.Context, limit int) ([]*models.Event, error) {
	if limit <= 0 {
		limit = 1000
	}

	rows, err := r.db.QueryContext(ctx, `
		SELECT id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
		FROM events
		ORDER BY timestamp
		LIMIT ?
	`, limit)
	if err != nil {
		return nil, fmt.Errorf("failed to query oldest events: %w", err)
	}
	defer rows.Close()

	var events []*models.Event
	for rows.Next() {
		event, err := r.scanEventFromRows(rows)
		if err != nil {
			return nil, err
		}
		events = append(events, event)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating oldest events: %w", err)
	}

	return events, nil
}

// DeleteByIDs deletes events by their IDs.
// Returns the number of events deleted.
func (r *EventRepository) DeleteByIDs(ctx context.Context, ids []string) (int64, error) {
	if len(ids) == 0 {
		return 0, nil
	}

	// Build placeholder list
	placeholders := make([]string, len(ids))
	args := make([]any, len(ids))
	for i, id := range ids {
		placeholders[i] = "?"
		args[i] = id
	}

	query := fmt.Sprintf(`DELETE FROM events WHERE id IN (%s)`, strings.Join(placeholders, ","))
	result, err := r.db.ExecContext(ctx, query, args...)
	if err != nil {
		return 0, fmt.Errorf("failed to delete events by ids: %w", err)
	}

	count, err := result.RowsAffected()
	if err != nil {
		return 0, fmt.Errorf("failed to get deleted count: %w", err)
	}
	return count, nil
}
