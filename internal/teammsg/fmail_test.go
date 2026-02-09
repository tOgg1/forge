package teammsg

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
)

type fakeSaver struct {
	last *fmail.Message
	err  error
}

func (s *fakeSaver) SaveMessage(message *fmail.Message) (string, error) {
	s.last = message
	return "id", s.err
}

func TestFmailMessenger_SendTaskPrefixesAt(t *testing.T) {
	saver := &fakeSaver{}
	m := &fmailMessenger{from: "sender", save: saver}
	require.NoError(t, m.SendTask("agent-1", "do it"))
	require.NotNil(t, saver.last)
	require.Equal(t, "sender", saver.last.From)
	require.Equal(t, "@agent-1", saver.last.To)
	require.Equal(t, "do it", saver.last.Body)
}

func TestFmailMessenger_SendTopicRejectsDMTarget(t *testing.T) {
	saver := &fakeSaver{}
	m := &fmailMessenger{from: "sender", save: saver}
	require.Error(t, m.SendTopic("@agent-1", "x"))
}

func TestFmailMessenger_EmptyBodyRejected(t *testing.T) {
	saver := &fakeSaver{}
	m := &fmailMessenger{from: "sender", save: saver}
	require.Error(t, m.SendTopic("task", " "))
}
