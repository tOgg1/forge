package data

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestNormalizeSendRequestDefaults(t *testing.T) {
	msg, err := normalizeSendRequest(SendRequest{
		To:   "task",
		Body: "ship",
		Tags: []string{"Auth", "auth", ""},
	}, "viewer")
	require.NoError(t, err)
	require.Equal(t, "viewer", msg.From)
	require.Equal(t, "task", msg.To)
	require.Equal(t, "ship", msg.Body)
	require.Equal(t, fmail.PriorityNormal, msg.Priority)
	require.Equal(t, []string{"auth"}, msg.Tags)
	require.False(t, msg.Time.IsZero())
}

func TestNormalizeSendRequestRejectsMissingFields(t *testing.T) {
	_, err := normalizeSendRequest(SendRequest{To: "", Body: "x"}, "viewer")
	require.Error(t, err)

	_, err = normalizeSendRequest(SendRequest{To: "task", Body: ""}, "viewer")
	require.Error(t, err)
}
