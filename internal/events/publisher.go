// Package events provides event publishing and subscription for Swarm.
package events

import (
	"context"
	"sync"

	"github.com/opencode-ai/swarm/internal/models"
)

// EventHandler is a callback function invoked when an event matches a subscription.
type EventHandler func(event *models.Event)

// Filter defines criteria for matching events.
type Filter struct {
	// EventTypes filters by event type (nil = all types).
	EventTypes []models.EventType

	// EntityTypes filters by entity type (nil = all entities).
	EntityTypes []models.EntityType

	// EntityID filters to a specific entity ID (empty = all).
	EntityID string
}

// Matches returns true if the event matches the filter criteria.
func (f *Filter) Matches(event *models.Event) bool {
	if event == nil {
		return false
	}

	// Check event type filter
	if len(f.EventTypes) > 0 {
		matched := false
		for _, t := range f.EventTypes {
			if event.Type == t {
				matched = true
				break
			}
		}
		if !matched {
			return false
		}
	}

	// Check entity type filter
	if len(f.EntityTypes) > 0 {
		matched := false
		for _, t := range f.EntityTypes {
			if event.EntityType == t {
				matched = true
				break
			}
		}
		if !matched {
			return false
		}
	}

	// Check entity ID filter
	if f.EntityID != "" && event.EntityID != f.EntityID {
		return false
	}

	return true
}

// subscription represents an active event subscription.
type subscription struct {
	id      string
	filter  Filter
	handler EventHandler
}

// Publisher defines the interface for event publishing and subscription.
type Publisher interface {
	// Publish sends an event to all matching subscribers.
	Publish(ctx context.Context, event *models.Event)

	// Subscribe registers a handler to receive events matching the filter.
	// Returns a subscription ID for later unsubscription.
	Subscribe(id string, filter Filter, handler EventHandler) error

	// Unsubscribe removes a subscription by ID.
	Unsubscribe(id string) error

	// SubscriberCount returns the number of active subscribers.
	SubscriberCount() int
}

// InMemoryPublisher implements Publisher using in-process pub/sub.
type InMemoryPublisher struct {
	mu            sync.RWMutex
	subscriptions map[string]*subscription
	// Optional: persist events to repository
	repo Repository
}

// PublisherOption configures an InMemoryPublisher.
type PublisherOption func(*InMemoryPublisher)

// WithRepository configures the publisher to also persist events.
func WithRepository(repo Repository) PublisherOption {
	return func(p *InMemoryPublisher) {
		p.repo = repo
	}
}

// NewInMemoryPublisher creates a new in-memory event publisher.
func NewInMemoryPublisher(opts ...PublisherOption) *InMemoryPublisher {
	p := &InMemoryPublisher{
		subscriptions: make(map[string]*subscription),
	}
	for _, opt := range opts {
		opt(p)
	}
	return p
}

// Publish sends an event to all matching subscribers.
// If a repository is configured, the event is also persisted.
func (p *InMemoryPublisher) Publish(ctx context.Context, event *models.Event) {
	if event == nil {
		return
	}

	// Persist to repository if configured
	if p.repo != nil {
		// Best effort - don't fail publish on persistence error
		_ = p.repo.Create(ctx, event)
	}

	// Get matching subscriptions under read lock
	p.mu.RLock()
	var handlers []EventHandler
	for _, sub := range p.subscriptions {
		if sub.filter.Matches(event) {
			handlers = append(handlers, sub.handler)
		}
	}
	p.mu.RUnlock()

	// Invoke handlers outside the lock to avoid deadlocks
	for _, handler := range handlers {
		handler(event)
	}
}

// PublishAsync sends an event to all matching subscribers asynchronously.
// Each handler is invoked in its own goroutine.
func (p *InMemoryPublisher) PublishAsync(ctx context.Context, event *models.Event) {
	if event == nil {
		return
	}

	// Persist to repository if configured
	if p.repo != nil {
		_ = p.repo.Create(ctx, event)
	}

	p.mu.RLock()
	for _, sub := range p.subscriptions {
		if sub.filter.Matches(event) {
			go sub.handler(event)
		}
	}
	p.mu.RUnlock()
}

// Subscribe registers a handler to receive events matching the filter.
func (p *InMemoryPublisher) Subscribe(id string, filter Filter, handler EventHandler) error {
	if id == "" {
		return ErrInvalidSubscriptionID
	}
	if handler == nil {
		return ErrNilHandler
	}

	p.mu.Lock()
	defer p.mu.Unlock()

	if _, exists := p.subscriptions[id]; exists {
		return ErrSubscriptionExists
	}

	p.subscriptions[id] = &subscription{
		id:      id,
		filter:  filter,
		handler: handler,
	}

	return nil
}

// Unsubscribe removes a subscription by ID.
func (p *InMemoryPublisher) Unsubscribe(id string) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if _, exists := p.subscriptions[id]; !exists {
		return ErrSubscriptionNotFound
	}

	delete(p.subscriptions, id)
	return nil
}

// SubscriberCount returns the number of active subscribers.
func (p *InMemoryPublisher) SubscriberCount() int {
	p.mu.RLock()
	defer p.mu.RUnlock()
	return len(p.subscriptions)
}

// UpdateSubscription updates the filter for an existing subscription.
func (p *InMemoryPublisher) UpdateSubscription(id string, filter Filter) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	sub, exists := p.subscriptions[id]
	if !exists {
		return ErrSubscriptionNotFound
	}

	sub.filter = filter
	return nil
}

// Close removes all subscriptions.
func (p *InMemoryPublisher) Close() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.subscriptions = make(map[string]*subscription)
}

// Errors for publisher operations.
var (
	ErrInvalidSubscriptionID = &PublisherError{Message: "subscription ID is required"}
	ErrNilHandler            = &PublisherError{Message: "handler cannot be nil"}
	ErrSubscriptionExists    = &PublisherError{Message: "subscription with this ID already exists"}
	ErrSubscriptionNotFound  = &PublisherError{Message: "subscription not found"}
)

// PublisherError represents an error from publisher operations.
type PublisherError struct {
	Message string
}

func (e *PublisherError) Error() string {
	return e.Message
}
