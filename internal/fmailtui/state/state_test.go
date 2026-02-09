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
