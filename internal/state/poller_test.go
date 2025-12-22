package state

import (
	"testing"
	"time"

	"github.com/opencode-ai/swarm/internal/models"
)

func TestDefaultPollerConfig(t *testing.T) {
	config := DefaultPollerConfig()

	if config.ActiveInterval <= 0 {
		t.Error("expected positive ActiveInterval")
	}
	if config.IdleInterval <= 0 {
		t.Error("expected positive IdleInterval")
	}
	if config.InactiveInterval <= 0 {
		t.Error("expected positive InactiveInterval")
	}
	if config.MaxConcurrentPolls <= 0 {
		t.Error("expected positive MaxConcurrentPolls")
	}
}

func TestPollerShouldPoll(t *testing.T) {
	config := PollerConfig{
		ActiveInterval:     100 * time.Millisecond,
		IdleInterval:       200 * time.Millisecond,
		InactiveInterval:   500 * time.Millisecond,
		MaxConcurrentPolls: 5,
	}

	p := NewPoller(config, nil, nil)
	now := time.Now()

	tests := []struct {
		name       string
		agent      *models.Agent
		lastPolled time.Time
		expect     bool
	}{
		{
			name:       "working agent never polled",
			agent:      &models.Agent{ID: "a1", State: models.AgentStateWorking},
			lastPolled: time.Time{},
			expect:     true,
		},
		{
			name:       "working agent recently polled",
			agent:      &models.Agent{ID: "a2", State: models.AgentStateWorking},
			lastPolled: now.Add(-50 * time.Millisecond),
			expect:     false,
		},
		{
			name:       "working agent poll due",
			agent:      &models.Agent{ID: "a3", State: models.AgentStateWorking},
			lastPolled: now.Add(-150 * time.Millisecond),
			expect:     true,
		},
		{
			name:       "idle agent recently polled",
			agent:      &models.Agent{ID: "a4", State: models.AgentStateIdle},
			lastPolled: now.Add(-100 * time.Millisecond),
			expect:     false,
		},
		{
			name:       "idle agent poll due",
			agent:      &models.Agent{ID: "a5", State: models.AgentStateIdle},
			lastPolled: now.Add(-250 * time.Millisecond),
			expect:     true,
		},
		{
			name:       "paused agent recently polled",
			agent:      &models.Agent{ID: "a6", State: models.AgentStatePaused},
			lastPolled: now.Add(-300 * time.Millisecond),
			expect:     false,
		},
		{
			name:       "paused agent poll due",
			agent:      &models.Agent{ID: "a7", State: models.AgentStatePaused},
			lastPolled: now.Add(-600 * time.Millisecond),
			expect:     true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Set up poll state
			if !tt.lastPolled.IsZero() {
				p.pollStates[tt.agent.ID] = &agentPollState{
					agentID:      tt.agent.ID,
					lastPolledAt: tt.lastPolled,
				}
			}

			got := p.shouldPoll(tt.agent, now)
			if got != tt.expect {
				t.Errorf("shouldPoll() = %v, want %v", got, tt.expect)
			}
		})
	}
}
