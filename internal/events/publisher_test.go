package events

import (
	"context"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/opencode-ai/swarm/internal/models"
)

func TestFilter_Matches(t *testing.T) {
	tests := []struct {
		name   string
		filter Filter
		event  *models.Event
		want   bool
	}{
		{
			name:   "empty filter matches any event",
			filter: Filter{},
			event: &models.Event{
				Type:       models.EventTypeAgentSpawned,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-1",
			},
			want: true,
		},
		{
			name:   "nil event returns false",
			filter: Filter{},
			event:  nil,
			want:   false,
		},
		{
			name: "event type filter matches",
			filter: Filter{
				EventTypes: []models.EventType{models.EventTypeAgentSpawned},
			},
			event: &models.Event{
				Type:       models.EventTypeAgentSpawned,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-1",
			},
			want: true,
		},
		{
			name: "event type filter rejects non-matching",
			filter: Filter{
				EventTypes: []models.EventType{models.EventTypeAgentSpawned},
			},
			event: &models.Event{
				Type:       models.EventTypeAgentTerminated,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-1",
			},
			want: false,
		},
		{
			name: "multiple event types - matches any",
			filter: Filter{
				EventTypes: []models.EventType{
					models.EventTypeAgentSpawned,
					models.EventTypeAgentTerminated,
				},
			},
			event: &models.Event{
				Type:       models.EventTypeAgentTerminated,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-1",
			},
			want: true,
		},
		{
			name: "entity type filter matches",
			filter: Filter{
				EntityTypes: []models.EntityType{models.EntityTypeAgent},
			},
			event: &models.Event{
				Type:       models.EventTypeAgentSpawned,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-1",
			},
			want: true,
		},
		{
			name: "entity type filter rejects non-matching",
			filter: Filter{
				EntityTypes: []models.EntityType{models.EntityTypeNode},
			},
			event: &models.Event{
				Type:       models.EventTypeAgentSpawned,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-1",
			},
			want: false,
		},
		{
			name: "entity ID filter matches",
			filter: Filter{
				EntityID: "agent-1",
			},
			event: &models.Event{
				Type:       models.EventTypeAgentSpawned,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-1",
			},
			want: true,
		},
		{
			name: "entity ID filter rejects non-matching",
			filter: Filter{
				EntityID: "agent-1",
			},
			event: &models.Event{
				Type:       models.EventTypeAgentSpawned,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-2",
			},
			want: false,
		},
		{
			name: "combined filters - all must match",
			filter: Filter{
				EventTypes:  []models.EventType{models.EventTypeAgentSpawned},
				EntityTypes: []models.EntityType{models.EntityTypeAgent},
				EntityID:    "agent-1",
			},
			event: &models.Event{
				Type:       models.EventTypeAgentSpawned,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-1",
			},
			want: true,
		},
		{
			name: "combined filters - entity type mismatch",
			filter: Filter{
				EventTypes:  []models.EventType{models.EventTypeAgentSpawned},
				EntityTypes: []models.EntityType{models.EntityTypeNode},
				EntityID:    "agent-1",
			},
			event: &models.Event{
				Type:       models.EventTypeAgentSpawned,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-1",
			},
			want: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := tt.filter.Matches(tt.event)
			if got != tt.want {
				t.Errorf("Filter.Matches() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestInMemoryPublisher_Subscribe(t *testing.T) {
	pub := NewInMemoryPublisher()

	handler := func(event *models.Event) {}

	// Subscribe successfully
	err := pub.Subscribe("sub-1", Filter{}, handler)
	if err != nil {
		t.Errorf("Subscribe() error = %v, want nil", err)
	}

	if pub.SubscriberCount() != 1 {
		t.Errorf("SubscriberCount() = %d, want 1", pub.SubscriberCount())
	}

	// Duplicate subscription should fail
	err = pub.Subscribe("sub-1", Filter{}, handler)
	if err != ErrSubscriptionExists {
		t.Errorf("Subscribe() duplicate error = %v, want %v", err, ErrSubscriptionExists)
	}

	// Empty ID should fail
	err = pub.Subscribe("", Filter{}, handler)
	if err != ErrInvalidSubscriptionID {
		t.Errorf("Subscribe() empty ID error = %v, want %v", err, ErrInvalidSubscriptionID)
	}

	// Nil handler should fail
	err = pub.Subscribe("sub-2", Filter{}, nil)
	if err != ErrNilHandler {
		t.Errorf("Subscribe() nil handler error = %v, want %v", err, ErrNilHandler)
	}
}

func TestInMemoryPublisher_Unsubscribe(t *testing.T) {
	pub := NewInMemoryPublisher()

	handler := func(event *models.Event) {}

	// Subscribe first
	_ = pub.Subscribe("sub-1", Filter{}, handler)

	// Unsubscribe successfully
	err := pub.Unsubscribe("sub-1")
	if err != nil {
		t.Errorf("Unsubscribe() error = %v, want nil", err)
	}

	if pub.SubscriberCount() != 0 {
		t.Errorf("SubscriberCount() = %d, want 0", pub.SubscriberCount())
	}

	// Unsubscribe non-existent should fail
	err = pub.Unsubscribe("sub-1")
	if err != ErrSubscriptionNotFound {
		t.Errorf("Unsubscribe() non-existent error = %v, want %v", err, ErrSubscriptionNotFound)
	}
}

func TestInMemoryPublisher_Publish(t *testing.T) {
	pub := NewInMemoryPublisher()
	ctx := context.Background()

	var received []*models.Event
	var mu sync.Mutex

	handler := func(event *models.Event) {
		mu.Lock()
		received = append(received, event)
		mu.Unlock()
	}

	_ = pub.Subscribe("sub-1", Filter{}, handler)

	event := &models.Event{
		ID:         "event-1",
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-1",
	}

	pub.Publish(ctx, event)

	mu.Lock()
	if len(received) != 1 {
		t.Errorf("received %d events, want 1", len(received))
	}
	if len(received) > 0 && received[0].ID != event.ID {
		t.Errorf("received event ID = %s, want %s", received[0].ID, event.ID)
	}
	mu.Unlock()
}

func TestInMemoryPublisher_PublishWithFilter(t *testing.T) {
	pub := NewInMemoryPublisher()
	ctx := context.Background()

	var agentEvents, nodeEvents int
	var mu sync.Mutex

	// Agent event subscriber
	_ = pub.Subscribe("agent-sub", Filter{
		EntityTypes: []models.EntityType{models.EntityTypeAgent},
	}, func(event *models.Event) {
		mu.Lock()
		agentEvents++
		mu.Unlock()
	})

	// Node event subscriber
	_ = pub.Subscribe("node-sub", Filter{
		EntityTypes: []models.EntityType{models.EntityTypeNode},
	}, func(event *models.Event) {
		mu.Lock()
		nodeEvents++
		mu.Unlock()
	})

	// Publish agent event
	pub.Publish(ctx, &models.Event{
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-1",
	})

	// Publish node event
	pub.Publish(ctx, &models.Event{
		Type:       models.EventTypeNodeOnline,
		EntityType: models.EntityTypeNode,
		EntityID:   "node-1",
	})

	mu.Lock()
	if agentEvents != 1 {
		t.Errorf("agentEvents = %d, want 1", agentEvents)
	}
	if nodeEvents != 1 {
		t.Errorf("nodeEvents = %d, want 1", nodeEvents)
	}
	mu.Unlock()
}

func TestInMemoryPublisher_PublishNilEvent(t *testing.T) {
	pub := NewInMemoryPublisher()
	ctx := context.Background()

	called := false
	_ = pub.Subscribe("sub-1", Filter{}, func(event *models.Event) {
		called = true
	})

	// Publish nil event should not call handler
	pub.Publish(ctx, nil)

	if called {
		t.Error("handler was called for nil event")
	}
}

func TestInMemoryPublisher_PublishAsync(t *testing.T) {
	pub := NewInMemoryPublisher()
	ctx := context.Background()

	var count int64

	_ = pub.Subscribe("sub-1", Filter{}, func(event *models.Event) {
		atomic.AddInt64(&count, 1)
	})

	event := &models.Event{
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-1",
	}

	pub.PublishAsync(ctx, event)

	// Wait a bit for async handler
	time.Sleep(50 * time.Millisecond)

	if atomic.LoadInt64(&count) != 1 {
		t.Errorf("count = %d, want 1", count)
	}
}

func TestInMemoryPublisher_UpdateSubscription(t *testing.T) {
	pub := NewInMemoryPublisher()
	ctx := context.Background()

	var count int
	var mu sync.Mutex

	// Start with agent filter
	_ = pub.Subscribe("sub-1", Filter{
		EntityTypes: []models.EntityType{models.EntityTypeAgent},
	}, func(event *models.Event) {
		mu.Lock()
		count++
		mu.Unlock()
	})

	// Agent event should be received
	pub.Publish(ctx, &models.Event{
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-1",
	})

	// Update to node filter
	err := pub.UpdateSubscription("sub-1", Filter{
		EntityTypes: []models.EntityType{models.EntityTypeNode},
	})
	if err != nil {
		t.Errorf("UpdateSubscription() error = %v", err)
	}

	// Agent event should NOT be received now
	pub.Publish(ctx, &models.Event{
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-2",
	})

	// Node event should be received
	pub.Publish(ctx, &models.Event{
		Type:       models.EventTypeNodeOnline,
		EntityType: models.EntityTypeNode,
		EntityID:   "node-1",
	})

	mu.Lock()
	if count != 2 {
		t.Errorf("count = %d, want 2 (1 agent + 1 node)", count)
	}
	mu.Unlock()
}

func TestInMemoryPublisher_UpdateSubscriptionNotFound(t *testing.T) {
	pub := NewInMemoryPublisher()

	err := pub.UpdateSubscription("nonexistent", Filter{})
	if err != ErrSubscriptionNotFound {
		t.Errorf("UpdateSubscription() error = %v, want %v", err, ErrSubscriptionNotFound)
	}
}

func TestInMemoryPublisher_Close(t *testing.T) {
	pub := NewInMemoryPublisher()

	_ = pub.Subscribe("sub-1", Filter{}, func(event *models.Event) {})
	_ = pub.Subscribe("sub-2", Filter{}, func(event *models.Event) {})

	if pub.SubscriberCount() != 2 {
		t.Errorf("SubscriberCount() before Close = %d, want 2", pub.SubscriberCount())
	}

	pub.Close()

	if pub.SubscriberCount() != 0 {
		t.Errorf("SubscriberCount() after Close = %d, want 0", pub.SubscriberCount())
	}
}

func TestInMemoryPublisher_ConcurrentAccess(t *testing.T) {
	pub := NewInMemoryPublisher()
	ctx := context.Background()

	var wg sync.WaitGroup
	var count int64

	// Concurrent subscribers
	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			subID := "sub-" + string(rune('a'+id))
			_ = pub.Subscribe(subID, Filter{}, func(event *models.Event) {
				atomic.AddInt64(&count, 1)
			})
		}(i)
	}

	wg.Wait()

	// Concurrent publish
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			pub.Publish(ctx, &models.Event{
				Type:       models.EventTypeAgentSpawned,
				EntityType: models.EntityTypeAgent,
				EntityID:   "agent-1",
			})
		}()
	}

	wg.Wait()

	// Should have 10 subscribers * 100 events = 1000 handler calls
	expected := int64(10 * 100)
	if atomic.LoadInt64(&count) != expected {
		t.Errorf("count = %d, want %d", count, expected)
	}
}

// mockRepository implements Repository for testing.
type mockRepository struct {
	mu     sync.Mutex
	events []*models.Event
}

func (m *mockRepository) Create(ctx context.Context, event *models.Event) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.events = append(m.events, event)
	return nil
}

func TestInMemoryPublisher_WithRepository(t *testing.T) {
	repo := &mockRepository{}
	pub := NewInMemoryPublisher(WithRepository(repo))
	ctx := context.Background()

	event := &models.Event{
		ID:         "event-1",
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "agent-1",
	}

	pub.Publish(ctx, event)

	repo.mu.Lock()
	if len(repo.events) != 1 {
		t.Errorf("repo.events = %d, want 1", len(repo.events))
	}
	if len(repo.events) > 0 && repo.events[0].ID != event.ID {
		t.Errorf("repo event ID = %s, want %s", repo.events[0].ID, event.ID)
	}
	repo.mu.Unlock()
}
