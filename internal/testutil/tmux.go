package testutil

import (
	"context"
	"fmt"
	"os/exec"
	"strings"
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/tmux"
)

// RequireTmux skips the test if tmux is not installed and returns a local client.
func RequireTmux(t *testing.T) *tmux.Client {
	t.Helper()
	if _, err := exec.LookPath("tmux"); err != nil {
		t.Skip("tmux not installed")
	}
	return tmux.NewLocalClient()
}

// NewTmuxSession creates a temporary tmux session and returns a cleanup function.
func NewTmuxSession(t *testing.T, session, workDir string) (*tmux.Client, string, func()) {
	t.Helper()
	client := RequireTmux(t)
	if session == "" {
		session = fmt.Sprintf("swarm-test-%d", time.Now().UnixNano())
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := client.NewSession(ctx, session, workDir); err != nil {
		t.Fatalf("failed to create tmux session: %v", err)
	}

	cleanup := func() {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = client.KillSession(ctx, session)
	}

	return client, session, cleanup
}

// TmuxTestEnv provides a full test environment for integration tests.
type TmuxTestEnv struct {
	Client  *tmux.Client
	Session string
	WorkDir string
	cleanup func()
	t       *testing.T
}

// NewTmuxTestEnv creates a complete integration test environment with tmux.
func NewTmuxTestEnv(t *testing.T) *TmuxTestEnv {
	t.Helper()
	workDir := t.TempDir()
	client, session, cleanup := NewTmuxSession(t, "", workDir)
	return &TmuxTestEnv{
		Client:  client,
		Session: session,
		WorkDir: workDir,
		cleanup: cleanup,
		t:       t,
	}
}

// Close cleans up the test environment.
func (e *TmuxTestEnv) Close() {
	if e.cleanup != nil {
		e.cleanup()
	}
}

// SendKeys sends keys to the session's first pane.
func (e *TmuxTestEnv) SendKeys(keys string, enter bool) {
	e.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := e.Client.SendKeys(ctx, e.Session, keys, true, enter); err != nil {
		e.t.Fatalf("failed to send keys: %v", err)
	}
}

// Capture captures the pane content.
func (e *TmuxTestEnv) Capture() string {
	e.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	content, err := e.Client.CapturePane(ctx, e.Session, false)
	if err != nil {
		e.t.Fatalf("failed to capture pane: %v", err)
	}
	return content
}

// CaptureWithHistory captures the pane content including scrollback history.
func (e *TmuxTestEnv) CaptureWithHistory() string {
	e.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	content, err := e.Client.CapturePane(ctx, e.Session, true)
	if err != nil {
		e.t.Fatalf("failed to capture pane with history: %v", err)
	}
	return content
}

// WaitForContent waits for specific content to appear in the pane.
func (e *TmuxTestEnv) WaitForContent(substring string, timeout time.Duration) bool {
	e.t.Helper()
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		content := e.Capture()
		if strings.Contains(content, substring) {
			return true
		}
		time.Sleep(100 * time.Millisecond)
	}
	return false
}

// WaitForStable waits for pane content to stabilize (stop changing).
func (e *TmuxTestEnv) WaitForStable(timeout time.Duration, stableFor time.Duration) string {
	e.t.Helper()
	deadline := time.Now().Add(timeout)
	var lastContent string
	stableSince := time.Now()

	for time.Now().Before(deadline) {
		content := e.Capture()
		if content == lastContent {
			if time.Since(stableSince) >= stableFor {
				return content
			}
		} else {
			lastContent = content
			stableSince = time.Now()
		}
		time.Sleep(50 * time.Millisecond)
	}
	return lastContent
}

// SplitPane creates a new pane in the session.
func (e *TmuxTestEnv) SplitPane(horizontal bool) string {
	e.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	paneID, err := e.Client.SplitWindow(ctx, e.Session, horizontal, e.WorkDir)
	if err != nil {
		e.t.Fatalf("failed to split window: %v", err)
	}
	return paneID
}

// ListPanes returns all panes in the session.
func (e *TmuxTestEnv) ListPanes() []tmux.Pane {
	e.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	panes, err := e.Client.ListPanes(ctx, e.Session)
	if err != nil {
		e.t.Fatalf("failed to list panes: %v", err)
	}
	return panes
}

// SendInterrupt sends Ctrl+C to the session.
func (e *TmuxTestEnv) SendInterrupt() {
	e.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := e.Client.SendInterrupt(ctx, e.Session); err != nil {
		e.t.Fatalf("failed to send interrupt: %v", err)
	}
}
