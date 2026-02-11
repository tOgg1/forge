package fmail

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestRobotHelpPayloadShape(t *testing.T) {
	payload := robotHelp("2.2.0")
	require.Equal(t, "2.2.0", payload.Version)

	data, err := json.Marshal(payload)
	require.NoError(t, err)
	require.True(t, json.Valid(data))

	var root map[string]any
	require.NoError(t, json.Unmarshal(data, &root))

	for _, key := range []string{
		"name", "version", "description", "setup", "commands",
		"patterns", "env", "message_format", "storage",
	} {
		_, ok := root[key]
		require.Truef(t, ok, "missing key %q", key)
	}

	commands, ok := root["commands"].(map[string]any)
	require.True(t, ok)
	for _, key := range []string{"send", "log", "messages", "watch", "who", "status", "register", "topics", "gc"} {
		_, ok := commands[key]
		require.Truef(t, ok, "missing command %q", key)
	}

	patterns, ok := root["patterns"].(map[string]any)
	require.True(t, ok)
	for _, key := range []string{"request_response", "broadcast", "coordinate"} {
		_, ok := patterns[key]
		require.Truef(t, ok, "missing pattern %q", key)
	}

	env, ok := root["env"].(map[string]any)
	require.True(t, ok)
	for _, key := range []string{"FMAIL_AGENT", "FMAIL_ROOT", "FMAIL_PROJECT"} {
		_, ok := env[key]
		require.Truef(t, ok, "missing env %q", key)
	}

	format, ok := root["message_format"].(map[string]any)
	require.True(t, ok)
	for _, key := range []string{"id", "from", "to", "time", "body"} {
		_, ok := format[key]
		require.Truef(t, ok, "missing message_format %q", key)
	}
}

func TestNormalizeRobotHelpVersion(t *testing.T) {
	require.Equal(t, robotHelpSpecVersion, normalizeRobotHelpVersion("dev"))
	require.Equal(t, robotHelpSpecVersion, normalizeRobotHelpVersion(""))
	require.Equal(t, "2.2.0", normalizeRobotHelpVersion("v2.2.0"))
}
