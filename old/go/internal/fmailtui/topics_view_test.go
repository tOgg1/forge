package fmailtui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
)

type stubTopicsProvider struct {
	topics []data.TopicInfo
	dms    []data.DMConversation

	byTopic map[string][]fmail.Message
	byDM    map[string][]fmail.Message

	topicsCalls int
	dmsCalls    int
	topicCalls  map[string]int
	dmCalls     map[string]int
}

func (s *stubTopicsProvider) Topics() ([]data.TopicInfo, error) {
	s.topicsCalls++
	out := make([]data.TopicInfo, len(s.topics))
	copy(out, s.topics)
	return out, nil
}

func (s *stubTopicsProvider) Messages(topic string, opts data.MessageFilter) ([]fmail.Message, error) {
	if s.topicCalls == nil {
		s.topicCalls = make(map[string]int)
	}
	s.topicCalls[topic]++

	msgs := append([]fmail.Message(nil), s.byTopic[topic]...)
	if opts.Limit > 0 && len(msgs) > opts.Limit {
		msgs = msgs[len(msgs)-opts.Limit:]
	}
	return msgs, nil
}

func (s *stubTopicsProvider) DMConversations(string) ([]data.DMConversation, error) {
	s.dmsCalls++
	out := make([]data.DMConversation, len(s.dms))
	copy(out, s.dms)
	return out, nil
}

func (s *stubTopicsProvider) DMs(agent string, opts data.MessageFilter) ([]fmail.Message, error) {
	if s.dmCalls == nil {
		s.dmCalls = make(map[string]int)
	}
	s.dmCalls[agent]++

	msgs := append([]fmail.Message(nil), s.byDM[agent]...)
	if opts.Limit > 0 && len(msgs) > opts.Limit {
		msgs = msgs[len(msgs)-opts.Limit:]
	}
	return msgs, nil
}

func (s *stubTopicsProvider) Agents() ([]fmail.AgentRecord, error) { return nil, nil }

func (s *stubTopicsProvider) Search(data.SearchQuery) ([]data.SearchResult, error) {
	return nil, nil
}

func (s *stubTopicsProvider) Subscribe(data.SubscriptionFilter) (<-chan fmail.Message, func()) {
	ch := make(chan fmail.Message)
	return ch, func() { close(ch) }
}

func TestTopicsViewRebuildItemsHonorsStarFilterAndSort(t *testing.T) {
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	v := newTopicsView(t.TempDir(), &stubTopicsProvider{}, nil)
	v.now = now
	v.topics = []data.TopicInfo{
		{Name: "task", MessageCount: 5, LastActivity: now.Add(-10 * time.Minute), Participants: []string{"a", "b"}},
		{Name: "build", MessageCount: 7, LastActivity: now.Add(-2 * time.Minute), Participants: []string{"c"}},
		{Name: "review", MessageCount: 3, LastActivity: now.Add(-30 * time.Minute), Participants: []string{"a", "d"}},
	}
	v.starred["review"] = true
	v.unreadByTop["task"] = 2

	v.rebuildItems()
	require.Equal(t, "review", v.items[0].label) // starred always on top

	v.filter = "ta"
	v.rebuildItems()
	require.Len(t, v.items, 1)
	require.Equal(t, "task", v.items[0].label)

	v.filter = ""
	v.sortKey = topicSortCount
	v.rebuildItems()
	require.Equal(t, "review", v.items[0].label) // still starred
	require.Equal(t, "build", v.items[1].label)
}

func TestTopicsViewApplyLoadedComputesUnreadFromReadMarkers(t *testing.T) {
	now := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	provider := &stubTopicsProvider{
		topics: []data.TopicInfo{
			{Name: "task", MessageCount: 2, LastActivity: now},
		},
		dms: []data.DMConversation{
			{Agent: "bob", MessageCount: 2, LastActivity: now},
		},
		byTopic: map[string][]fmail.Message{
			"task": {
				{ID: "20260209-100000-0001", From: "alice", To: "task", Time: now.Add(-2 * time.Minute), Body: "old"},
				{ID: "20260209-101000-0001", From: "alice", To: "task", Time: now.Add(-time.Minute), Body: "new"},
			},
		},
		byDM: map[string][]fmail.Message{
			"bob": {
				{ID: "20260209-100000-0001", From: "bob", To: "@viewer", Time: now.Add(-2 * time.Minute), Body: "old"},
				{ID: "20260209-101000-0001", From: "bob", To: "@viewer", Time: now.Add(-time.Minute), Body: "new"},
			},
		},
	}

	v := newTopicsView(t.TempDir(), provider, nil)
	v.self = "viewer"
	v.readMarkers = map[string]string{
		"task": "20260209-100000-0001",
		"@bob": "20260209-100000-0001",
	}

	msg, ok := v.loadCmd()().(topicsLoadedMsg)
	require.True(t, ok)
	require.NoError(t, msg.err)
	v.applyLoaded(msg)
	require.Equal(t, 1, v.unreadByTop["task"])
	require.Equal(t, 1, v.unreadByDM["bob"])
}

func TestTopicsViewApplyLoadedDefaultsUnreadToAllWithoutMarker(t *testing.T) {
	now := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	provider := &stubTopicsProvider{
		topics: []data.TopicInfo{
			{Name: "task", MessageCount: 2, LastActivity: now},
		},
		dms: []data.DMConversation{
			{Agent: "bob", MessageCount: 2, LastActivity: now},
		},
		byTopic: map[string][]fmail.Message{
			"task": {
				{ID: "20260209-100000-0001", From: "alice", To: "task", Time: now.Add(-2 * time.Minute), Body: "old"},
				{ID: "20260209-101000-0001", From: "alice", To: "task", Time: now.Add(-time.Minute), Body: "new"},
			},
		},
		byDM: map[string][]fmail.Message{
			"bob": {
				{ID: "20260209-100000-0001", From: "bob", To: "@viewer", Time: now.Add(-2 * time.Minute), Body: "old"},
				{ID: "20260209-101000-0001", From: "bob", To: "@viewer", Time: now.Add(-time.Minute), Body: "new"},
			},
		},
	}

	v := newTopicsView(t.TempDir(), provider, nil)
	v.self = "viewer"

	msg, ok := v.loadCmd()().(topicsLoadedMsg)
	require.True(t, ok)
	require.NoError(t, msg.err)
	v.applyLoaded(msg)
	require.Equal(t, 2, v.unreadByTop["task"])
	require.Equal(t, 2, v.unreadByDM["bob"])
}

func TestTopicsViewApplyLoadedSkipsFullUnreadRescanWhenCountsUnchanged(t *testing.T) {
	now := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	provider := &stubTopicsProvider{
		topics: []data.TopicInfo{{Name: "task", MessageCount: 2, LastActivity: now}},
		dms:    []data.DMConversation{{Agent: "bob", MessageCount: 2, LastActivity: now}},
		byTopic: map[string][]fmail.Message{
			"task": {
				{ID: "20260209-100000-0001", From: "alice", To: "task", Time: now.Add(-2 * time.Minute), Body: "old"},
				{ID: "20260209-101000-0001", From: "alice", To: "task", Time: now.Add(-time.Minute), Body: "new"},
			},
		},
		byDM: map[string][]fmail.Message{
			"bob": {
				{ID: "20260209-100000-0001", From: "bob", To: "@viewer", Time: now.Add(-2 * time.Minute), Body: "old"},
				{ID: "20260209-101000-0001", From: "bob", To: "@viewer", Time: now.Add(-time.Minute), Body: "new"},
			},
		},
	}
	v := newTopicsView(t.TempDir(), provider, nil)
	v.self = "viewer"

	first, ok := v.loadCmd()().(topicsLoadedMsg)
	require.True(t, ok)
	v.applyLoaded(first)

	provider.topicCalls = make(map[string]int)
	provider.dmCalls = make(map[string]int)

	second, ok := v.loadCmd()().(topicsLoadedMsg)
	require.True(t, ok)
	v.applyLoaded(second)

	require.Empty(t, provider.topicCalls)
	require.Empty(t, provider.dmCalls)
	require.Equal(t, 2, v.unreadByTop["task"])
	require.Equal(t, 2, v.unreadByDM["bob"])
}

func TestTopicsViewApplyIncomingUpdatesUnreadIncrementally(t *testing.T) {
	now := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	v := newTopicsView(t.TempDir(), &stubTopicsProvider{}, nil)
	v.self = "viewer"
	v.topics = []data.TopicInfo{{Name: "task", MessageCount: 1, LastActivity: now.Add(-time.Minute), Participants: []string{"alice"}}}
	v.dms = []data.DMConversation{{Agent: "bob", MessageCount: 1, LastActivity: now.Add(-time.Minute)}}
	v.unreadByTop["task"] = 1
	v.unreadByDM["bob"] = 1
	v.readMarkers = map[string]string{
		"task": "20260209-100500-0001",
		"@bob": "20260209-100500-0001",
	}

	v.applyIncoming(fmail.Message{
		ID:   "20260209-101500-0001",
		From: "alice",
		To:   "task",
		Time: now,
		Body: "new topic msg",
	})
	require.Equal(t, 2, v.unreadByTop["task"])
	require.Equal(t, 2, v.topics[0].MessageCount)

	v.applyIncoming(fmail.Message{
		ID:   "20260209-101600-0001",
		From: "bob",
		To:   "@viewer",
		Time: now.Add(30 * time.Second),
		Body: "new dm msg",
	})
	require.Equal(t, 2, v.unreadByDM["bob"])
	require.Equal(t, 2, v.dms[0].MessageCount)
}

func TestTopicsViewShouldRefreshDebouncesTicks(t *testing.T) {
	v := newTopicsView(t.TempDir(), &stubTopicsProvider{}, nil)
	base := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	v.lastLoad = base

	require.False(t, v.shouldRefresh(base.Add(5*time.Second)))
	require.True(t, v.shouldRefresh(base.Add(topicsMetadataRefresh)))
}

func TestTopicsViewRenderListPanelShowsKeyLegendAndFilteredEmptyState(t *testing.T) {
	now := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	v := newTopicsView(t.TempDir(), &stubTopicsProvider{}, nil)
	v.now = now
	v.topics = []data.TopicInfo{
		{Name: "task", MessageCount: 2, LastActivity: now},
	}
	v.filter = "zzz"
	v.rebuildItems()

	rendered := v.renderListPanel(96, 14, themePalette(ThemeDefault))
	require.True(t, strings.Contains(rendered, "j/k move"))
	require.True(t, strings.Contains(rendered, "No matches for"))
}

func TestChangedMarkerKeysReturnsOnlyChangedKeys(t *testing.T) {
	prev := map[string]string{
		"task": "a",
		"@bob": "b",
	}
	next := map[string]string{
		"task": "a",
		"@bob": "c",
	}
	changed := changedMarkerKeys(prev, next)
	require.Len(t, changed, 1)
	require.Equal(t, "@bob", changed[0])
}

func TestTopicsViewRecomputeUnreadTargetsCmdOnlyScansSelectedTargets(t *testing.T) {
	now := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	provider := &stubTopicsProvider{
		byTopic: map[string][]fmail.Message{
			"task": {
				{ID: "20260209-100000-0001", From: "alice", To: "task", Time: now.Add(-2 * time.Minute), Body: "old"},
				{ID: "20260209-101000-0001", From: "alice", To: "task", Time: now.Add(-time.Minute), Body: "new"},
			},
			"build": {
				{ID: "20260209-100000-0002", From: "alice", To: "build", Time: now.Add(-2 * time.Minute), Body: "old"},
				{ID: "20260209-101000-0002", From: "alice", To: "build", Time: now.Add(-time.Minute), Body: "new"},
			},
		},
		byDM: map[string][]fmail.Message{
			"bob": {
				{ID: "20260209-100000-0001", From: "bob", To: "@viewer", Time: now.Add(-2 * time.Minute), Body: "old"},
				{ID: "20260209-101000-0001", From: "bob", To: "@viewer", Time: now.Add(-time.Minute), Body: "new"},
			},
			"cara": {
				{ID: "20260209-100000-0001", From: "cara", To: "@viewer", Time: now.Add(-2 * time.Minute), Body: "old"},
				{ID: "20260209-101000-0001", From: "cara", To: "@viewer", Time: now.Add(-time.Minute), Body: "new"},
			},
		},
	}

	v := newTopicsView(t.TempDir(), provider, nil)
	v.self = "viewer"
	v.topics = []data.TopicInfo{
		{Name: "task", MessageCount: 2, LastActivity: now},
		{Name: "build", MessageCount: 2, LastActivity: now},
	}
	v.dms = []data.DMConversation{
		{Agent: "bob", MessageCount: 2, LastActivity: now},
		{Agent: "cara", MessageCount: 2, LastActivity: now},
	}
	v.readMarkers = map[string]string{
		"task": "20260209-100000-0001",
		"@bob": "20260209-100000-0001",
	}

	cmd := v.recomputeUnreadTargetsCmd([]string{"task", "@bob"})
	require.NotNil(t, cmd)
	msg, ok := cmd().(topicsUnreadSnapshotMsg)
	require.True(t, ok)
	require.NoError(t, msg.err)
	require.Equal(t, 1, msg.unreadByTop["task"])
	require.Equal(t, 1, msg.unreadByDM["bob"])

	require.Equal(t, 1, provider.topicCalls["task"])
	require.Equal(t, 0, provider.topicCalls["build"])
	require.Equal(t, 1, provider.dmCalls["bob"])
	require.Equal(t, 0, provider.dmCalls["cara"])
}

func TestTopicsViewPreviewLoadsLazilyAndCaches(t *testing.T) {
	now := time.Date(2026, 2, 9, 12, 0, 0, 0, time.UTC)
	provider := &stubTopicsProvider{
		byTopic: map[string][]fmail.Message{
			"task": {
				{ID: "20260209-120000-0001", From: "alice", To: "task", Time: now, Body: "hello"},
			},
			"build": {
				{ID: "20260209-120100-0001", From: "bob", To: "build", Time: now, Body: "world"},
			},
		},
	}

	v := newTopicsView(t.TempDir(), provider, nil)
	v.topics = []data.TopicInfo{
		{Name: "task", LastActivity: now},
		{Name: "build", LastActivity: now.Add(-time.Minute)},
	}
	v.rebuildItems()

	cmd := v.ensurePreviewCmd()
	require.NotNil(t, cmd)
	previewMsg, ok := cmd().(topicsPreviewLoadedMsg)
	require.True(t, ok)
	v.applyPreview(previewMsg)
	require.Equal(t, 1, provider.topicCalls["task"])

	require.Nil(t, v.ensurePreviewCmd())
	require.Equal(t, 1, provider.topicCalls["task"])

	v.moveSelection(1)
	cmd = v.ensurePreviewCmd()
	require.NotNil(t, cmd)
	previewMsg, ok = cmd().(topicsPreviewLoadedMsg)
	require.True(t, ok)
	v.applyPreview(previewMsg)
	require.Equal(t, 1, provider.topicCalls["build"])
}

func TestTopicsViewStarTogglePersistsToStateFile(t *testing.T) {
	root := t.TempDir()
	statePath := filepath.Join(root, ".fmail", "tui-state.json")
	st := tuistate.New(statePath)
	st.SetReadMarker("task", "20260209-100000-0001")
	st.SetStarredTopics([]string{"alerts"})
	require.NoError(t, st.SaveNow())

	v := newTopicsView(root, &stubTopicsProvider{}, st)
	v.loadState()
	require.True(t, v.starred["alerts"])

	v.topics = []data.TopicInfo{{Name: "task", LastActivity: time.Now().UTC()}}
	v.rebuildItems()

	cmd := v.handleKey(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'*'}})
	require.Nil(t, cmd)
	require.True(t, v.starred["task"])

	require.NoError(t, st.SaveNow())
	st2 := tuistate.New(statePath)
	require.NoError(t, st2.Load())
	snap := st2.Snapshot()
	require.Contains(t, snap.StarredTopics, "task")
	require.Equal(t, "20260209-100000-0001", snap.ReadMarkers["task"])
}

func TestTopicsViewComposeWritesMessageAndMarksRead(t *testing.T) {
	root := t.TempDir()
	statePath := filepath.Join(root, ".fmail", "tui-state.json")
	st := tuistate.New(statePath)
	require.NoError(t, st.Load())

	v := newTopicsView(root, &stubTopicsProvider{}, st)
	v.self = "viewer"
	v.items = []topicsItem{{target: "task", label: "task"}}
	v.selected = 0

	require.Nil(t, v.handleKey(runeKey('n')))
	require.True(t, v.composeActive)

	require.Nil(t, v.handleKey(runeKey('h')))
	require.Nil(t, v.handleKey(runeKey('i')))

	cmd := v.handleKey(tea.KeyMsg{Type: tea.KeyEnter})
	require.NotNil(t, cmd)
	sent, ok := cmd().(topicsSentMsg)
	require.True(t, ok)
	require.NoError(t, sent.err)
	require.NotEmpty(t, sent.msg.ID)

	require.Nil(t, v.Update(sent))
	require.False(t, v.composeActive)

	entries, err := os.ReadDir(filepath.Join(root, ".fmail", "topics", "task"))
	require.NoError(t, err)
	require.NotEmpty(t, entries)

	require.Equal(t, sent.msg.ID, st.ReadMarker("task"))
}
