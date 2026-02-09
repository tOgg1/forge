package fmailtui

import (
	"fmt"
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
)

type stubThreadProvider struct {
	topics  []data.TopicInfo
	byTopic map[string][]fmail.Message
}

func (s *stubThreadProvider) Topics() ([]data.TopicInfo, error) {
	out := make([]data.TopicInfo, len(s.topics))
	for i := range s.topics {
		out[i] = s.topics[i]
		if out[i].MessageCount == 0 {
			if msgs, ok := s.byTopic[out[i].Name]; ok {
				out[i].MessageCount = len(msgs)
			}
		}
	}
	return out, nil
}

func (s *stubThreadProvider) Messages(topic string, opts data.MessageFilter) ([]fmail.Message, error) {
	msgs := append([]fmail.Message(nil), s.byTopic[topic]...)
	if opts.Limit > 0 && len(msgs) > opts.Limit {
		msgs = msgs[len(msgs)-opts.Limit:]
	}
	return msgs, nil
}

func (s *stubThreadProvider) DMConversations(string) ([]data.DMConversation, error) {
	return nil, nil
}

func (s *stubThreadProvider) DMs(agent string, opts data.MessageFilter) ([]fmail.Message, error) {
	msgs := append([]fmail.Message(nil), s.byTopic["@"+agent]...)
	if opts.Limit > 0 && len(msgs) > opts.Limit {
		msgs = msgs[len(msgs)-opts.Limit:]
	}
	return msgs, nil
}

func (s *stubThreadProvider) Agents() ([]fmail.AgentRecord, error) { return nil, nil }

func (s *stubThreadProvider) Search(data.SearchQuery) ([]data.SearchResult, error) {
	return nil, nil
}

func (s *stubThreadProvider) Subscribe(data.SubscriptionFilter) (<-chan fmail.Message, func()) {
	ch := make(chan fmail.Message)
	return ch, func() { close(ch) }
}

func TestThreadViewDepthClampAddsOverflowIndicator(t *testing.T) {
	msgs := makeThreadChain(9)
	v := newThreadView("", &stubThreadProvider{})
	v.mode = threadModeThreaded
	v.allMsgs = msgs
	v.rebuildRows("", false)

	idx := v.indexForID(msgs[len(msgs)-1].ID)
	require.GreaterOrEqual(t, idx, 0)
	row := v.rows[idx]
	require.Equal(t, threadMaxDepth, row.depth)
	require.True(t, row.overflow)
	require.Contains(t, row.connector, "└─")
}

func TestThreadViewEnterExpandsLongMessage(t *testing.T) {
	longBody := strings.Repeat("line\n", threadMaxBodyLines+5)
	msgs := []fmail.Message{
		{ID: "20260209-080000-0001", From: "architect", To: "task", Time: time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC), Body: longBody},
		{ID: "20260209-080001-0001", From: "coder", To: "task", Time: time.Date(2026, 2, 9, 8, 0, 1, 0, time.UTC), Body: "reply", ReplyTo: "20260209-080000-0001"},
	}
	provider := &stubThreadProvider{
		topics:  []data.TopicInfo{{Name: "task", LastActivity: msgs[1].Time}},
		byTopic: map[string][]fmail.Message{"task": msgs},
	}

	v := newThreadView("", provider)
	v.lastWidth = 120
	v.lastHeight = 30
	v.applyLoaded(mustLoad(v))

	idx := v.indexForID("20260209-080000-0001")
	require.GreaterOrEqual(t, idx, 0)
	v.selected = idx

	cmd := v.handleKey(tea.KeyMsg{Type: tea.KeyEnter})
	require.Nil(t, cmd)
	require.True(t, v.expandedBodies["20260209-080000-0001"])
}

func TestThreadViewToggleFlatKeepsSelectedMessage(t *testing.T) {
	msgs := []fmail.Message{
		{ID: "20260209-080000-0001", From: "a", To: "task", Time: time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC), Body: "root"},
		{ID: "20260209-080001-0001", From: "b", To: "task", Time: time.Date(2026, 2, 9, 8, 0, 1, 0, time.UTC), Body: "child", ReplyTo: "20260209-080000-0001"},
	}
	provider := &stubThreadProvider{
		topics:  []data.TopicInfo{{Name: "task", LastActivity: msgs[1].Time}},
		byTopic: map[string][]fmail.Message{"task": msgs},
	}

	v := newThreadView("", provider)
	v.lastWidth = 120
	v.lastHeight = 30
	v.applyLoaded(mustLoad(v))

	idx := v.indexForID("20260209-080000-0001")
	require.GreaterOrEqual(t, idx, 0)
	v.selected = idx
	selectedBefore := v.selectedID()

	cmd := v.handleKey(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'f'}})
	require.Nil(t, cmd)
	require.Equal(t, threadModeFlat, v.mode)
	require.Equal(t, selectedBefore, v.selectedID())
}

func TestThreadViewPaginationLoadsOlderAtTop(t *testing.T) {
	msgs := make([]fmail.Message, 0, 1205)
	base := time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC)
	for i := 0; i < 1205; i++ {
		id := fmt.Sprintf("20260209-%06d-0001", 80000+i)
		msgs = append(msgs, fmail.Message{ID: id, From: "a", To: "task", Time: base.Add(time.Duration(i) * time.Second), Body: "m"})
	}
	provider := &stubThreadProvider{
		topics:  []data.TopicInfo{{Name: "task", LastActivity: msgs[len(msgs)-1].Time, MessageCount: len(msgs)}},
		byTopic: map[string][]fmail.Message{"task": msgs},
	}

	v := newThreadView("", provider)
	v.lastWidth = 120
	v.lastHeight = 30
	v.applyLoaded(mustLoad(v))

	require.Equal(t, threadPageSize, len(v.allMsgs))
	require.Equal(t, len(msgs), v.total)

	v.selected = 0
	cmd := v.maybeLoadOlder()
	require.NotNil(t, cmd)
	loadedMsg, ok := cmd().(threadLoadedMsg)
	require.True(t, ok)
	v.applyLoaded(loadedMsg)
	require.Equal(t, threadPageSize*2, len(v.allMsgs))
}

func TestThreadViewPendingNewCountWhenScrolledUp(t *testing.T) {
	msgs := make([]fmail.Message, 0, 20)
	base := time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC)
	for i := 0; i < 20; i++ {
		msgs = append(msgs, fmail.Message{ID: fmt.Sprintf("20260209-0800%02d-0001", i), From: "a", To: "task", Time: base.Add(time.Duration(i) * time.Second), Body: "line"})
	}
	provider := &stubThreadProvider{
		topics:  []data.TopicInfo{{Name: "task", LastActivity: msgs[len(msgs)-1].Time, MessageCount: len(msgs)}},
		byTopic: map[string][]fmail.Message{"task": msgs},
	}

	v := newThreadView("", provider)
	v.lastWidth = 80
	v.lastHeight = 12
	v.applyLoaded(mustLoad(v))
	v.selected = 0 // not at bottom

	next := append([]fmail.Message(nil), msgs...)
	next = append(next, fmail.Message{ID: "20260209-081000-0001", From: "a", To: "task", Time: base.Add(100 * time.Second), Body: "new"})
	provider.byTopic["task"] = next
	provider.topics = []data.TopicInfo{{Name: "task", LastActivity: next[len(next)-1].Time, MessageCount: len(next)}}

	v.applyLoaded(mustLoad(v))
	require.Greater(t, v.pendingNew, 0)
}

func mustLoad(v *threadView) threadLoadedMsg {
	msg, ok := v.loadCmd()().(threadLoadedMsg)
	if !ok {
		panic("expected threadLoadedMsg")
	}
	return msg
}

func makeThreadChain(n int) []fmail.Message {
	base := time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC)
	msgs := make([]fmail.Message, 0, n)
	parent := ""
	for i := 0; i < n; i++ {
		id := fmt.Sprintf("20260209-0800%02d-0001", i)
		msg := fmail.Message{ID: id, From: "a", To: "task", Time: base.Add(time.Duration(i) * time.Second), Body: "m"}
		if parent != "" {
			msg.ReplyTo = parent
		}
		msgs = append(msgs, msg)
		parent = id
	}
	return msgs
}
