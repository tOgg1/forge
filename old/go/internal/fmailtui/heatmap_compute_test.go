package fmailtui

import (
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestBuildHeatmapMatrix_AgentBuckets(t *testing.T) {
	start := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	end := start.Add(3 * time.Hour)
	bucket := time.Hour
	msgs := []fmail.Message{
		{ID: "1", From: "a", To: "task", Time: start.Add(5 * time.Minute), Body: "x"},
		{ID: "2", From: "a", To: "task", Time: start.Add(65 * time.Minute), Body: "x"},
		{ID: "3", From: "b", To: "build", Time: start.Add(70 * time.Minute), Body: "x"},
		{ID: "4", From: "a", To: "@b", Time: start.Add(125 * time.Minute), Body: "x"},
	}

	m := buildHeatmapMatrix(msgs, start, end, bucket, heatmapModeAgents)
	require.Equal(t, 3, m.Cols)
	require.Len(t, m.Rows, 2)
	// Sort by total for deterministic assertions.
	m.sortRows(heatmapSortTotal)
	require.Equal(t, "a", m.Rows[0].Label)
	require.Equal(t, 3, m.Rows[0].Total)
	require.Equal(t, []int{1, 1, 1}, m.Rows[0].Counts)
	require.Equal(t, "b", m.Rows[1].Label)
	require.Equal(t, 1, m.Rows[1].Total)
	require.Equal(t, []int{0, 1, 0}, m.Rows[1].Counts)
}

func TestBuildHeatmapMatrix_TopicMode(t *testing.T) {
	start := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	end := start.Add(2 * time.Hour)
	bucket := time.Hour
	msgs := []fmail.Message{
		{ID: "1", From: "a", To: "task", Time: start.Add(10 * time.Minute), Body: "x"},
		{ID: "2", From: "b", To: "task", Time: start.Add(20 * time.Minute), Body: "x"},
		{ID: "3", From: "a", To: "build", Time: start.Add(70 * time.Minute), Body: "x"},
	}

	m := buildHeatmapMatrix(msgs, start, end, bucket, heatmapModeTopics)
	require.Equal(t, 2, m.Cols)
	require.Len(t, m.Rows, 2)
	m.sortRows(heatmapSortName)
	require.Equal(t, "build", m.Rows[0].Label)
	require.Equal(t, []int{0, 1}, m.Rows[0].Counts)
	require.Equal(t, "task", m.Rows[1].Label)
	require.Equal(t, []int{2, 0}, m.Rows[1].Counts)
}

