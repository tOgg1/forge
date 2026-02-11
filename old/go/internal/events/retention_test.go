package events

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/config"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

func setupTestDB(t *testing.T) (*db.DB, *db.EventRepository) {
	t.Helper()
	database, err := db.OpenInMemory()
	if err != nil {
		t.Fatalf("failed to open database: %v", err)
	}
	if _, err := database.MigrateUp(context.Background()); err != nil {
		database.Close()
		t.Fatalf("failed to migrate database: %v", err)
	}
	return database, db.NewEventRepository(database)
}

func TestRetentionService_CleanupByAge(t *testing.T) {
	tmpDir := t.TempDir()
	database, repo := setupTestDB(t)
	defer database.Close()

	ctx := context.Background()

	// Create events with different timestamps
	now := time.Now().UTC()
	oldEvent := &models.Event{
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-1",
		Timestamp:  now.Add(-48 * time.Hour), // 2 days old
	}
	recentEvent := &models.Event{
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-2",
		Timestamp:  now.Add(-1 * time.Hour), // 1 hour old
	}

	if err := repo.Create(ctx, oldEvent); err != nil {
		t.Fatalf("failed to create old event: %v", err)
	}
	if err := repo.Create(ctx, recentEvent); err != nil {
		t.Fatalf("failed to create recent event: %v", err)
	}

	// Verify both exist
	count, err := repo.Count(ctx)
	if err != nil {
		t.Fatalf("failed to count events: %v", err)
	}
	if count != 2 {
		t.Errorf("expected 2 events, got %d", count)
	}

	// Create retention service with 24h max age
	cfg := config.DefaultConfig()
	cfg.Global.DataDir = tmpDir
	cfg.EventRetention.Enabled = true
	cfg.EventRetention.MaxAge = 24 * time.Hour
	cfg.EventRetention.MaxCount = 0
	cfg.EventRetention.CleanupInterval = 1 * time.Hour
	cfg.EventRetention.BatchSize = 100
	cfg.EventRetention.ArchiveBeforeDelete = false

	svc := NewRetentionService(cfg, repo)

	// Run cleanup
	if err := svc.RunCleanup(ctx); err != nil {
		t.Fatalf("cleanup failed: %v", err)
	}

	// Verify old event was deleted
	count, err = repo.Count(ctx)
	if err != nil {
		t.Fatalf("failed to count events: %v", err)
	}
	if count != 1 {
		t.Errorf("expected 1 event after cleanup, got %d", count)
	}

	// Verify the remaining event is the recent one
	page, err := repo.Query(ctx, db.EventQuery{Limit: 10})
	if err != nil {
		t.Fatalf("failed to query events: %v", err)
	}
	if len(page.Events) != 1 {
		t.Fatalf("expected 1 event, got %d", len(page.Events))
	}
	if page.Events[0].EntityID != "agent-2" {
		t.Errorf("expected remaining event to be agent-2, got %s", page.Events[0].EntityID)
	}
}

func TestRetentionService_CleanupByCount(t *testing.T) {
	tmpDir := t.TempDir()
	database, repo := setupTestDB(t)
	defer database.Close()

	ctx := context.Background()

	// Create 10 events
	now := time.Now().UTC()
	for i := 0; i < 10; i++ {
		event := &models.Event{
			Type:       models.EventTypeAgentSpawned,
			EntityType: models.EntityTypeAgent,
			EntityID:   "agent-" + string(rune('0'+i)),
			Timestamp:  now.Add(-time.Duration(10-i) * time.Hour), // Oldest first
		}
		if err := repo.Create(ctx, event); err != nil {
			t.Fatalf("failed to create event %d: %v", i, err)
		}
	}

	// Verify all exist
	count, err := repo.Count(ctx)
	if err != nil {
		t.Fatalf("failed to count events: %v", err)
	}
	if count != 10 {
		t.Errorf("expected 10 events, got %d", count)
	}

	// Create retention service with max 5 events
	cfg := config.DefaultConfig()
	cfg.Global.DataDir = tmpDir
	cfg.EventRetention.Enabled = true
	cfg.EventRetention.MaxAge = 0 // No age limit
	cfg.EventRetention.MaxCount = 5
	cfg.EventRetention.CleanupInterval = 1 * time.Hour
	cfg.EventRetention.BatchSize = 100
	cfg.EventRetention.ArchiveBeforeDelete = false

	svc := NewRetentionService(cfg, repo)

	// Run cleanup
	if err := svc.RunCleanup(ctx); err != nil {
		t.Fatalf("cleanup failed: %v", err)
	}

	// Verify only 5 events remain
	count, err = repo.Count(ctx)
	if err != nil {
		t.Fatalf("failed to count events: %v", err)
	}
	if count != 5 {
		t.Errorf("expected 5 events after cleanup, got %d", count)
	}
}

func TestRetentionService_ArchiveBeforeDelete(t *testing.T) {
	tmpDir := t.TempDir()
	archiveDir := filepath.Join(tmpDir, "archives")

	database, repo := setupTestDB(t)
	defer database.Close()

	ctx := context.Background()

	// Create an old event
	now := time.Now().UTC()
	oldTime := now.Add(-48 * time.Hour)
	oldEvent := &models.Event{
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-1",
		Timestamp:  oldTime,
		Payload:    json.RawMessage(`{"key":"value"}`),
	}

	if err := repo.Create(ctx, oldEvent); err != nil {
		t.Fatalf("failed to create event: %v", err)
	}

	// Create retention service with archive enabled
	cfg := config.DefaultConfig()
	cfg.Global.DataDir = tmpDir
	cfg.EventRetention.Enabled = true
	cfg.EventRetention.MaxAge = 24 * time.Hour
	cfg.EventRetention.MaxCount = 0
	cfg.EventRetention.CleanupInterval = 1 * time.Hour
	cfg.EventRetention.BatchSize = 100
	cfg.EventRetention.ArchiveBeforeDelete = true
	cfg.EventRetention.ArchiveDir = archiveDir

	svc := NewRetentionService(cfg, repo)

	// Run cleanup
	if err := svc.RunCleanup(ctx); err != nil {
		t.Fatalf("cleanup failed: %v", err)
	}

	// Verify event was deleted from database
	count, err := repo.Count(ctx)
	if err != nil {
		t.Fatalf("failed to count events: %v", err)
	}
	if count != 0 {
		t.Errorf("expected 0 events after cleanup, got %d", count)
	}

	// Verify archive file was created
	expectedFilename := "events_" + oldTime.Format("2006-01-02") + ".jsonl"
	archivePath := filepath.Join(archiveDir, expectedFilename)

	data, err := os.ReadFile(archivePath)
	if err != nil {
		t.Fatalf("failed to read archive file: %v", err)
	}

	// Verify archive contains the event
	var archivedEvent models.Event
	if err := json.Unmarshal(data[:len(data)-1], &archivedEvent); err != nil { // -1 for newline
		t.Fatalf("failed to unmarshal archived event: %v", err)
	}

	if archivedEvent.EntityID != "agent-1" {
		t.Errorf("expected archived event EntityID 'agent-1', got %s", archivedEvent.EntityID)
	}
}

func TestRetentionService_Stats(t *testing.T) {
	tmpDir := t.TempDir()
	database, repo := setupTestDB(t)
	defer database.Close()

	ctx := context.Background()

	// Create some events
	now := time.Now().UTC()
	for i := 0; i < 5; i++ {
		event := &models.Event{
			Type:       models.EventTypeAgentSpawned,
			EntityType: models.EntityTypeAgent,
			EntityID:   "agent-test",
			Timestamp:  now.Add(-time.Duration(i) * time.Hour),
		}
		if err := repo.Create(ctx, event); err != nil {
			t.Fatalf("failed to create event: %v", err)
		}
	}

	cfg := config.DefaultConfig()
	cfg.Global.DataDir = tmpDir
	cfg.EventRetention.Enabled = true
	cfg.EventRetention.MaxAge = 24 * time.Hour
	cfg.EventRetention.CleanupInterval = 1 * time.Hour

	svc := NewRetentionService(cfg, repo)

	stats, err := svc.Stats(ctx)
	if err != nil {
		t.Fatalf("failed to get stats: %v", err)
	}

	if stats.EventCount != 5 {
		t.Errorf("expected 5 events, got %d", stats.EventCount)
	}

	if stats.OldestEvent == nil {
		t.Error("expected oldest event timestamp, got nil")
	}
}

func TestRetentionService_DisabledByDefault(t *testing.T) {
	tmpDir := t.TempDir()
	database, repo := setupTestDB(t)
	defer database.Close()

	ctx := context.Background()

	cfg := config.DefaultConfig()
	cfg.Global.DataDir = tmpDir
	cfg.EventRetention.Enabled = false

	svc := NewRetentionService(cfg, repo)

	// Start should not error when disabled
	if err := svc.Start(ctx); err != nil {
		t.Fatalf("start failed: %v", err)
	}
	svc.Stop()
}

func TestEventRepository_Count(t *testing.T) {
	database, repo := setupTestDB(t)
	defer database.Close()

	ctx := context.Background()

	// Empty count
	count, err := repo.Count(ctx)
	if err != nil {
		t.Fatalf("failed to count: %v", err)
	}
	if count != 0 {
		t.Errorf("expected 0, got %d", count)
	}

	// Add events
	for i := 0; i < 3; i++ {
		event := &models.Event{
			Type:       models.EventTypeAgentSpawned,
			EntityType: models.EntityTypeAgent,
			EntityID:   "test",
		}
		if err := repo.Create(ctx, event); err != nil {
			t.Fatalf("failed to create event: %v", err)
		}
	}

	count, err = repo.Count(ctx)
	if err != nil {
		t.Fatalf("failed to count: %v", err)
	}
	if count != 3 {
		t.Errorf("expected 3, got %d", count)
	}
}

func TestEventRepository_DeleteOlderThan(t *testing.T) {
	database, repo := setupTestDB(t)
	defer database.Close()

	ctx := context.Background()
	now := time.Now().UTC()

	// Create old and new events
	oldEvent := &models.Event{
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "old",
		Timestamp:  now.Add(-2 * time.Hour),
	}
	newEvent := &models.Event{
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "new",
		Timestamp:  now,
	}

	if err := repo.Create(ctx, oldEvent); err != nil {
		t.Fatalf("failed to create old event: %v", err)
	}
	if err := repo.Create(ctx, newEvent); err != nil {
		t.Fatalf("failed to create new event: %v", err)
	}

	// Delete events older than 1 hour ago
	cutoff := now.Add(-1 * time.Hour)
	deleted, err := repo.DeleteOlderThan(ctx, cutoff, 100)
	if err != nil {
		t.Fatalf("delete failed: %v", err)
	}
	if deleted != 1 {
		t.Errorf("expected 1 deleted, got %d", deleted)
	}

	// Verify count
	count, err := repo.Count(ctx)
	if err != nil {
		t.Fatalf("count failed: %v", err)
	}
	if count != 1 {
		t.Errorf("expected 1 remaining, got %d", count)
	}
}

func TestEventRepository_DeleteExcess(t *testing.T) {
	database, repo := setupTestDB(t)
	defer database.Close()

	ctx := context.Background()

	// Create 10 events
	now := time.Now().UTC()
	for i := 0; i < 10; i++ {
		event := &models.Event{
			Type:       models.EventTypeAgentSpawned,
			EntityType: models.EntityTypeAgent,
			EntityID:   "test",
			Timestamp:  now.Add(time.Duration(i) * time.Minute),
		}
		if err := repo.Create(ctx, event); err != nil {
			t.Fatalf("failed to create event: %v", err)
		}
	}

	// Delete excess, keeping max 7
	deleted, err := repo.DeleteExcess(ctx, 7, 100)
	if err != nil {
		t.Fatalf("delete excess failed: %v", err)
	}
	if deleted != 3 {
		t.Errorf("expected 3 deleted, got %d", deleted)
	}

	count, err := repo.Count(ctx)
	if err != nil {
		t.Fatalf("count failed: %v", err)
	}
	if count != 7 {
		t.Errorf("expected 7 remaining, got %d", count)
	}
}
