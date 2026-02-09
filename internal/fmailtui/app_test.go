package fmailtui

import (
	"net"
	"os"
	"path/filepath"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/layout"
	"github.com/tOgg1/forge/internal/fmailtui/state"
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

func TestNewModelOperatorStartsInOperatorView(t *testing.T) {
	model, err := NewModel(Config{
		Root:     t.TempDir(),
		Operator: true,
	})
	require.NoError(t, err)
	t.Cleanup(func() {
		require.NoError(t, model.Close())
	})

	require.Equal(t, ViewOperator, model.activeViewID())
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

	model = applyUpdateWithCmd(t, model, tea.KeyMsg{Type: tea.KeyEsc})
	require.Equal(t, ViewDashboard, model.activeViewID())

	// Dashboard Enter opens a focused drill-down pane.
	model = applyUpdateWithCmd(t, model, tea.KeyMsg{Type: tea.KeyEnter})
	require.NotEqual(t, ViewDashboard, model.activeViewID())

	// Back to dashboard.
	model = applyUpdateWithCmd(t, model, tea.KeyMsg{Type: tea.KeyEsc})
	require.Equal(t, ViewDashboard, model.activeViewID())

	// Tab changes layout focus, and Enter still drills down.
	beforeFocus := model.layout.Focus()
	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyTab})
	require.NotEqual(t, beforeFocus, model.layout.Focus())

	model = applyUpdateWithCmd(t, model, tea.KeyMsg{Type: tea.KeyEnter})
	require.NotEqual(t, ViewDashboard, model.activeViewID())

	model = applyUpdateWithCmd(t, model, tea.KeyMsg{Type: tea.KeyEsc})
	require.Equal(t, ViewDashboard, model.activeViewID())
}

func TestRoutesKeyToActiveView(t *testing.T) {
	model := newTestModel(t, Config{})
	// Dashboard is now a *dashboardView, not a placeholder.
	// Verify that key events reach the active view without panic.
	view := model.views[ViewDashboard].(*dashboardView)
	require.NotNil(t, view)

	// Tab should be routed without panic and keep dashboard active.
	beforeFocus := model.layout.Focus()
	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyTab})
	require.NotEqual(t, beforeFocus, model.layout.Focus())
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

func TestTimelineLocalKeysNotHijackedByGlobalRoutes(t *testing.T) {
	model := newTestModel(t, Config{})
	model.layout.SetMode(layout.ModeSingle)
	model = applyUpdate(t, model, runeKey('m'))
	require.Equal(t, ViewTimeline, model.activeViewID())

	view := model.views[ViewTimeline].(*timelineView)
	view.now = time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)
	view.all = []fmail.Message{
		{ID: "20260209-095500-0001", From: "alice", To: "task", Time: view.now.Add(-5 * time.Minute), Body: "msg"},
	}
	view.windowEnd = view.now
	view.rebuildReplyIndex()
	view.rebuildVisible()

	model = applyUpdate(t, model, runeKey('t'))
	require.Equal(t, ViewTimeline, model.activeViewID())
	require.True(t, view.jumpActive)

	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyEsc})
	require.Equal(t, ViewTimeline, model.activeViewID())
	require.False(t, view.jumpActive)

	model = applyUpdateWithCmd(t, model, runeKey('o'))
	require.Equal(t, ViewThread, model.activeViewID())
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

func TestLayoutControlsAndPersistence(t *testing.T) {
	root := t.TempDir()
	model := newTestModel(t, Config{Root: root})
	model.width = 160
	model.height = 50

	require.Equal(t, layout.ModeSplit, model.layout.Mode())
	require.Equal(t, 1, model.layout.Focus())

	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyTab})
	require.Equal(t, 0, model.layout.Focus())

	before := model.layout.SplitRatio()
	model = applyUpdate(t, model, runeKey('|'))
	require.Greater(t, model.layout.SplitRatio(), before)

	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyCtrlW})
	model = applyUpdate(t, model, runeKey('o'))
	require.True(t, model.layout.Expanded())

	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyCtrlZ})
	require.Equal(t, layout.ModeZen, model.layout.Mode())
	model = applyUpdate(t, model, tea.KeyMsg{Type: tea.KeyCtrlZ})
	require.Equal(t, layout.ModeSplit, model.layout.Mode())

	require.NoError(t, model.tuiState.SaveNow())
	reloaded, err := NewModel(Config{Root: root})
	require.NoError(t, err)
	t.Cleanup(func() {
		require.NoError(t, reloaded.Close())
	})

	require.Equal(t, layout.ModeSplit, reloaded.layout.Mode())
	require.True(t, reloaded.layout.Expanded())
	require.Greater(t, reloaded.layout.SplitRatio(), layout.DefaultSplitRatio)
}

func TestNewModelRestoresLayoutPreferences(t *testing.T) {
	root := t.TempDir()
	statePath := filepath.Join(root, ".fmail", "tui-state.json")
	st := state.New(statePath)
	require.NoError(t, st.Load())
	st.UpdatePreferences(func(p *state.Preferences) {
		p.DefaultLayout = "dashboard"
		p.LayoutSplitRatio = 0.6
		p.LayoutSplitCollapsed = true
		p.LayoutFocus = 3
		p.LayoutExpanded = true
		p.DashboardGrid = "1x3"
		p.DashboardViews = []string{"topics", "thread", "agents", "live-tail"}
	})
	require.NoError(t, st.SaveNow())

	model, err := NewModel(Config{Root: root})
	require.NoError(t, err)
	t.Cleanup(func() {
		require.NoError(t, model.Close())
	})

	require.Equal(t, layout.ModeDashboard, model.layout.Mode())
	require.Equal(t, 0.6, model.layout.SplitRatio())
	require.True(t, model.layout.SplitCollapsed())
	require.Equal(t, 3, model.layout.Focus())
	require.True(t, model.layout.Expanded())
	require.Equal(t, layout.Grid1x3, model.layout.Grid())
	require.Equal(t, [4]string{"topics", "thread", "agents", "live-tail"}, model.layout.DashboardViews())
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
