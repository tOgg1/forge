package hooks

import (
	"strings"
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/models"
)

func TestEventEnv_ForgeOnly(t *testing.T) {
	event := &models.Event{
		ID:         "evt-1",
		Timestamp:  time.Date(2026, 2, 10, 6, 0, 0, 0, time.UTC),
		Type:       models.EventTypeAgentSpawned,
		EntityType: models.EntityTypeAgent,
		EntityID:   "loop-1",
		Payload:    []byte(`{"ok":true}`),
	}

	env := eventEnv(event)
	joined := strings.Join(env, "\n")

	if !strings.Contains(joined, "FORGE_EVENT_ID=evt-1") {
		t.Fatalf("expected FORGE_EVENT_ID in env, got %v", env)
	}
	if !strings.Contains(joined, "FORGE_EVENT_TYPE="+string(models.EventTypeAgentSpawned)) {
		t.Fatalf("expected FORGE_EVENT_TYPE in env, got %v", env)
	}
	if strings.Contains(joined, "SWARM_") {
		t.Fatalf("expected no legacy SWARM_* entries, got %v", env)
	}
}
