package fmailtui

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestParseQuickSendInput(t *testing.T) {
	target, body, ok := parseQuickSendInput(":task ship auth")
	require.True(t, ok)
	require.Equal(t, "task", target)
	require.Equal(t, "ship auth", body)

	_, _, ok = parseQuickSendInput(":task")
	require.False(t, ok)

	_, _, ok = parseQuickSendInput(":   ")
	require.False(t, ok)
}

func TestThreadViewComposeReplySeed(t *testing.T) {
	v := &threadView{
		topic: "task",
		rows: []threadRow{
			{msg: fmail.Message{ID: "20260209-080000-0001", From: "architect", To: "task", Body: "Plan v1\nstep 2"}},
		},
		selected: 0,
	}

	seed, ok := v.ComposeReplySeed(false)
	require.True(t, ok)
	require.Equal(t, "task", seed.Target)
	require.Equal(t, "20260209-080000-0001", seed.ReplyTo)
	require.Equal(t, "Plan v1", seed.ParentLine)

	dmSeed, ok := v.ComposeReplySeed(true)
	require.True(t, ok)
	require.Equal(t, "@architect", dmSeed.Target)
}

func TestComposeSendRequestQuick(t *testing.T) {
	m := &Model{selfAgent: "viewer"}
	m.quick.input = ":task hello there"

	req, err := m.composeSendRequest(sendSourceQuick)
	require.NoError(t, err)
	require.Equal(t, "viewer", req.From)
	require.Equal(t, "task", req.To)
	require.Equal(t, "hello there", req.Body)
	require.Equal(t, fmail.PriorityNormal, req.Priority)
}
