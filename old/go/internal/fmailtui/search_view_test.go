package fmailtui

import (
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
)

func TestSearchViewRenderResultsShowsPriorityAndStateBadges(t *testing.T) {
	now := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	st := tuistate.New(t.TempDir() + "/tui-state.json")
	st.SetReadMarker("task", "20260209-100000-0001")
	st.UpsertBookmark("20260209-110000-0001", "task", "important")
	st.SetAnnotation("20260209-110000-0001", "follow up")

	v := newSearchView(t.TempDir(), "viewer", nil, st)
	v.now = now
	v.query = "token"
	v.selected = 0
	v.results = []data.SearchResult{
		{
			Topic: "task",
			Message: fmail.Message{
				ID:       "20260209-110000-0001",
				From:     "alice",
				To:       "task",
				Time:     now,
				Priority: fmail.PriorityHigh,
				Body:     "token refresh plan",
			},
		},
	}

	rendered := v.renderResults(120, 20, themePalette(ThemeDefault))
	require.True(t, strings.Contains(rendered, "[HIGH]"))
	require.True(t, strings.Contains(rendered, "★"))
	require.True(t, strings.Contains(rendered, "✎"))
	require.True(t, strings.Contains(rendered, "●"))
	require.True(t, strings.Contains(rendered, "→"))
}

func TestSearchViewRenderStatsShowsSelectedPosition(t *testing.T) {
	v := newSearchView(t.TempDir(), "viewer", nil, nil)
	v.results = []data.SearchResult{
		{Topic: "task", Message: fmail.Message{ID: "1"}},
		{Topic: "task", Message: fmail.Message{ID: "2"}},
	}
	v.selected = 1

	stats := v.renderStatsLine(120, themePalette(ThemeDefault))
	require.Contains(t, stats, "selected:2/2")
}
