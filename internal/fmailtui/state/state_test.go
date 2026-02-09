package state

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestManager_LoadMissingFileOK(t *testing.T) {
	root := t.TempDir()
	m := New(filepath.Join(root, ".fmail", "tui-state.json"))
	require.NoError(t, m.Load())
	s := m.Snapshot()
	require.Equal(t, CurrentVersion, s.Version)
}

func TestManager_LegacyMigration(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, ".fmail", "tui-state.json")
	require.NoError(t, os.MkdirAll(filepath.Dir(path), 0o755))
	require.NoError(t, os.WriteFile(path, []byte(`{"read_markers":{"task":"20260209-080000-0001"},"starred_topics":["task"]}`), 0o644))

	m := New(path)
	require.NoError(t, m.Load())
	s := m.Snapshot()
	require.Equal(t, CurrentVersion, s.Version)
	require.Equal(t, "20260209-080000-0001", s.ReadMarkers["task"])
	require.Contains(t, s.StarredTopics, "task")
}

func TestManager_PrunesAndCapsBookmarksOnSave(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, ".fmail", "tui-state.json")
	m := New(path)

	now := time.Now().UTC()
	// 1 stale bookmark, 501 fresh => should prune stale and cap to 500.
	state := m.Snapshot()
	state.Bookmarks = append(state.Bookmarks, Bookmark{
		MessageID: "old",
		Topic:     "task",
		CreatedAt: now.Add(-(bookmarkMaxAge + time.Hour)),
	})
	for i := 0; i < 501; i++ {
		state.Bookmarks = append(state.Bookmarks, Bookmark{
			MessageID: "m",
			Topic:     "task",
			CreatedAt: now.Add(time.Duration(i) * time.Second),
		})
	}

	// Directly set internal state for this test.
	m.mu.Lock()
	m.state = state
	m.dirty = true
	m.mu.Unlock()

	require.NoError(t, m.SaveNow())
	require.NoError(t, m.Load())

	loaded := m.Snapshot()
	require.LessOrEqual(t, len(loaded.Bookmarks), maxBookmarks)
	for _, bm := range loaded.Bookmarks {
		require.NotEqual(t, "old", bm.MessageID)
	}
}

func TestManager_DraftRoundTrip(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, ".fmail", "tui-state.json")
	m := New(path)
	require.NoError(t, m.Load())

	m.SetDraft(ComposeDraft{
		Target:   "task",
		To:       "task",
		Priority: "normal",
		Tags:     "urgent,auth",
		Body:     "ship it",
	})
	require.NoError(t, m.SaveNow())

	loaded := New(path)
	require.NoError(t, loaded.Load())
	draft, ok := loaded.Draft("task")
	require.True(t, ok)
	require.Equal(t, "task", draft.To)
	require.Equal(t, "ship it", draft.Body)

	loaded.DeleteDraft("task")
	require.NoError(t, loaded.SaveNow())

	reloaded := New(path)
	require.NoError(t, reloaded.Load())
	_, ok = reloaded.Draft("task")
	require.False(t, ok)
}

func TestManager_GroupRoundTripAndNormalization(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, ".fmail", "tui-state.json")
	m := New(path)
	require.NoError(t, m.Load())

	m.SetGroup("frontend", []string{"coder-1", "@coder-1", "designer"})
	m.SetGroup("empty", nil)
	require.NoError(t, m.SaveNow())

	loaded := New(path)
	require.NoError(t, loaded.Load())
	groups := loaded.Groups()
	require.Contains(t, groups, "frontend")
	require.Equal(t, []string{"@coder-1", "@designer"}, groups["frontend"])
	require.NotContains(t, groups, "empty")
}

func TestManager_ToggleBookmarkRoundTrip(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, ".fmail", "tui-state.json")
	m := New(path)
	require.NoError(t, m.Load())

	added := m.ToggleBookmark("20260209-101010-0001", "task")
	require.True(t, added)
	require.True(t, m.IsBookmarked("20260209-101010-0001"))
	require.NoError(t, m.SaveNow())

	loaded := New(path)
	require.NoError(t, loaded.Load())
	require.True(t, loaded.IsBookmarked("20260209-101010-0001"))

	added = loaded.ToggleBookmark("20260209-101010-0001", "task")
	require.False(t, added)
	require.False(t, loaded.IsBookmarked("20260209-101010-0001"))
}

func TestManager_LayoutPreferencesRoundTripAndNormalize(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, ".fmail", "tui-state.json")
	m := New(path)
	require.NoError(t, m.Load())

	m.UpdatePreferences(func(p *Preferences) {
		p.DefaultLayout = "DASHBOARD"
		p.LayoutSplitRatio = 9
		p.LayoutSplitCollapsed = true
		p.LayoutFocus = -4
		p.LayoutExpanded = true
		p.DashboardGrid = "5x5"
		p.DashboardViews = []string{"topics", "thread", "agents", "live-tail", "overflow"}
	})
	require.NoError(t, m.SaveNow())

	reloaded := New(path)
	require.NoError(t, reloaded.Load())
	p := reloaded.Preferences()
	require.Equal(t, "dashboard", p.DefaultLayout)
	require.Equal(t, 0.8, p.LayoutSplitRatio)
	require.True(t, p.LayoutSplitCollapsed)
	require.Equal(t, 0, p.LayoutFocus)
	require.True(t, p.LayoutExpanded)
	require.Equal(t, "2x2", p.DashboardGrid)
	require.Equal(t, []string{"topics", "thread", "agents", "live-tail"}, p.DashboardViews)
}

func TestManager_ThemePreferenceRoundTrip(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, ".fmail", "tui-state.json")
	m := New(path)
	require.NoError(t, m.Load())

	require.Equal(t, "", m.Theme())
	m.SetTheme("high-contrast")
	require.NoError(t, m.SaveNow())

	loaded := New(path)
	require.NoError(t, loaded.Load())
	require.Equal(t, "high-contrast", loaded.Theme())
	require.Equal(t, "high-contrast", loaded.Preferences().Theme)
}
