// Package db provides SQLite database access for Swarm.
package db

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"time"

	"github.com/google/uuid"
	"github.com/opencode-ai/swarm/internal/models"
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

// NewEventRepository creates a new EventRepository.
func NewEventRepository(db *DB) *EventRepository {
	return &EventRepository{db: db}
}

// EventQuery defines filters for querying events.
type EventQuery struct {
	Type       *models.EventType  // Filter by event type
	EntityType *models.EntityType // Filter by entity type
	EntityID   *string            // Filter by entity ID
	Since      *time.Time         // Events after this time (exclusive)
	Until      *time.Time         // Events before this time (exclusive)
	Cursor     string             // Pagination cursor (event ID)
	Limit      int                // Max results to return
}

// EventPage represents a page of query results.
type EventPage struct {
	Events     []*models.Event
	NextCursor string
}

// Create appends a new event to the event log.
func (r *EventRepository) Create(ctx context.Context, event *models.Event) error {
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

	_, err := r.db.ExecContext(ctx, `
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
