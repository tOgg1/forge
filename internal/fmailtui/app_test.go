package fmailtui

import (
	"net"
	"os"
	"path/filepath"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/stretchr/testify/require"
)

func TestNewModelInitializesStoreAndProject(t *testing.T) {
	root := t.TempDir()
	model, err := NewModel(Config{
		Root:      root,
		ProjectID: "prj-test",
	})
	require.NoError(t, err)
	t.Cleanup(func() {
		require.NoError(t, model.Close())
	})

	require.Equal(t, "prj-test", model.projectID)
	require.Equal(t, defaultPollInterval, model.pollInterval)
	require.Equal(t, ThemeDefault, model.theme)
	require.Equal(t, []ViewID{ViewDashboard}, model.viewStack)

	_, err = os.Stat(filepath.Join(model.root, ".fmail", "project.json"))
	require.NoError(t, err)
}

func TestNewModelRejectsInvalidTheme(t *testing.T) {
	_, err := NewModel(Config{
		Root:  t.TempDir(),
		Theme: "matrix",
	})
	require.Error(t, err)
	require.Contains(t, err.Error(), "invalid theme")
}

func TestUpdateHandlesResizeHelpAndQuit(t *testing.T) {
	model := newTestModel(t, Config{})

	model = applyUpdate(t, model, tea.WindowSizeMsg{Width: 120, Height: 40})
	require.Equal(t, 120, model.width)
	require.Equal(t, 40, model.height)

	model = applyUpdate(t, model, runeKey('?'))
	require.True(t, model.showHelp)
	model = applyUpdate(t, model, runeKey('?'))
	require.False(t, model.showHelp)

	_, cmd := model.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	require.NotNil(t, cmd)
	_, ok := cmd().(tea.QuitMsg)
	require.True(t, ok)
}

func TestViewStackAndEnterNavigation(t *testing.T) {
	model := newTestModel(t, Config{})

	require.Equal(t, ViewDashboard, model.activeViewID())
	model = applyUpdate(t, model, runeKey('t'))
	require.Equal(t, ViewTopics, model.activeViewID())
	require.Equal(t, 2, len(model.viewStack))

	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyEsc})
	require.Equal(t, ViewDashboard, model.activeViewID())

	// Dashboard Enter opens the focused pane (agents by default).
	model = applyUpdateWithCmd(t, model, tea.KeyMsg{Type: tea.KeyEnter})
	require.Equal(t, ViewAgents, model.activeViewID())

	// Back to dashboard.
	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyEsc})
	require.Equal(t, ViewDashboard, model.activeViewID())

	// Tab to topics focus then Enter should route to thread (even if no topics yet).
	dash := model.views[ViewDashboard].(*dashboardView)
	require.Equal(t, focusAgents, dash.focus)
	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyTab})
	require.Equal(t, focusTopics, dash.focus)

	model = applyUpdateWithCmd(t, model, tea.KeyMsg{Type: tea.KeyEnter})
	require.Equal(t, ViewThread, model.activeViewID())

	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyEsc})
	require.Equal(t, ViewDashboard, model.activeViewID())
}

func TestRoutesKeyToActiveView(t *testing.T) {
	model := newTestModel(t, Config{})
	// Dashboard is now a *dashboardView, not a placeholder.
	// Verify that key events reach the active view without panic.
	view := model.views[ViewDashboard].(*dashboardView)
	require.NotNil(t, view)

	// Tab should cycle dashboard focus without error.
	require.Equal(t, focusAgents, view.focus)
	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyTab})
	require.Equal(t, focusTopics, view.focus)
	require.Equal(t, ViewDashboard, model.activeViewID())
}

func TestTopicsLocalKeysNotHijackedByGlobalRoutes(t *testing.T) {
	model := newTestModel(t, Config{})
	model = applyUpdate(t, model, runeKey('t'))
	require.Equal(t, ViewTopics, model.activeViewID())

	model = applyUpdate(t, model, runeKey('s'))
	require.Equal(t, ViewTopics, model.activeViewID()) // sort in Topics view, not Search route

	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'/'}})
	require.Equal(t, ViewTopics, model.activeViewID()) // filter in Topics view

	model = applyUpdate(t, model, runeKey('d'))
	require.Equal(t, ViewTopics, model.activeViewID()) // DM toggle in Topics view
}

func TestConnectForgedOptional(t *testing.T) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	require.NoError(t, err)
	defer ln.Close()

	accepted := make(chan struct{})
	go func() {
		conn, err := ln.Accept()
		if err != nil {
			return
		}
		close(accepted)
		_ = conn.Close()
	}()

	model, err := NewModel(Config{
		Root:       t.TempDir(),
		ForgedAddr: ln.Addr().String(),
	})
	require.NoError(t, err)
	require.NotNil(t, model.forgedClient)
	t.Cleanup(func() {
		require.NoError(t, model.Close())
	})

	select {
	case <-accepted:
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for forged dial")
	}
}

func TestNewModelForgedDialFailureNonFatal(t *testing.T) {
	root := t.TempDir()
	socketPath := filepath.Join(root, "missing.sock")

	model, err := NewModel(Config{
		Root:       root,
		ForgedAddr: "unix://" + socketPath,
	})
	require.NoError(t, err)
	require.Nil(t, model.forgedClient)
	require.Error(t, model.forgedErr)
	require.NoError(t, model.Close())
}

func newTestModel(t *testing.T, cfg Config) *Model {
	t.Helper()
	if cfg.Root == "" {
		cfg.Root = t.TempDir()
	}
	model, err := NewModel(cfg)
	require.NoError(t, err)
	t.Cleanup(func() {
		require.NoError(t, model.Close())
	})
	return model
}

func runeKey(r rune) tea.KeyMsg {
	return tea.KeyMsg{
		Type:  tea.KeyRunes,
		Runes: []rune{r},
	}
}

func applyUpdate(t *testing.T, model *Model, msg tea.Msg) *Model {
	t.Helper()
	next, _ := model.Update(msg)
	out, ok := next.(*Model)
	require.True(t, ok)
	return out
}

func applyUpdateWithCmd(t *testing.T, model *Model, msg tea.Msg) *Model {
	t.Helper()
	next, cmd := model.Update(msg)
	out, ok := next.(*Model)
	require.True(t, ok)
	if cmd == nil {
		return out
	}
	return runCmd(t, out, cmd)
}

func runCmd(t *testing.T, model *Model, cmd tea.Cmd) *Model {
	t.Helper()
	return runCmdDepth(t, model, cmd, 0)
}

const maxRunCmdDepth = 8

func runCmdDepth(t *testing.T, model *Model, cmd tea.Cmd, depth int) *Model {
	t.Helper()
	if cmd == nil || depth >= maxRunCmdDepth {
		return model
	}

	// Run cmd with a short timeout to skip blocking commands (ticks, channel waits).
	type result struct{ msg tea.Msg }
	ch := make(chan result, 1)
	go func() { ch <- result{cmd()} }()
	select {
	case r := <-ch:
		switch typed := r.msg.(type) {
		case nil:
			return model
		case tea.BatchMsg:
			out := model
			for _, sub := range typed {
				out = runCmdDepth(t, out, sub, depth+1)
			}
			return out
		default:
			next, nextCmd := model.Update(typed)
			out, ok := next.(*Model)
			require.True(t, ok)
			return runCmdDepth(t, out, nextCmd, depth+1)
		}
	case <-time.After(50 * time.Millisecond):
		// Command is blocking (tick, subscription wait) — skip it.
		return model
	}
}
