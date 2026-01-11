package fmail

import (
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestRegisterAgentRecordUnique(t *testing.T) {
	root := t.TempDir()
	fixed := time.Date(2026, 1, 10, 18, 0, 0, 0, time.UTC)

	store, err := NewStore(root, WithNow(func() time.Time { return fixed }))
	require.NoError(t, err)

	record, err := store.RegisterAgentRecord("Alice", "test-host")
	require.NoError(t, err)
	require.Equal(t, "alice", record.Name)
	require.Equal(t, "test-host", record.Host)
	require.Equal(t, fixed, record.FirstSeen)
	require.Equal(t, fixed, record.LastSeen)

	_, err = store.RegisterAgentRecord("alice", "other-host")
	require.ErrorIs(t, err, ErrAgentExists)
}
