package fmailtui

import (
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
)

type stubTimelineProvider struct {
	topics  []data.TopicInfo
	dms     []data.DMConversation
	byTopic map[string][]fmail.Message
	byDM    map[string][]fmail.Message
}

func (s *stubTimelineProvider) Topics() ([]data.TopicInfo, error) {
	out := make([]data.TopicInfo, len(s.topics))
	copy(out, s.topics)
	return out, nil
}

func (s *stubTimelineProvider) Messages(topic string, opts data.MessageFilter) ([]fmail.Message, error) {
	msgs := append([]fmail.Message(nil), s.byTopic[topic]...)
	if opts.Limit > 0 && len(msgs) > opts.Limit {
		msgs = msgs[len(msgs)-opts.Limit:]
	}
	return msgs, nil
}

func (s *stubTimelineProvider) DMConversations(string) ([]data.DMConversation, error) {
	out := make([]data.DMConversation, len(s.dms))
	copy(out, s.dms)
	return out, nil
}

func (s *stubTimelineProvider) DMs(agent string, opts data.MessageFilter) ([]fmail.Message, error) {
	msgs := append([]fmail.Message(nil), s.byDM[agent]...)
	if opts.Limit > 0 && len(msgs) > opts.Limit {
		msgs = msgs[len(msgs)-opts.Limit:]
	}
	return msgs, nil
}

func (s *stubTimelineProvider) Agents() ([]fmail.AgentRecord, error) { return nil, nil }

func (s *stubTimelineProvider) Search(data.SearchQuery) ([]data.SearchResult, error) {
	return nil, nil
}

func (s *stubTimelineProvider) Subscribe(data.SubscriptionFilter) (<-chan fmail.Message, func()) {
	ch := make(chan fmail.Message)
	return ch, func() { close(ch) }
}

func TestTimelineLoadMergesTopicsAndDMsChronologically(t *testing.T) {
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	provider := &stubTimelineProvider{
		topics: []data.TopicInfo{
			{Name: "task", LastActivity: now},
		},
		dms: []data.DMConversation{
			{Agent: "bob", LastActivity: now},
		},
		byTopic: map[string][]fmail.Message{
			"task": {
				{ID: "20260209-095500-0001", From: "alice", To: "task", Time: now.Add(-5 * time.Minute), Body: "topic"},
			},
		},
		byDM: map[string][]fmail.Message{
			"bob": {
				{ID: "20260209-095700-0001", From: "bob", To: "@viewer", Time: now.Add(-3 * time.Minute), Body: "dm"},
			},
		},
	}

	v := newTimelineView(t.TempDir(), "viewer", provider, nil)
	msg, ok := v.loadCmd()().(timelineLoadedMsg)
	require.True(t, ok)
	require.NoError(t, msg.err)
	v.applyLoaded(msg)

	require.Len(t, v.all, 2)
	require.Equal(t, "20260209-095500-0001", v.all[0].ID)
	require.Equal(t, "20260209-095700-0001", v.all[1].ID)
}

func TestTimelineChronologicalRendersGapAndReplyMarker(t *testing.T) {
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	v := newTimelineView(t.TempDir(), "viewer", nil, nil)
	v.now = now
	v.all = []fmail.Message{
		{ID: "20260209-095000-0001", From: "alice", To: "task", Time: now.Add(-10 * time.Minute), Body: "root"},
		{ID: "20260209-095700-0001", From: "bob", To: "task", Time: now.Add(-3 * time.Minute), Body: "child", ReplyTo: "20260209-095000-0001"},
	}
	v.zoomIdx = 3
	v.windowEnd = now
	v.rebuildReplyIndex()
	v.rebuildVisible()

	lines := v.renderChronological(100, 20, themePalette(ThemeDefault))
	joined := ""
	for _, line := range lines {
		joined += line + "\n"
	}
	require.Contains(t, joined, "gap")
	require.Contains(t, joined, "│")
}

func TestTimelineFilterAndJumpParsing(t *testing.T) {
	filter := parseTimelineFilter("from:alice tag:auth since:1h has:reply hello world")
	require.Equal(t, "alice", filter.From)
	require.Equal(t, []string{"auth"}, filter.Tags)
	require.Equal(t, time.Hour, filter.Since)
	require.True(t, filter.HasReply)
	require.Equal(t, "hello world", filter.Text)

	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	jump, ok := parseTimelineJump("2026-02-09 09:30", now)
	require.True(t, ok)
	require.Equal(t, 9, jump.Hour())
	require.Equal(t, 30, jump.Minute())
}

func TestTimelineDetailOpenAndOpenThreadCommand(t *testing.T) {
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	v := newTimelineView(t.TempDir(), "viewer", nil, nil)
	v.now = now
	v.all = []fmail.Message{
		{ID: "20260209-095500-0001", From: "alice", To: "task", Time: now.Add(-5 * time.Minute), Body: "topic"},
	}
	v.windowEnd = now
	v.rebuildReplyIndex()
	v.rebuildVisible()

	require.Nil(t, v.handleKey(tea.KeyMsg{Type: tea.KeyEnter}))
	require.True(t, v.detailOpen)

	cmd := v.handleKey(runeKey('o'))
	require.NotNil(t, cmd)
	batch, ok := cmd().(tea.BatchMsg)
	require.True(t, ok)
	require.Len(t, batch, 2)
}

func TestTimelineBookmarkToggleUsesStateManager(t *testing.T) {
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	st := tuistate.New(t.TempDir() + "/tui-state.json")
	require.NoError(t, st.Load())

	v := newTimelineView(t.TempDir(), "viewer", nil, st)
	v.now = now
	v.all = []fmail.Message{
		{ID: "20260209-095500-0001", From: "alice", To: "task", Time: now.Add(-5 * time.Minute), Body: "topic"},
	}
	v.windowEnd = now
	v.rebuildReplyIndex()
	v.rebuildVisible()

	require.Nil(t, v.handleKey(runeKey('b')))
	require.True(t, st.IsBookmarked("20260209-095500-0001"))
}
