package fmailtui

import (
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmailtui/state"
)

func TestParseQuickSendInput(t *testing.T) {
	target, body, ok := parseQuickSendInput(":task implement JWT auth")
	require.True(t, ok)
	require.Equal(t, "task", target)
	require.Equal(t, "implement JWT auth", body)

	_, _, ok = parseQuickSendInput(":task")
	require.False(t, ok)

	_, _, ok = parseQuickSendInput(":")
	require.False(t, ok)
}

func TestDraftPersistenceRestorePrompt(t *testing.T) {
	root := t.TempDir()
	statePath := filepath.Join(root, ".fmail", "tui-state.json")

	m := &Model{
		root:      root,
		selfAgent: "viewer",
		tuiState:  state.New(statePath),
	}
	require.NoError(t, m.tuiState.Load())

	m.compose.active = true
	m.compose.to = "task"
	m.compose.priority = "normal"
	m.compose.tags = "auth, urgent"
	m.compose.body = "draft message"
	m.persistDraft(true)
	require.NoError(t, m.tuiState.SaveNow())

	m2 := &Model{
		root:      root,
		selfAgent: "viewer",
		tuiState:  state.New(statePath),
	}
	require.NoError(t, m2.tuiState.Load())

	m2.openComposeOverlay("task", composeReplySeed{})
	require.True(t, m2.compose.restoreAsk)
	require.Equal(t, "draft message", m2.compose.draftCached.Body)

	m2.persistDraft(false)
	require.NoError(t, m2.tuiState.SaveNow())
	_, ok := m2.tuiState.Draft("task")
	require.False(t, ok)
}
