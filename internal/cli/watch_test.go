package cli

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"testing"
	"time"

	"github.com/opencode-ai/swarm/internal/db"
	"github.com/opencode-ai/swarm/internal/models"
)

func setupTestDB(t *testing.T) *db.DB {
	t.Helper()
	database, err := db.OpenInMemory()
	if err != nil {
		t.Fatalf("failed to open database: %v", err)
	}

	if err := database.Migrate(context.Background()); err != nil {
		t.Fatalf("failed to migrate: %v", err)
	}

	return database
}

func TestEventStreamer_WriteEvent(t *testing.T) {
	database := setupTestDB(t)
	defer database.Close()

	repo := db.NewEventRepository(database)

	var buf bytes.Buffer
	config := DefaultStreamConfig()
	config.PollInterval = 10 * time.Millisecond

	streamer := NewEventStreamer(repo, &buf, config)

	// Test writeEvent directly
	event := &models.Event{
		ID:         "test-event-1",
		Timestamp:  time.Now().UTC(),
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-1",
	}

	if err := streamer.writeEvent(event); err != nil {
		t.Fatalf("writeEvent failed: %v", err)
	}

	// Verify output is valid JSON
	var decoded models.Event
	if err := json.Unmarshal(buf.Bytes(), &decoded); err != nil {
		t.Fatalf("output is not valid JSON: %v", err)
	}

	if decoded.ID != event.ID {
		t.Errorf("expected ID %q, got %q", event.ID, decoded.ID)
	}
	if decoded.Type != event.Type {
		t.Errorf("expected Type %q, got %q", event.Type, decoded.Type)
	}
}

func TestEventStreamer_Poll(t *testing.T) {
	database := setupTestDB(t)
	defer database.Close()

	repo := db.NewEventRepository(database)
	ctx := context.Background()

	// Create some events
	for i := 0; i < 5; i++ {
		event := &models.Event{
			Type:       models.EventTypeAgentStateChanged,
			EntityType: models.EntityTypeAgent,
			EntityID:   "agent-1",
			Payload:    json.RawMessage(`{"old_state":"idle","new_state":"working"}`),
		}
		if err := repo.Create(ctx, event); err != nil {
			t.Fatalf("failed to create event: %v", err)
		}
		time.Sleep(10 * time.Millisecond) // Ensure different timestamps
	}

	var buf bytes.Buffer
	config := DefaultStreamConfig()
	config.BatchSize = 2

	streamer := NewEventStreamer(repo, &buf, config)

	// Poll should return up to BatchSize events
	past := time.Now().Add(-1 * time.Hour)
	events, cursor, err := streamer.poll(ctx, "", &past)
	if err != nil {
		t.Fatalf("poll failed: %v", err)
	}

	if len(events) != 2 {
		t.Errorf("expected 2 events, got %d", len(events))
	}

	if cursor == "" {
		t.Error("expected non-empty cursor for pagination")
	}
}

func TestEventStreamer_FilterByEntityType(t *testing.T) {
	database := setupTestDB(t)
	defer database.Close()

	repo := db.NewEventRepository(database)
	ctx := context.Background()

	// Create agent event
	agentEvent := &models.Event{
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-1",
	}
	if err := repo.Create(ctx, agentEvent); err != nil {
		t.Fatalf("failed to create agent event: %v", err)
	}

	// Create node event
	nodeEvent := &models.Event{
		Type:       models.EventTypeNodeOnline,
		EntityType: models.EntityTypeNode,
		EntityID:   "node-1",
	}
	if err := repo.Create(ctx, nodeEvent); err != nil {
		t.Fatalf("failed to create node event: %v", err)
	}

	var buf bytes.Buffer
	config := DefaultStreamConfig()
	config.EntityTypes = []models.EntityType{models.EntityTypeAgent}

	streamer := NewEventStreamer(repo, &buf, config)

	past := time.Now().Add(-1 * time.Hour)
	events, _, err := streamer.poll(ctx, "", &past)
	if err != nil {
		t.Fatalf("poll failed: %v", err)
	}

	if len(events) != 1 {
		t.Errorf("expected 1 event, got %d", len(events))
	}

	if events[0].EntityType != models.EntityTypeAgent {
		t.Errorf("expected agent event, got %s", events[0].EntityType)
	}
}

func TestEventStreamer_StreamWithCancellation(t *testing.T) {
	database := setupTestDB(t)
	defer database.Close()

	repo := db.NewEventRepository(database)

	var buf bytes.Buffer
	config := DefaultStreamConfig()
	config.PollInterval = 10 * time.Millisecond

	streamer := NewEventStreamer(repo, &buf, config)

	// Create a context that cancels after a short delay
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()

	// Stream should return nil on context cancellation
	err := streamer.Stream(ctx)
	if err != nil {
		t.Errorf("expected nil error on cancellation, got: %v", err)
	}
}

func TestDefaultStreamConfig(t *testing.T) {
	config := DefaultStreamConfig()

	if config.PollInterval != 500*time.Millisecond {
		t.Errorf("expected PollInterval 500ms, got %v", config.PollInterval)
	}

	if config.BatchSize != 100 {
		t.Errorf("expected BatchSize 100, got %d", config.BatchSize)
	}

	if config.IncludeExisting {
		t.Error("expected IncludeExisting to be false by default")
	}
}

func TestMustBeJSONLForWatch(t *testing.T) {
	// Save original values
	origWatch := watchMode
	origJSONL := jsonlOutput
	defer func() {
		watchMode = origWatch
		jsonlOutput = origJSONL
	}()

	tests := []struct {
		name      string
		watch     bool
		jsonl     bool
		wantError bool
	}{
		{"watch without jsonl", true, false, true},
		{"watch with jsonl", true, true, false},
		{"no watch", false, false, false},
		{"no watch with jsonl", false, true, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			watchMode = tt.watch
			jsonlOutput = tt.jsonl

			err := MustBeJSONLForWatch()
			if tt.wantError && err == nil {
				t.Error("expected error but got nil")
			}
			if !tt.wantError && err != nil {
				t.Errorf("expected no error but got: %v", err)
			}
		})
	}
}

func TestParseSince(t *testing.T) {
	tests := []struct {
		name      string
		input     string
		wantErr   bool
		checkFunc func(*time.Time) bool // returns true if parsed time is valid
	}{
		{
			name:    "empty string",
			input:   "",
			wantErr: false,
			checkFunc: func(t *time.Time) bool {
				return t == nil
			},
		},
		{
			name:    "1 hour duration",
			input:   "1h",
			wantErr: false,
			checkFunc: func(t *time.Time) bool {
				if t == nil {
					return false
				}
				// Should be roughly 1 hour ago
				diff := time.Since(*t)
				return diff >= 59*time.Minute && diff <= 61*time.Minute
			},
		},
		{
			name:    "30 minutes duration",
			input:   "30m",
			wantErr: false,
			checkFunc: func(t *time.Time) bool {
				if t == nil {
					return false
				}
				diff := time.Since(*t)
				return diff >= 29*time.Minute && diff <= 31*time.Minute
			},
		},
		{
			name:    "1 day duration",
			input:   "1d",
			wantErr: false,
			checkFunc: func(t *time.Time) bool {
				if t == nil {
					return false
				}
				diff := time.Since(*t)
				return diff >= 23*time.Hour && diff <= 25*time.Hour
			},
		},
		{
			name:    "7 days duration",
			input:   "7d",
			wantErr: false,
			checkFunc: func(t *time.Time) bool {
				if t == nil {
					return false
				}
				diff := time.Since(*t)
				return diff >= 6*24*time.Hour && diff <= 8*24*time.Hour
			},
		},
		{
			name:    "RFC3339 timestamp",
			input:   "2024-01-15T10:30:00Z",
			wantErr: false,
			checkFunc: func(t *time.Time) bool {
				if t == nil {
					return false
				}
				expected := time.Date(2024, 1, 15, 10, 30, 0, 0, time.UTC)
				return t.Equal(expected)
			},
		},
		{
			name:    "RFC3339 with timezone",
			input:   "2024-01-15T10:30:00-05:00",
			wantErr: false,
			checkFunc: func(t *time.Time) bool {
				if t == nil {
					return false
				}
				// Should be converted to UTC: 15:30 UTC
				expected := time.Date(2024, 1, 15, 15, 30, 0, 0, time.UTC)
				return t.Equal(expected)
			},
		},
		{
			name:    "simple date",
			input:   "2024-01-15",
			wantErr: false,
			checkFunc: func(t *time.Time) bool {
				if t == nil {
					return false
				}
				expected := time.Date(2024, 1, 15, 0, 0, 0, 0, time.UTC)
				return t.Equal(expected)
			},
		},
		{
			name:    "date with time no timezone",
			input:   "2024-01-15T10:30:00",
			wantErr: false,
			checkFunc: func(t *time.Time) bool {
				if t == nil {
					return false
				}
				return t.Year() == 2024 && t.Month() == 1 && t.Day() == 15 &&
					t.Hour() == 10 && t.Minute() == 30
			},
		},
		{
			name:    "invalid format",
			input:   "not-a-time",
			wantErr: true,
		},
		{
			name:    "invalid duration",
			input:   "abc123",
			wantErr: true,
		},
		{
			name:    "whitespace trimmed",
			input:   "  1h  ",
			wantErr: false,
			checkFunc: func(t *time.Time) bool {
				if t == nil {
					return false
				}
				diff := time.Since(*t)
				return diff >= 59*time.Minute && diff <= 61*time.Minute
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ParseSince(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Errorf("ParseSince(%q) expected error, got nil", tt.input)
				}
				return
			}
			if err != nil {
				t.Errorf("ParseSince(%q) unexpected error: %v", tt.input, err)
				return
			}
			if tt.checkFunc != nil && !tt.checkFunc(got) {
				t.Errorf("ParseSince(%q) = %v, did not pass validation", tt.input, got)
			}
		})
	}
}

func TestParseDurationWithDays(t *testing.T) {
	tests := []struct {
		input    string
		expected time.Duration
		wantErr  bool
	}{
		{"1d", 24 * time.Hour, false},
		{"7d", 7 * 24 * time.Hour, false},
		{"0.5d", 12 * time.Hour, false},
		{"1h", time.Hour, false},
		{"30m", 30 * time.Minute, false},
		{"1h30m", 90 * time.Minute, false},
		{"invalid", 0, true},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got, err := parseDurationWithDays(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Errorf("parseDurationWithDays(%q) expected error", tt.input)
				}
				return
			}
			if err != nil {
				t.Errorf("parseDurationWithDays(%q) error: %v", tt.input, err)
				return
			}
			if got != tt.expected {
				t.Errorf("parseDurationWithDays(%q) = %v, want %v", tt.input, got, tt.expected)
			}
		})
	}
}

func TestStreamEventsWithReplay(t *testing.T) {
	database := setupTestDB(t)
	defer database.Close()

	repo := db.NewEventRepository(database)
	ctx := context.Background()

	// Record the time before creating events
	beforeCreation := time.Now().Add(-1 * time.Second).UTC()

	// Create events
	for i := 0; i < 5; i++ {
		event := &models.Event{
			Type:       models.EventTypeAgentStateChanged,
			EntityType: models.EntityTypeAgent,
			EntityID:   "agent-1",
			Payload:    json.RawMessage(`{"old_state":"idle","new_state":"working"}`),
		}
		if err := repo.Create(ctx, event); err != nil {
			t.Fatalf("failed to create event: %v", err)
		}
		time.Sleep(5 * time.Millisecond) // Small delay between events
	}

	var buf bytes.Buffer

	// Use a custom config with shorter poll interval for testing
	config := DefaultStreamConfig()
	config.PollInterval = 10 * time.Millisecond
	config.Since = &beforeCreation
	config.IncludeExisting = true

	streamer := NewEventStreamer(repo, &buf, config)

	// Create a context with short timeout (but long enough for at least one poll)
	ctxWithTimeout, cancel := context.WithTimeout(ctx, 100*time.Millisecond)
	defer cancel()

	err := streamer.Stream(ctxWithTimeout)

	// Should return nil on context timeout (graceful shutdown)
	if err != nil {
		t.Errorf("StreamEventsWithReplay error: %v", err)
	}

	// Check that some events were written
	if buf.Len() == 0 {
		t.Error("expected some events to be written")
	}

	// Count lines (each event is a JSON line)
	lines := bytes.Count(buf.Bytes(), []byte("\n"))
	if lines < 5 {
		t.Errorf("expected at least 5 events, got %d lines", lines)
	}
}

func TestEventStreamer_IncludeExisting(t *testing.T) {
	database := setupTestDB(t)
	defer database.Close()

	repo := db.NewEventRepository(database)
	ctx := context.Background()

	// Create historical events
	for i := 0; i < 3; i++ {
		event := &models.Event{
			Type:       models.EventTypeAgentSpawned,
			EntityType: models.EntityTypeAgent,
			EntityID:   fmt.Sprintf("agent-%d", i),
		}
		if err := repo.Create(ctx, event); err != nil {
			t.Fatalf("failed to create event: %v", err)
		}
	}

	var buf bytes.Buffer
	config := DefaultStreamConfig()
	config.PollInterval = 10 * time.Millisecond
	config.IncludeExisting = true
	since := time.Now().Add(-1 * time.Hour)
	config.Since = &since

	streamer := NewEventStreamer(repo, &buf, config)

	// Run for a short time
	ctxWithTimeout, cancel := context.WithTimeout(ctx, 50*time.Millisecond)
	defer cancel()

	err := streamer.Stream(ctxWithTimeout)
	if err != nil {
		t.Errorf("Stream error: %v", err)
	}

	// Should have output the historical events
	if buf.Len() == 0 {
		t.Error("expected historical events to be written when IncludeExisting is true")
	}

	// Count lines (each event is a JSON line)
	lines := bytes.Count(buf.Bytes(), []byte("\n"))
	if lines < 3 {
		t.Errorf("expected at least 3 events, got %d lines", lines)
	}
}
