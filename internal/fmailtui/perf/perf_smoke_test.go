//go:build perf

package perf

import (
	"os"
	"strconv"
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/fmailtui/data"
)

func TestPerfSmokeBudgets(t *testing.T) {
	if testing.Short() {
		t.Skip("perf smoke skipped in -short")
	}

	projectRoot := t.TempDir()
	writeSyntheticMailbox(t, projectRoot, datasetConfig{
		topics:         200,
		topicMessages:  20,
		dmPeers:        50,
		dmMessagesEach: 20,
		agents:         30,
	})

	provider, err := data.NewFileProvider(data.FileProviderConfig{
		Root:         projectRoot,
		CacheTTL:     30 * time.Second,
		PollInterval: 250 * time.Millisecond,
		PollMax:      5 * time.Second,
		SelfAgent:    "viewer",
	})
	if err != nil {
		t.Fatalf("new file provider: %v", err)
	}

	scale := perfBudgetScale()
	coldBudget := scaleDuration(350*time.Millisecond, scale)
	refreshBudget := scaleDuration(15*time.Millisecond, scale)
	searchWarmBudget := scaleDuration(35*time.Millisecond, scale)

	coldStart := time.Now()
	if _, err := provider.Topics(); err != nil {
		t.Fatalf("Topics: %v", err)
	}
	if _, err := provider.DMConversations("viewer"); err != nil {
		t.Fatalf("DMConversations: %v", err)
	}
	if _, err := provider.Search(data.SearchQuery{Text: "needle"}); err != nil {
		t.Fatalf("Search(cold): %v", err)
	}
	coldDur := time.Since(coldStart)
	t.Logf("cold load (Topics+DMConversations+Search cold): %s (budget %s, scale %.2f)", coldDur, coldBudget, scale)
	if coldDur > coldBudget {
		t.Fatalf("cold load too slow: %s > %s (scale %.2f)", coldDur, coldBudget, scale)
	}

	// Refresh (cache hit).
	refreshStart := time.Now()
	if _, err := provider.Topics(); err != nil {
		t.Fatalf("Topics(warm): %v", err)
	}
	refreshDur := time.Since(refreshStart)
	t.Logf("refresh (Topics warm): %s (budget %s, scale %.2f)", refreshDur, refreshBudget, scale)
	if refreshDur > refreshBudget {
		t.Fatalf("refresh too slow: %s > %s (scale %.2f)", refreshDur, refreshBudget, scale)
	}

	// Search (warm index).
	searchWarmStart := time.Now()
	if _, err := provider.Search(data.SearchQuery{Text: "needle"}); err != nil {
		t.Fatalf("Search(warm): %v", err)
	}
	searchWarmDur := time.Since(searchWarmStart)
	t.Logf("search warm: %s (budget %s, scale %.2f)", searchWarmDur, searchWarmBudget, scale)
	if searchWarmDur > searchWarmBudget {
		t.Fatalf("search warm too slow: %s > %s (scale %.2f)", searchWarmDur, searchWarmBudget, scale)
	}
}

func perfBudgetScale() float64 {
	// Allows running on slower laptops/CI-like VMs without changing code:
	//   FM_PERF_BUDGET_SCALE=2 make perf-smoke
	raw := os.Getenv("FM_PERF_BUDGET_SCALE")
	if raw == "" {
		return 1
	}
	v, err := strconv.ParseFloat(raw, 64)
	if err != nil || v <= 0 {
		return 1
	}
	return v
}

func scaleDuration(d time.Duration, scale float64) time.Duration {
	if scale <= 0 {
		return d
	}
	return time.Duration(float64(d) * scale)
}

