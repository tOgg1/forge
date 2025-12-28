package adapters

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/models"
)

func TestNewOpenCodeEventWatcher(t *testing.T) {
	t.Run("default config", func(t *testing.T) {
		config := DefaultEventWatcherConfig()
		watcher := NewOpenCodeEventWatcher(config, nil)
		if watcher == nil {
			t.Fatal("expected non-nil watcher")
		}
		if len(watcher.WatchedAgents()) != 0 {
			t.Errorf("expected 0 watched agents, got %d", len(watcher.WatchedAgents()))
		}
	})

	t.Run("zero values get defaults", func(t *testing.T) {
		watcher := NewOpenCodeEventWatcher(EventWatcherConfig{}, nil)
		if watcher == nil {
			t.Fatal("expected non-nil watcher")
		}
		if watcher.config.ReconnectDelay != 1*time.Second {
			t.Errorf("expected 1s reconnect delay, got %v", watcher.config.ReconnectDelay)
		}
		if watcher.config.MaxReconnectDelay != 30*time.Second {
			t.Errorf("expected 30s max reconnect delay, got %v", watcher.config.MaxReconnectDelay)
		}
	})
}

func TestOpenCodeEventWatcher_Watch(t *testing.T) {
	t.Run("validates agent ID", func(t *testing.T) {
		watcher := NewOpenCodeEventWatcher(DefaultEventWatcherConfig(), nil)
		err := watcher.Watch(context.Background(), "", "http://localhost:8080/event")
		if err == nil {
			t.Error("expected error for empty agent ID")
		}
	})

	t.Run("validates events URL", func(t *testing.T) {
		watcher := NewOpenCodeEventWatcher(DefaultEventWatcherConfig(), nil)
		err := watcher.Watch(context.Background(), "agent-1", "")
		if err == nil {
			t.Error("expected error for empty events URL")
		}
	})

	t.Run("prevents duplicate watches", func(t *testing.T) {
		watcher := NewOpenCodeEventWatcher(DefaultEventWatcherConfig(), nil)
		ctx, cancel := context.WithCancel(context.Background())
		defer cancel()

		// First watch should succeed (even if connection fails)
		err := watcher.Watch(ctx, "agent-1", "http://localhost:9999/event")
		if err != nil {
			t.Fatalf("first watch failed: %v", err)
		}

		// Second watch for same agent should fail
		err = watcher.Watch(ctx, "agent-1", "http://localhost:9999/event")
		if err == nil {
			t.Error("expected error for duplicate watch")
		}

		watcher.UnwatchAll()
	})
}

func TestOpenCodeEventWatcher_Unwatch(t *testing.T) {
	t.Run("unwatch unknown agent", func(t *testing.T) {
		watcher := NewOpenCodeEventWatcher(DefaultEventWatcherConfig(), nil)
		err := watcher.Unwatch("unknown-agent")
		if err == nil {
			t.Error("expected error for unknown agent")
		}
	})
}

func TestOpenCodeEventWatcher_IsWatching(t *testing.T) {
	watcher := NewOpenCodeEventWatcher(DefaultEventWatcherConfig(), nil)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	if watcher.IsWatching("agent-1") {
		t.Error("should not be watching before Watch()")
	}

	_ = watcher.Watch(ctx, "agent-1", "http://localhost:9999/event")

	if !watcher.IsWatching("agent-1") {
		t.Error("should be watching after Watch()")
	}

	watcher.UnwatchAll()

	if watcher.IsWatching("agent-1") {
		t.Error("should not be watching after UnwatchAll()")
	}
}

func TestOpenCodeEventWatcher_StateMapping(t *testing.T) {
	tests := []struct {
		eventType     string
		expectedState models.AgentState
		shouldMap     bool
	}{
		{OpenCodeEventSessionIdle, models.AgentStateIdle, true},
		{OpenCodeEventSessionBusy, models.AgentStateWorking, true},
		{OpenCodeEventToolStart, models.AgentStateWorking, true},
		{OpenCodeEventPermission, models.AgentStateAwaitingApproval, true},
		{OpenCodeEventPermissionDone, models.AgentStateWorking, true},
		{OpenCodeEventError, models.AgentStateError, true},
		{OpenCodeEventToolEnd, "", false},
		{OpenCodeEventHeartbeat, "", false},
		{OpenCodeEventTokenUsage, "", false},
		{"unknown.event", "", false},
	}

	watcher := NewOpenCodeEventWatcher(DefaultEventWatcherConfig(), nil)

	for _, tt := range tests {
		t.Run(tt.eventType, func(t *testing.T) {
			event := OpenCodeEvent{Type: tt.eventType}
			state, _, ok := watcher.mapEventToState(event)

			if ok != tt.shouldMap {
				t.Errorf("shouldMap: expected %v, got %v", tt.shouldMap, ok)
			}
			if tt.shouldMap && state != tt.expectedState {
				t.Errorf("state: expected %s, got %s", tt.expectedState, state)
			}
		})
	}
}

func TestOpenCodeEventWatcher_SSEStream(t *testing.T) {
	t.Run("receives and maps events", func(t *testing.T) {
		var mu sync.Mutex
		states := make([]models.AgentState, 0)

		onState := func(agentID string, state models.AgentState, info models.StateInfo) {
			mu.Lock()
			states = append(states, state)
			mu.Unlock()
		}

		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			w.Header().Set("Cache-Control", "no-cache")
			w.WriteHeader(http.StatusOK)

			flusher, ok := w.(http.Flusher)
			if !ok {
				t.Error("expected Flusher support")
				return
			}

			// Send a sequence of events
			events := []struct {
				eventType string
				data      string
			}{
				{"session.busy", `{"session_id":"s1"}`},
				{"session.idle", `{"session_id":"s1"}`},
				{"permission.requested", `{"session_id":"s1"}`},
			}

			for _, e := range events {
				fmt.Fprintf(w, "event:%s\ndata:%s\n\n", e.eventType, e.data)
				flusher.Flush()
				time.Sleep(10 * time.Millisecond)
			}
		}))
		defer server.Close()

		config := DefaultEventWatcherConfig()
		config.HTTPClient = server.Client()
		watcher := NewOpenCodeEventWatcher(config, onState)

		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancel()

		err := watcher.Watch(ctx, "test-agent", server.URL)
		if err != nil {
			t.Fatalf("Watch failed: %v", err)
		}

		// Wait for events to be processed
		time.Sleep(200 * time.Millisecond)
		watcher.UnwatchAll()

		mu.Lock()
		defer mu.Unlock()

		if len(states) < 3 {
			t.Errorf("expected at least 3 states, got %d: %v", len(states), states)
		}
		if len(states) >= 3 {
			if states[0] != models.AgentStateWorking {
				t.Errorf("expected first state to be working, got %s", states[0])
			}
			if states[1] != models.AgentStateIdle {
				t.Errorf("expected second state to be idle, got %s", states[1])
			}
			if states[2] != models.AgentStateAwaitingApproval {
				t.Errorf("expected third state to be awaiting_approval, got %s", states[2])
			}
		}
	})

	t.Run("calls event handler", func(t *testing.T) {
		var mu sync.Mutex
		events := make([]OpenCodeEvent, 0)

		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			w.WriteHeader(http.StatusOK)

			flusher, _ := w.(http.Flusher)
			fmt.Fprint(w, "event:heartbeat\ndata:{\"ts\":123}\n\n")
			flusher.Flush()
		}))
		defer server.Close()

		config := DefaultEventWatcherConfig()
		config.HTTPClient = server.Client()
		watcher := NewOpenCodeEventWatcher(config, nil)
		watcher.SetEventHandler(func(agentID string, event OpenCodeEvent) {
			mu.Lock()
			events = append(events, event)
			mu.Unlock()
		})

		ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
		defer cancel()

		_ = watcher.Watch(ctx, "test-agent", server.URL)
		time.Sleep(100 * time.Millisecond)
		watcher.UnwatchAll()

		mu.Lock()
		defer mu.Unlock()

		if len(events) == 0 {
			t.Error("expected at least one event")
		}
		if len(events) > 0 && events[0].Type != "heartbeat" {
			t.Errorf("expected heartbeat event, got %s", events[0].Type)
		}
	})
}

func TestOpenCodeEventWatcher_WatchAgent(t *testing.T) {
	t.Run("nil agent", func(t *testing.T) {
		watcher := NewOpenCodeEventWatcher(DefaultEventWatcherConfig(), nil)
		err := watcher.WatchAgent(context.Background(), nil)
		if err == nil {
			t.Error("expected error for nil agent")
		}
	})

	t.Run("agent without OpenCode connection", func(t *testing.T) {
		watcher := NewOpenCodeEventWatcher(DefaultEventWatcherConfig(), nil)
		agent := &models.Agent{
			ID:   "agent-1",
			Type: models.AgentTypeClaudeCode,
		}
		err := watcher.WatchAgent(context.Background(), agent)
		if err == nil {
			t.Error("expected error for agent without OpenCode connection")
		}
	})

	t.Run("agent with valid OpenCode connection", func(t *testing.T) {
		watcher := NewOpenCodeEventWatcher(DefaultEventWatcherConfig(), nil)
		ctx, cancel := context.WithCancel(context.Background())
		defer cancel()

		agent := &models.Agent{
			ID:   "agent-1",
			Type: models.AgentTypeOpenCode,
			Metadata: models.AgentMetadata{
				OpenCode: &models.OpenCodeConnection{
					Host: "localhost",
					Port: 17001,
				},
			},
		}

		err := watcher.WatchAgent(ctx, agent)
		if err != nil {
			t.Errorf("unexpected error: %v", err)
		}

		if !watcher.IsWatching("agent-1") {
			t.Error("expected agent to be watched")
		}

		watcher.UnwatchAll()
	})
}

func TestOpenCodeEventWatcher_Reconnection(t *testing.T) {
	t.Run("reconnects after server closes connection", func(t *testing.T) {
		connectCount := 0
		var mu sync.Mutex

		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			mu.Lock()
			connectCount++
			count := connectCount
			mu.Unlock()

			w.Header().Set("Content-Type", "text/event-stream")
			w.WriteHeader(http.StatusOK)

			flusher, _ := w.(http.Flusher)
			fmt.Fprint(w, "event:heartbeat\ndata:{}\n\n")
			flusher.Flush()

			// First two connections close immediately to test reconnection
			if count <= 2 {
				return
			}

			// Third connection stays open until context done
			<-r.Context().Done()
		}))
		defer server.Close()

		config := EventWatcherConfig{
			ReconnectDelay:         5 * time.Millisecond,
			MaxReconnectDelay:      20 * time.Millisecond,
			ReconnectBackoffFactor: 1.2,
			HTTPClient:             server.Client(),
		}
		watcher := NewOpenCodeEventWatcher(config, nil)

		ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
		defer cancel()

		_ = watcher.Watch(ctx, "test-agent", server.URL)

		// Wait for reconnections to happen
		time.Sleep(200 * time.Millisecond)

		watcher.UnwatchAll()

		mu.Lock()
		defer mu.Unlock()

		// Should have at least 2 connections due to reconnection
		if connectCount < 2 {
			t.Errorf("expected at least 2 connections (reconnect), got %d", connectCount)
		}
	})
}
