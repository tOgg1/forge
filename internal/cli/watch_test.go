package cli

import (
	"bytes"
	"context"
	"encoding/json"
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
