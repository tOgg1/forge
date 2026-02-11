package fmailtui

import (
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

func TestStatusStateCounts(t *testing.T) {
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	var s statusState

	s.record(fmail.Message{From: "a"}, now.Add(-30*time.Second))
	s.record(fmail.Message{From: "b"}, now.Add(-10*time.Second))
	s.record(fmail.Message{From: "a"}, now.Add(-5*time.Second))

	require.Equal(t, 3, s.msgPerMin(now))
	require.Equal(t, 2, s.agentsActive(now))

	// After 11 minutes, everything pruned.
	later := now.Add(11 * time.Minute)
	_, _ = s.onTick(later)
	require.Equal(t, 0, s.msgPerMin(later))
	require.Equal(t, 0, s.agentsActive(later))
}

func TestReconnectBackoffClamps(t *testing.T) {
	require.Equal(t, 5*time.Second, reconnectBackoff(1))
	require.Equal(t, 10*time.Second, reconnectBackoff(2))
	require.Equal(t, 40*time.Second, reconnectBackoff(4))
	require.Equal(t, 60*time.Second, reconnectBackoff(10))
}

func TestStatusProbeStateTransitions(t *testing.T) {
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	var s statusState
	s.conn.maxAttempts = 2

	s.applyProbe(statusProbeMsg{configured: true, connected: false, method: "unix:.fmail/forged.sock"}, now)
	require.Equal(t, connReconnecting, s.conn.state)
	require.Equal(t, 1, s.conn.attempt)
	require.Equal(t, now.Add(5*time.Second), s.conn.nextProbe)

	s.applyProbe(statusProbeMsg{configured: true, connected: false, method: "unix:.fmail/forged.sock"}, now.Add(6*time.Second))
	require.Equal(t, connReconnecting, s.conn.state)
	require.Equal(t, 2, s.conn.attempt)
	require.Equal(t, now.Add(6*time.Second).Add(10*time.Second), s.conn.nextProbe)

	// Maxed out attempts: disconnected, no further increments.
	s.applyProbe(statusProbeMsg{configured: true, connected: false, method: "unix:.fmail/forged.sock"}, now.Add(20*time.Second))
	require.Equal(t, connDisconnected, s.conn.state)
	require.Equal(t, 2, s.conn.attempt)

	// Recovery.
	s.applyProbe(statusProbeMsg{configured: true, connected: true, method: "unix:.fmail/forged.sock"}, now.Add(21*time.Second))
	require.Equal(t, connConnected, s.conn.state)
	require.Equal(t, 0, s.conn.attempt)
}

func TestRenderConnStrings(t *testing.T) {
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)

	text, _ := renderConn(statusConn{state: connConnected, method: "unix:.fmail/forged.sock"}, now, styles.DefaultTheme)
	require.Contains(t, text, "connected")
	require.Contains(t, text, "unix:.fmail/forged.sock")

	text, _ = renderConn(statusConn{state: connReconnecting, method: "unix:.fmail/forged.sock", attempt: 1, maxAttempts: 10}, now, styles.DefaultTheme)
	require.Contains(t, text, "reconnecting")
	require.Contains(t, text, "attempt 1/10")

	text, _ = renderConn(statusConn{state: connPolling}, now, styles.DefaultTheme)
	require.Contains(t, text, "polling")
	require.Contains(t, text, "file:")
}
