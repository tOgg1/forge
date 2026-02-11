package fmailtui

import (
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestComputeStats_Basics(t *testing.T) {
	t0 := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	msgs := []fmail.Message{
		{ID: "1", From: "architect", To: "task", Time: t0, Body: "root"},
		{ID: "2", From: "coder", To: "task", Time: t0.Add(10 * time.Second), Body: "reply1", ReplyTo: "1"},
		{ID: "3", From: "architect", To: "build", Time: t0.Add(2 * time.Minute), Body: "note"},
		{ID: "4", From: "tester", To: "@architect", Time: t0.Add(1 * time.Hour), Body: "dm root"},
		{ID: "5", From: "architect", To: "@tester", Time: t0.Add(1*time.Hour + 40*time.Second), Body: "dm reply", ReplyTo: "4"},
		{ID: "6", From: "reviewer", To: "task", Time: t0.Add(20 * time.Second), Body: "reply2", ReplyTo: "1"},
	}

	start := t0
	end := t0.Add(2 * time.Hour)
	s := computeStats(msgs, start, end)

	require.Equal(t, 6, s.TotalMessages)
	require.Equal(t, 4, s.ActiveAgents)
	require.Equal(t, 4, s.ActiveTopics)

	require.Equal(t, 3, s.ReplySamples)
	require.Equal(t, 20*time.Second, s.MedianReply)
	require.InDelta(t, float64((70*time.Second)/3), float64(s.AvgReply), float64(time.Second))

	require.Equal(t, 3, s.LongestThreadMessages)
	require.Equal(t, "1", s.MostRepliedID)
	require.Equal(t, 2, s.MostRepliedCount)

	require.NotEmpty(t, s.TopAgents)
	require.Equal(t, "architect", s.TopAgents[0].Label)
	require.Equal(t, 3, s.TopAgents[0].Count)

	require.NotEmpty(t, s.TopicVolumes)
	require.Equal(t, "task", s.TopicVolumes[0].Label)
	require.Equal(t, 3, s.TopicVolumes[0].Count)

	require.NotEmpty(t, s.ResponseLatency)
	require.Equal(t, "<30s", s.ResponseLatency[0].Label)
	require.Equal(t, 2, s.ResponseLatency[0].Count)
	require.Equal(t, "30s-5m", s.ResponseLatency[1].Label)
	require.Equal(t, 1, s.ResponseLatency[1].Count)

	require.Equal(t, t0, s.BusiestHourStart)
	require.Equal(t, 4, s.BusiestHourCount)
	require.Equal(t, t0.Add(1*time.Hour), s.QuietestHourStart)
	require.Equal(t, 2, s.QuietestHourCount)

	require.InDelta(t, 2.0, s.ThreadAvgMessages, 0.01)
	require.Equal(t, 1, s.ThreadDist.Standalone)
	require.Equal(t, 2, s.ThreadDist.Small)
	require.Equal(t, 0, s.ThreadDist.Medium)
	require.Equal(t, 0, s.ThreadDist.Large)

	sum := 0
	for _, n := range s.OverTimeCounts {
		sum += n
	}
	require.Equal(t, 6, sum)
}

func TestChooseBucketInterval_PrefersNiceSteps(t *testing.T) {
	start := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	end := start.Add(2 * time.Hour)
	require.Equal(t, 5*time.Minute, chooseBucketInterval(start, end, 48))

	end = start.Add(24 * time.Hour)
	require.Equal(t, 30*time.Minute, chooseBucketInterval(start, end, 48))

	end = start.Add(30 * 24 * time.Hour)
	require.Equal(t, 24*time.Hour, chooseBucketInterval(start, end, 48))
}

