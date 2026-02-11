package fmailtui

import (
	"sort"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
)

type operatorTestProvider struct {
	topics []data.TopicInfo
	dms    []data.DMConversation
	agents []fmail.AgentRecord

	topicMessages map[string][]fmail.Message
	dmMessages    map[string][]fmail.Message

	sent []data.SendRequest
}

func (p *operatorTestProvider) Topics() ([]data.TopicInfo, error) {
	out := make([]data.TopicInfo, len(p.topics))
	copy(out, p.topics)
	return out, nil
}

func (p *operatorTestProvider) Messages(topic string, opts data.MessageFilter) ([]fmail.Message, error) {
	msgs := append([]fmail.Message(nil), p.topicMessages[topic]...)
	if opts.Limit > 0 && len(msgs) > opts.Limit {
		msgs = msgs[len(msgs)-opts.Limit:]
	}
	return msgs, nil
}

func (p *operatorTestProvider) DMConversations(agent string) ([]data.DMConversation, error) {
	_ = agent
	out := make([]data.DMConversation, len(p.dms))
	copy(out, p.dms)
	return out, nil
}

func (p *operatorTestProvider) DMs(agent string, opts data.MessageFilter) ([]fmail.Message, error) {
	msgs := append([]fmail.Message(nil), p.dmMessages[agent]...)
	if opts.Limit > 0 && len(msgs) > opts.Limit {
		msgs = msgs[len(msgs)-opts.Limit:]
	}
	return msgs, nil
}

func (p *operatorTestProvider) Agents() ([]fmail.AgentRecord, error) {
	out := make([]fmail.AgentRecord, len(p.agents))
	copy(out, p.agents)
	return out, nil
}

func (p *operatorTestProvider) Search(query data.SearchQuery) ([]data.SearchResult, error) {
	_ = query
	return nil, nil
}

func (p *operatorTestProvider) Subscribe(filter data.SubscriptionFilter) (<-chan fmail.Message, func()) {
	_ = filter
	ch := make(chan fmail.Message)
	cancel := func() { close(ch) }
	return ch, cancel
}

func (p *operatorTestProvider) Send(req data.SendRequest) (fmail.Message, error) {
	p.sent = append(p.sent, req)
	return fmail.Message{
		ID:       fmail.NewMessageID(),
		From:     req.From,
		To:       req.To,
		Body:     req.Body,
		ReplyTo:  req.ReplyTo,
		Priority: req.Priority,
		Tags:     append([]string(nil), req.Tags...),
		Time:     time.Now().UTC(),
	}, nil
}

func TestOperatorSlashCommandsApplyPriorityTagsAndDM(t *testing.T) {
	now := time.Now().UTC()
	provider := &operatorTestProvider{
		agents: []fmail.AgentRecord{{Name: "architect", LastSeen: now}},
	}
	v := newOperatorView(t.TempDir(), "prj", "viewer", nil, provider, nil)

	v.compose = "/priority high"
	runOperatorCmd(v, v.submitCompose())
	require.Equal(t, fmail.PriorityHigh, v.composePriority)

	v.compose = "/tag urgent review"
	runOperatorCmd(v, v.submitCompose())
	require.Equal(t, []string{"urgent", "review"}, v.composeTags)

	v.compose = "/dm architect ship it"
	runOperatorCmd(v, v.submitCompose())
	require.Len(t, provider.sent, 1)
	require.Equal(t, "@architect", provider.sent[0].To)
	require.Equal(t, "ship it", provider.sent[0].Body)
	require.Equal(t, fmail.PriorityHigh, provider.sent[0].Priority)
	require.Equal(t, []string{"urgent", "review"}, provider.sent[0].Tags)
}

func TestOperatorGroupCreateAndSendPersists(t *testing.T) {
	root := t.TempDir()
	statePath := root + "/.fmail/tui-state.json"
	st := tuistate.New(statePath)
	require.NoError(t, st.Load())

	provider := &operatorTestProvider{}
	v := newOperatorView(root, "prj", "viewer", nil, provider, st)

	v.compose = "/group create frontend coder-1 coder-2"
	runOperatorCmd(v, v.submitCompose())
	require.Contains(t, v.groups, "frontend")
	require.Equal(t, []string{"@coder-1", "@coder-2"}, v.groups["frontend"])

	v.compose = "/group frontend deploy now"
	runOperatorCmd(v, v.submitCompose())
	require.Len(t, provider.sent, 2)
	actualTargets := []string{provider.sent[0].To, provider.sent[1].To}
	sort.Strings(actualTargets)
	require.Equal(t, []string{"@coder-1", "@coder-2"}, actualTargets)

	require.NoError(t, st.SaveNow())
	reloaded := tuistate.New(statePath)
	require.NoError(t, reloaded.Load())
	require.Equal(t, []string{"@coder-1", "@coder-2"}, reloaded.Groups()["frontend"])
}

func TestOperatorLoadConversationsUnread(t *testing.T) {
	now := time.Now().UTC()
	provider := &operatorTestProvider{
		topics: []data.TopicInfo{
			{
				Name:         "task",
				LastActivity: now.Add(-2 * time.Minute),
				LastMessage:  &fmail.Message{ID: "20260209-100000-0002"},
			},
		},
		dms:    []data.DMConversation{{Agent: "architect", LastActivity: now.Add(-time.Minute), UnreadCount: 2}},
		agents: []fmail.AgentRecord{{Name: "architect", LastSeen: now}},
		dmMessages: map[string][]fmail.Message{
			"architect": {
				{ID: "20260209-100000-0001", From: "architect", To: "@viewer", Body: "ping", Time: now.Add(-time.Minute)},
			},
		},
	}

	st := tuistate.New(t.TempDir() + "/.fmail/tui-state.json")
	require.NoError(t, st.Load())
	st.SetReadMarker("task", "20260209-100000-0001")

	v := newOperatorView(t.TempDir(), "prj", "viewer", nil, provider, st)
	loaded, ok := v.loadCmd()().(operatorLoadedMsg)
	require.True(t, ok)
	require.NoError(t, loaded.err)
	require.Equal(t, "@architect", loaded.target)
	require.Equal(t, 3, loaded.unread)
}

func runOperatorCmd(v *operatorView, cmd tea.Cmd) {
	if cmd == nil {
		return
	}
	msg := cmd()
	if msg == nil {
		return
	}
	_ = v.Update(msg)
}
