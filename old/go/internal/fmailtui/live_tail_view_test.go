package fmailtui

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/state"
)

func TestParseLiveTailFilter(t *testing.T) {
	f := parseLiveTailFilter("from:alice to:task priority:high tag:auth text:refresh dm:only")
	require.Equal(t, "alice", f.From)
	require.Equal(t, "task", f.To)
	require.Equal(t, "high", f.Priority)
	require.Equal(t, []string{"auth"}, f.Tags)
	require.Equal(t, "refresh", f.Text)
	require.True(t, f.DMOnly)
}

func TestLiveTailPauseBuffersAndResumeFlushes(t *testing.T) {
	v := newLiveTailView("", "me", nil, state.New(""))
	v.paused = true

	_ = v.applyIncoming(fmail.Message{From: "a", To: "task", Body: "hello"})
	require.Len(t, v.feed, 0)
	require.Len(t, v.buffered, 1)

	v.resume()
	require.Len(t, v.feed, 1)
	require.Len(t, v.buffered, 0)
}

func TestLiveTailFilterMatching(t *testing.T) {
	v := newLiveTailView("", "me", nil, state.New(""))
	v.filter = liveTailFilter{From: "alice", Priority: fmail.PriorityHigh, DMOnly: true}

	require.False(t, v.matchesFilter(fmail.Message{From: "alice", To: "task", Priority: fmail.PriorityHigh, Body: "x"}))
	require.True(t, v.matchesFilter(fmail.Message{From: "alice", To: "@bob", Priority: fmail.PriorityHigh, Body: "x"}))
	require.False(t, v.matchesFilter(fmail.Message{From: "bob", To: "@bob", Priority: fmail.PriorityHigh, Body: "x"}))
}

