package fmailtui

import (
	"fmt"
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

	messageCalls int
	dmCalls      int
	messageOpts  []data.MessageFilter
	dmOpts       []data.MessageFilter
}

func (s *stubTimelineProvider) Topics() ([]data.TopicInfo, error) {
	out := make([]data.TopicInfo, len(s.topics))
	copy(out, s.topics)
	return out, nil
}

func (s *stubTimelineProvider) Messages(topic string, opts data.MessageFilter) ([]fmail.Message, error) {
	s.messageCalls++
	s.messageOpts = append(s.messageOpts, opts)
	return filterTimelineMessages(s.byTopic[topic], opts), nil
}

func (s *stubTimelineProvider) DMConversations(string) ([]data.DMConversation, error) {
	out := make([]data.DMConversation, len(s.dms))
	copy(out, s.dms)
	return out, nil
}

func (s *stubTimelineProvider) DMs(agent string, opts data.MessageFilter) ([]fmail.Message, error) {
	s.dmCalls++
	s.dmOpts = append(s.dmOpts, opts)
	return filterTimelineMessages(s.byDM[agent], opts), nil
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
	msg, ok := v.loadWindowCmd(data.MessageFilter{Limit: timelineInitialPageSize}, timelineLoadReplace)().(timelineLoadedMsg)
	require.True(t, ok)
	require.NoError(t, msg.err)
	v.applyLoaded(msg)

	require.Len(t, v.all, 2)
	require.Equal(t, "20260209-095500-0001", v.all[0].ID)
	require.Equal(t, "20260209-095700-0001", v.all[1].ID)
}

func TestTimelineLazyPagingLoadsOlderNearTop(t *testing.T) {
	now := time.Date(2026, 2, 9, 12, 0, 0, 0, time.UTC)
	total := timelineInitialPageSize + timelinePageSize + 10
	msgs := make([]fmail.Message, 0, total)
	for i := 0; i < total; i++ {
		ts := now.Add(-time.Duration(total-i) * time.Minute)
		msgs = append(msgs, fmail.Message{
			ID:   fmt.Sprintf("%s-%04d", ts.Format("20060102-150405"), i),
			From: "alice",
			To:   "task",
			Time: ts,
			Body: fmt.Sprintf("msg-%d", i),
		})
	}

	provider := &stubTimelineProvider{
		topics:  []data.TopicInfo{{Name: "task", LastActivity: now}},
		byTopic: map[string][]fmail.Message{"task": msgs},
	}
	v := newTimelineView(t.TempDir(), "viewer", provider, nil)
	initial, ok := v.loadWindowCmd(data.MessageFilter{Limit: timelineInitialPageSize}, timelineLoadReplace)().(timelineLoadedMsg)
	require.True(t, ok)
	require.NoError(t, initial.err)
	v.applyLoaded(initial)

	require.Len(t, v.all, timelineInitialPageSize)
	require.True(t, v.hasOlder)

	v.rebuildVisible()
	v.selected = 0
	v.rememberSelection()

	cmd := v.handleKey(runeKey('k'))
	require.NotNil(t, cmd)

	older, ok := cmd().(timelineLoadedMsg)
	require.True(t, ok)
	require.NoError(t, older.err)
	v.applyLoaded(older)

	require.Len(t, v.all, timelineInitialPageSize+timelinePageSize)
	require.GreaterOrEqual(t, provider.messageCalls, 2)
	lastOpts := provider.messageOpts[len(provider.messageOpts)-1]
	require.Equal(t, timelinePageSize, lastOpts.Limit)
	require.False(t, lastOpts.Until.IsZero())
}

func TestTimelineTickDoesNotReloadProvider(t *testing.T) {
	now := time.Date(2026, 2, 9, 12, 0, 0, 0, time.UTC)
	provider := &stubTimelineProvider{
		topics: []data.TopicInfo{{Name: "task", LastActivity: now}},
		byTopic: map[string][]fmail.Message{
			"task": {{ID: "20260209-115900-0001", From: "alice", To: "task", Time: now.Add(-time.Minute), Body: "x"}},
		},
	}
	v := newTimelineView(t.TempDir(), "viewer", provider, nil)
	initial, ok := v.loadWindowCmd(data.MessageFilter{Limit: timelineInitialPageSize}, timelineLoadReplace)().(timelineLoadedMsg)
	require.True(t, ok)
	require.NoError(t, initial.err)
	v.applyLoaded(initial)

	before := provider.messageCalls
	v.Update(timelineTickMsg{})
	require.Equal(t, before, provider.messageCalls)
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

func filterTimelineMessages(input []fmail.Message, opts data.MessageFilter) []fmail.Message {
	if len(input) == 0 {
		return nil
	}
	out := make([]fmail.Message, 0, len(input))
	for i := range input {
		msg := input[i]
		if !opts.Since.IsZero() && msg.Time.Before(opts.Since) {
			continue
		}
		if !opts.Until.IsZero() && msg.Time.After(opts.Until) {
			continue
		}
		out = append(out, msg)
	}
	if opts.Limit > 0 && len(out) > opts.Limit {
		out = out[len(out)-opts.Limit:]
	}
	return out
}
