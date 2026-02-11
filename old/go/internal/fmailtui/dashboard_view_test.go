package fmailtui

import (
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
)

type stubDashboardProvider struct {
	topics []data.TopicInfo
	agents []fmail.AgentRecord

	topicsCalls int
	agentCalls  int
}

func (s *stubDashboardProvider) Topics() ([]data.TopicInfo, error) {
	s.topicsCalls++
	out := make([]data.TopicInfo, len(s.topics))
	copy(out, s.topics)
	return out, nil
}

func (s *stubDashboardProvider) Messages(string, data.MessageFilter) ([]fmail.Message, error) {
	return nil, nil
}

func (s *stubDashboardProvider) DMConversations(string) ([]data.DMConversation, error) {
	return nil, nil
}

func (s *stubDashboardProvider) DMs(string, data.MessageFilter) ([]fmail.Message, error) {
	return nil, nil
}

func (s *stubDashboardProvider) Agents() ([]fmail.AgentRecord, error) {
	s.agentCalls++
	out := make([]fmail.AgentRecord, len(s.agents))
	copy(out, s.agents)
	return out, nil
}

func (s *stubDashboardProvider) Search(data.SearchQuery) ([]data.SearchResult, error) {
	return nil, nil
}

func (s *stubDashboardProvider) Subscribe(data.SubscriptionFilter) (<-chan fmail.Message, func()) {
	ch := make(chan fmail.Message)
	return ch, func() { close(ch) }
}

func TestDashboardShouldRefreshDebouncesTicks(t *testing.T) {
	v := newDashboardView(t.TempDir(), "", &stubDashboardProvider{}, nil)
	base := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	v.lastRefresh = base

	require.False(t, v.shouldRefresh(base.Add(5*time.Second)))
	require.True(t, v.shouldRefresh(base.Add(dashboardMetadataRefresh)))
}

func TestDashboardApplyTopicsSnapshotTracksHotCountsByMetadataDelta(t *testing.T) {
	v := newDashboardView(t.TempDir(), "", &stubDashboardProvider{}, nil)
	now := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	v.topicCounts["task"] = 2

	v.applyTopicsSnapshot([]data.TopicInfo{{
		Name:         "task",
		MessageCount: 5,
		LastActivity: now,
	}}, now)

	require.Equal(t, 3, v.hotCounts["task"])
	require.Equal(t, 5, v.topicCounts["task"])
}

func TestDashboardApplyIncomingUpdatesHotCountsIncrementally(t *testing.T) {
	v := newDashboardView(t.TempDir(), "", &stubDashboardProvider{}, nil)
	now := time.Date(2026, 2, 9, 11, 0, 0, 0, time.UTC)
	v.applyTopicsSnapshot([]data.TopicInfo{{
		Name:         "task",
		MessageCount: 2,
		LastActivity: now.Add(-10 * time.Minute),
	}}, now)

	v.applyIncoming(fmail.Message{
		ID:   "20260209-110100-0001",
		From: "alice",
		To:   "task",
		Time: now.Add(time.Minute),
		Body: "new",
	})

	require.Equal(t, 1, v.hotCounts["task"])
	require.Equal(t, 3, v.topicCounts["task"])
	require.Equal(t, 3, v.topics[0].MessageCount)
}

func TestDashboardRenderFeedShowsStateAndPriorityCues(t *testing.T) {
	v := newDashboardView(t.TempDir(), "", &stubDashboardProvider{}, nil)
	v.feed = []fmail.Message{{
		ID:       "20260209-110100-0001",
		From:     "alice",
		To:       "",
		Priority: fmail.PriorityHigh,
		Time:     time.Date(2026, 2, 9, 11, 1, 2, 0, time.UTC),
		Body:     "important update",
	}}
	v.feedOffset = 2

	rendered := v.renderFeedPanel(88, 10, themePalette(ThemeDefault), false)
	require.True(t, strings.Contains(rendered, "paused:2"))
	require.True(t, strings.Contains(rendered, "Enter:thread"))
	require.True(t, strings.Contains(rendered, "(unknown)"))
	require.True(t, strings.Contains(rendered, "[HIGH]"))
	require.True(t, strings.Contains(rendered, "PAUSED (2)"))
}
