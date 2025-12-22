// Package cli provides the watch/streaming functionality for CLI commands.
package cli

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/opencode-ai/swarm/internal/db"
	"github.com/opencode-ai/swarm/internal/models"
)

// StreamConfig configures event streaming behavior.
type StreamConfig struct {
	// PollInterval is how often to check for new events.
	PollInterval time.Duration

	// EventTypes filters to specific event types (nil = all).
	EventTypes []models.EventType

	// EntityTypes filters to specific entity types (nil = all).
	EntityTypes []models.EntityType

	// EntityID filters to a specific entity.
	EntityID string

	// Since streams events after this timestamp.
	Since *time.Time

	// IncludeExisting includes events before streaming starts.
	IncludeExisting bool

	// BatchSize is the max events per poll.
	BatchSize int
}

// DefaultStreamConfig returns sensible defaults for streaming.
func DefaultStreamConfig() StreamConfig {
	return StreamConfig{
		PollInterval:    500 * time.Millisecond,
		IncludeExisting: false,
		BatchSize:       100,
	}
}

// EventStreamer streams events to an output writer in JSONL format.
type EventStreamer struct {
	repo   *db.EventRepository
	out    io.Writer
	config StreamConfig
	logger func(string, ...any)
}

// NewEventStreamer creates a new event streamer.
func NewEventStreamer(repo *db.EventRepository, out io.Writer, config StreamConfig) *EventStreamer {
	if config.PollInterval == 0 {
		config.PollInterval = 500 * time.Millisecond
	}
	if config.BatchSize == 0 {
		config.BatchSize = 100
	}
	return &EventStreamer{
		repo:   repo,
		out:    out,
		config: config,
		logger: func(format string, args ...any) {
			if IsVerbose() {
				fmt.Fprintf(os.Stderr, format+"\n", args...)
			}
		},
	}
}

// Stream starts streaming events until the context is cancelled.
// Returns nil on graceful shutdown (Ctrl+C), error otherwise.
func (s *EventStreamer) Stream(ctx context.Context) error {
	// Set up signal handling for graceful shutdown
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		select {
		case <-sigChan:
			s.logger("Received interrupt, shutting down...")
			cancel()
		case <-ctx.Done():
		}
	}()

	// Initialize cursor
	var cursor string
	var since *time.Time

	if s.config.IncludeExisting {
		// Start from the beginning or specified time
		since = s.config.Since
	} else {
		// Start from now
		now := time.Now().UTC()
		since = &now
	}

	ticker := time.NewTicker(s.config.PollInterval)
	defer ticker.Stop()

	s.logger("Starting event stream (poll interval: %v)", s.config.PollInterval)

	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticker.C:
			events, nextCursor, err := s.poll(ctx, cursor, since)
			if err != nil {
				if ctx.Err() != nil {
					return nil // Context cancelled, graceful shutdown
				}
				return fmt.Errorf("failed to poll events: %w", err)
			}

			for _, event := range events {
				if err := s.writeEvent(event); err != nil {
					return fmt.Errorf("failed to write event: %w", err)
				}
			}

			if nextCursor != "" {
				cursor = nextCursor
				since = nil // Use cursor-based pagination after first batch
			}
		}
	}
}

// poll fetches the next batch of events.
func (s *EventStreamer) poll(ctx context.Context, cursor string, since *time.Time) ([]*models.Event, string, error) {
	query := db.EventQuery{
		Cursor: cursor,
		Since:  since,
		Limit:  s.config.BatchSize,
	}

	// Apply filters
	if len(s.config.EventTypes) == 1 {
		query.Type = &s.config.EventTypes[0]
	}
	if len(s.config.EntityTypes) == 1 {
		query.EntityType = &s.config.EntityTypes[0]
	}
	if s.config.EntityID != "" {
		query.EntityID = &s.config.EntityID
	}

	page, err := s.repo.Query(ctx, query)
	if err != nil {
		return nil, "", err
	}

	// Filter by multiple event types if specified
	var filtered []*models.Event
	if len(s.config.EventTypes) > 1 {
		typeSet := make(map[models.EventType]bool)
		for _, t := range s.config.EventTypes {
			typeSet[t] = true
		}
		for _, e := range page.Events {
			if typeSet[e.Type] {
				filtered = append(filtered, e)
			}
		}
	} else {
		filtered = page.Events
	}

	// Filter by multiple entity types if specified
	if len(s.config.EntityTypes) > 1 {
		typeSet := make(map[models.EntityType]bool)
		for _, t := range s.config.EntityTypes {
			typeSet[t] = true
		}
		var refiltered []*models.Event
		for _, e := range filtered {
			if typeSet[e.EntityType] {
				refiltered = append(refiltered, e)
			}
		}
		filtered = refiltered
	}

	return filtered, page.NextCursor, nil
}

// writeEvent writes a single event as JSONL.
func (s *EventStreamer) writeEvent(event *models.Event) error {
	data, err := json.Marshal(event)
	if err != nil {
		return err
	}
	_, err = fmt.Fprintln(s.out, string(data))
	return err
}

// StreamEvents is a convenience function to stream events with default config.
// It blocks until Ctrl+C or context cancellation.
func StreamEvents(ctx context.Context, repo *db.EventRepository, out io.Writer) error {
	streamer := NewEventStreamer(repo, out, DefaultStreamConfig())
	return streamer.Stream(ctx)
}

// StreamEventsWithFilter streams events matching the given filters.
func StreamEventsWithFilter(
	ctx context.Context,
	repo *db.EventRepository,
	out io.Writer,
	eventTypes []models.EventType,
	entityTypes []models.EntityType,
	entityID string,
) error {
	config := DefaultStreamConfig()
	config.EventTypes = eventTypes
	config.EntityTypes = entityTypes
	config.EntityID = entityID

	streamer := NewEventStreamer(repo, out, config)
	return streamer.Stream(ctx)
}

// WatchHelper provides a standard way for commands to implement --watch mode.
// It returns true if watch mode is active and the command should stream.
func WatchHelper(ctx context.Context, repo *db.EventRepository, entityType models.EntityType, entityID string) error {
	if !IsWatchMode() {
		return nil
	}

	config := DefaultStreamConfig()
	config.EntityTypes = []models.EntityType{entityType}
	if entityID != "" {
		config.EntityID = entityID
	}

	streamer := NewEventStreamer(repo, os.Stdout, config)
	return streamer.Stream(ctx)
}

// MustBeJSONLForWatch ensures JSONL mode is used with --watch.
// Returns an error if --watch is used without --jsonl.
func MustBeJSONLForWatch() error {
	if IsWatchMode() && !IsJSONLOutput() {
		return fmt.Errorf("--watch requires --jsonl output format")
	}
	return nil
}
