package fmailtui

import (
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestAgentPresenceIndicator(t *testing.T) {
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	require.Equal(t, "✕", agentPresenceIndicator(now, time.Time{}))
	require.Equal(t, "●", agentPresenceIndicator(now, now.Add(-30*time.Second)))
	require.Equal(t, "○", agentPresenceIndicator(now, now.Add(-5*time.Minute)))
	require.Equal(t, "◌", agentPresenceIndicator(now, now.Add(-30*time.Minute)))
	require.Equal(t, "✕", agentPresenceIndicator(now, now.Add(-2*time.Hour)))
}

func TestRenderSparkScales(t *testing.T) {
	out := renderSpark([]int{0, 1, 2, 4, 8})
	require.Len(t, []rune(out), 5)
	require.NotEqual(t, out[0], out[len(out)-1])
}

func TestAgentsViewRebuildRowsFilterAndSort(t *testing.T) {
	v := newAgentsView("", nil)
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	v.now = now
	v.records = []fmail.AgentRecord{
		{Name: "coder-1", Host: "build", LastSeen: now.Add(-2 * time.Minute)},
		{Name: "architect", Host: "build", LastSeen: now.Add(-1 * time.Minute)},
		{Name: "reviewer", Host: "mac", LastSeen: now.Add(-30 * time.Minute)},
	}
	v.counts = map[string]int{"architect": 2, "coder-1": 5, "reviewer": 1}

	v.sortKey = agentSortName
	v.rebuildRows()
	require.Equal(t, "architect", v.rows[0].rec.Name)

	v.sortKey = agentSortMsgCount
	v.rebuildRows()
	require.Equal(t, "coder-1", v.rows[0].rec.Name)

	v.filter = "mac"
	v.rebuildRows()
	require.Len(t, v.rows, 1)
	require.Equal(t, "reviewer", v.rows[0].rec.Name)
}
