package fmailtui

import (
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
)

func TestNotificationCenterDefaultsDedupAndPersist(t *testing.T) {
	root := t.TempDir()
	st := tuistate.New(filepath.Join(root, ".fmail", "tui-state.json"))
	require.NoError(t, st.Load())

	center := newNotificationCenter("viewer", st)
	rules := center.Rules()
	require.GreaterOrEqual(t, len(rules), 2)

	msg := fmail.Message{
		ID:       "20260209-103000-0001",
		From:     "architect",
		To:       "@viewer",
		Priority: fmail.PriorityHigh,
		Body:     "refresh tokens should rotate",
		Time:     time.Now().UTC(),
	}
	actions, ok := center.ProcessMessage(msg)
	require.True(t, ok)
	require.True(t, actions.Bell)
	require.True(t, actions.Badge)
	require.True(t, actions.Highlight)
	require.Equal(t, 1, center.UnreadCount())

	_, dup := center.ProcessMessage(msg)
	require.False(t, dup)
	require.Equal(t, 1, len(center.Notifications()))

	require.True(t, center.MarkRead(msg.ID))
	require.Equal(t, 0, center.UnreadCount())
	require.True(t, center.Dismiss(msg.ID))
	require.Equal(t, 0, len(center.Notifications()))

	for i := 0; i < 60; i++ {
		id := "20260209-1031" + twoDigits(i) + "-0001"
		_, _ = center.ProcessMessage(fmail.Message{ID: id, From: "tester", To: "@viewer", Priority: fmail.PriorityHigh, Body: "test"})
	}
	require.LessOrEqual(t, len(center.Notifications()), notificationMemoryLimit)
	require.NoError(t, st.SaveNow())

	reloaded := tuistate.New(filepath.Join(root, ".fmail", "tui-state.json"))
	require.NoError(t, reloaded.Load())
	require.Len(t, reloaded.Notifications(), notificationPersistLimit)
}

func TestNotificationCenterRuleMatchingConditions(t *testing.T) {
	center := newNotificationCenter("viewer", nil)
	center.SetRules([]tuistate.NotificationRule{
		{
			Name:            "auth-watch",
			Topic:           "task*",
			From:            "arch*",
			To:              "task",
			Priority:        fmail.PriorityNormal,
			Tags:            []string{"auth", "jwt"},
			Text:            "refresh.*token",
			ActionFlash:     true,
			ActionHighlight: true,
			Enabled:         true,
		},
	})

	actions, ok := center.ProcessMessage(fmail.Message{
		ID:       "20260209-104000-0001",
		From:     "architect",
		To:       "task",
		Priority: fmail.PriorityHigh,
		Tags:     []string{"build", "auth"},
		Body:     "refresh token strategy",
	})
	require.True(t, ok)
	require.True(t, actions.Flash)
	require.True(t, actions.Highlight)

	_, ok = center.ProcessMessage(fmail.Message{
		ID:       "20260209-104001-0001",
		From:     "architect",
		To:       "task",
		Priority: fmail.PriorityLow,
		Tags:     []string{"auth"},
		Body:     "refresh token strategy",
	})
	require.False(t, ok)
}

func TestParseNotificationRuleSpec(t *testing.T) {
	rule, err := parseNotificationRuleSpec("name=auth topic=task* from=arch* to=@viewer priority=high tags=auth,jwt text=refresh.* actions=badge,flash enabled=true")
	require.NoError(t, err)
	require.Equal(t, "auth", rule.Name)
	require.Equal(t, "task*", rule.Topic)
	require.Equal(t, "arch*", rule.From)
	require.Equal(t, "@viewer", rule.To)
	require.Equal(t, fmail.PriorityHigh, rule.Priority)
	require.Equal(t, []string{"auth", "jwt"}, rule.Tags)
	require.True(t, rule.ActionBadge)
	require.True(t, rule.ActionFlash)
	require.False(t, rule.ActionBell)
	require.True(t, rule.Enabled)
}

func TestNotificationRulePreviewMatches(t *testing.T) {
	provider := &notificationPreviewProvider{results: []data.SearchResult{
		{Message: fmail.Message{ID: "1", From: "architect", To: "task", Priority: fmail.PriorityHigh, Body: "refresh token"}},
		{Message: fmail.Message{ID: "2", From: "tester", To: "build", Priority: fmail.PriorityNormal, Body: "run tests"}},
	}}
	center := newNotificationCenter("viewer", nil)
	rule := tuistate.NotificationRule{Name: "r1", Text: "refresh", Enabled: true, ActionBadge: true}
	matches, scanned, err := center.PreviewMatches(rule, provider, 100)
	require.NoError(t, err)
	require.Equal(t, 1, matches)
	require.Equal(t, 2, scanned)
}

type notificationPreviewProvider struct {
	results []data.SearchResult
}

func (p *notificationPreviewProvider) Topics() ([]data.TopicInfo, error) { return nil, nil }
func (p *notificationPreviewProvider) Messages(string, data.MessageFilter) ([]fmail.Message, error) {
	return nil, nil
}
func (p *notificationPreviewProvider) DMConversations(string) ([]data.DMConversation, error) {
	return nil, nil
}
func (p *notificationPreviewProvider) DMs(string, data.MessageFilter) ([]fmail.Message, error) {
	return nil, nil
}
func (p *notificationPreviewProvider) Agents() ([]fmail.AgentRecord, error) { return nil, nil }
func (p *notificationPreviewProvider) Search(data.SearchQuery) ([]data.SearchResult, error) {
	return p.results, nil
}
func (p *notificationPreviewProvider) Subscribe(data.SubscriptionFilter) (<-chan fmail.Message, func()) {
	ch := make(chan fmail.Message)
	close(ch)
	return ch, func() {}
}

func twoDigits(i int) string {
	if i < 10 {
		return "0" + string(rune('0'+i))
	}
	return string(rune('0'+(i/10))) + string(rune('0'+(i%10)))
}
