package fmail

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestLogCommandAlias(t *testing.T) {
	cmd := newLogCmd()
	require.Contains(t, cmd.Aliases, "logs")
}

func TestTopicsCommandAlias(t *testing.T) {
	cmd := newTopicsCmd()
	require.Contains(t, cmd.Aliases, "topic")
}

func TestRootCommandAliasesAndMessagesCommand(t *testing.T) {
	root := newRootCmd("dev")

	found, _, err := root.Find([]string{"logs"})
	require.NoError(t, err)
	require.Equal(t, "log", found.Name())

	found, _, err = root.Find([]string{"topic"})
	require.NoError(t, err)
	require.Equal(t, "topics", found.Name())

	found, _, err = root.Find([]string{"messages"})
	require.NoError(t, err)
	require.Equal(t, "messages", found.Name())
}
