package db

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"

	"github.com/opencode-ai/swarm/internal/models"
)

func TestEventRepositoryAppendAndQuery(t *testing.T) {
	ctx := context.Background()

	database, err := OpenInMemory()
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	defer database.Close()

	if _, err := database.MigrateUp(ctx); err != nil {
		t.Fatalf("migrate: %v", err)
	}

	repo := NewEventRepository(database)
	base := time.Now().UTC().Truncate(time.Second)

	event := &models.Event{
		Type:       models.EventTypeNodeOnline,
		EntityType: models.EntityTypeNode,
		EntityID:   "node-1",
		Timestamp:  base,
		Payload:    json.RawMessage(`{"status":"online"}`),
		Metadata:   map[string]string{"source": "test"},
	}

	if err := repo.Append(ctx, event); err != nil {
		t.Fatalf("Append: %v", err)
	}
	if event.ID == "" {
		t.Fatal("Append did not set event ID")
	}

	query := EventQuery{Type: &event.Type, Limit: 10}
	page, err := repo.Query(ctx, query)
	if err != nil {
		t.Fatalf("Query: %v", err)
	}

	if len(page.Events) != 1 {
		t.Fatalf("expected 1 event, got %d", len(page.Events))
	}

	got := page.Events[0]
	if got.Type != event.Type || got.EntityID != event.EntityID {
		t.Fatalf("unexpected event fields: %+v", got)
	}
	if string(got.Payload) != string(event.Payload) {
		t.Fatalf("unexpected payload: %s", string(got.Payload))
	}
	if got.Metadata["source"] != "test" {
		t.Fatalf("unexpected metadata: %+v", got.Metadata)
	}
}

func TestEventRepositoryCursorPagination(t *testing.T) {
	ctx := context.Background()

	database, err := OpenInMemory()
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	defer database.Close()

	if _, err := database.MigrateUp(ctx); err != nil {
		t.Fatalf("migrate: %v", err)
	}

	repo := NewEventRepository(database)
	base := time.Now().UTC().Truncate(time.Second)

	for i := 0; i < 3; i++ {
		event := &models.Event{
			Type:       models.EventTypeAgentSpawned,
			EntityType: models.EntityTypeAgent,
			EntityID:   "agent-1",
			Timestamp:  base.Add(time.Duration(i) * time.Second),
		}
		if err := repo.Append(ctx, event); err != nil {
			t.Fatalf("Append %d: %v", i, err)
		}
	}

	page, err := repo.Query(ctx, EventQuery{Limit: 2})
	if err != nil {
		t.Fatalf("Query: %v", err)
	}
	if len(page.Events) != 2 {
		t.Fatalf("expected 2 events, got %d", len(page.Events))
	}
	if page.NextCursor == "" {
		t.Fatal("expected NextCursor")
	}

	page2, err := repo.Query(ctx, EventQuery{Cursor: page.NextCursor, Limit: 2})
	if err != nil {
		t.Fatalf("Query page 2: %v", err)
	}
	if len(page2.Events) != 1 {
		t.Fatalf("expected 1 event, got %d", len(page2.Events))
	}
}

func TestEventRepositoryTimeRange(t *testing.T) {
	ctx := context.Background()

	database, err := OpenInMemory()
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	defer database.Close()

	if _, err := database.MigrateUp(ctx); err != nil {
		t.Fatalf("migrate: %v", err)
	}

	repo := NewEventRepository(database)
	base := time.Now().UTC().Truncate(time.Second)

	first := &models.Event{
		Type:       models.EventTypeWorkspaceCreated,
		EntityType: models.EntityTypeWorkspace,
		EntityID:   "ws-1",
		Timestamp:  base,
	}
	second := &models.Event{
		Type:       models.EventTypeWorkspaceCreated,
		EntityType: models.EntityTypeWorkspace,
		EntityID:   "ws-2",
		Timestamp:  base.Add(5 * time.Second),
	}

	if err := repo.Append(ctx, first); err != nil {
		t.Fatalf("Append first: %v", err)
	}
	if err := repo.Append(ctx, second); err != nil {
		t.Fatalf("Append second: %v", err)
	}

	since := base.Add(3 * time.Second)
	page, err := repo.Query(ctx, EventQuery{Since: &since})
	if err != nil {
		t.Fatalf("Query since: %v", err)
	}
	if len(page.Events) != 1 || page.Events[0].EntityID != "ws-2" {
		t.Fatalf("expected ws-2 event, got %+v", page.Events)
	}

	until := base.Add(2 * time.Second)
	page, err = repo.Query(ctx, EventQuery{Until: &until})
	if err != nil {
		t.Fatalf("Query until: %v", err)
	}
	if len(page.Events) != 1 || page.Events[0].EntityID != "ws-1" {
		t.Fatalf("expected ws-1 event, got %+v", page.Events)
	}
}

func TestEventRepositoryValidation(t *testing.T) {
	ctx := context.Background()

	database, err := OpenInMemory()
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	defer database.Close()

	if _, err := database.MigrateUp(ctx); err != nil {
		t.Fatalf("migrate: %v", err)
	}

	repo := NewEventRepository(database)

	err = repo.Append(ctx, &models.Event{})
	if !errors.Is(err, ErrInvalidEvent) {
		t.Fatalf("expected ErrInvalidEvent, got %v", err)
	}
}
