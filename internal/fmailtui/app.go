package fmailtui

import (
	"fmt"
	"net"
	"os"
	"path/filepath"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	"github.com/tOgg1/forge/internal/fmailtui/layout"
	"github.com/tOgg1/forge/internal/fmailtui/state"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

const (
	defaultPollInterval = 2 * time.Second
	forgedDialTimeout   = 300 * time.Millisecond
)

type Theme string

const (
	ThemeDefault      Theme = "default"
	ThemeHighContrast Theme = "high-contrast"
)

type ViewID string

const (
	ViewDashboard ViewID = "dashboard"
	ViewTopics    ViewID = "topics"
	ViewThread    ViewID = "thread"
	ViewAgents    ViewID = "agents"
	ViewOperator  ViewID = "operator"
	ViewSearch    ViewID = "search"
	ViewLiveTail  ViewID = "live-tail"
	ViewTimeline  ViewID = "timeline"
	ViewStats     ViewID = "stats"
	ViewBookmarks ViewID = "bookmarks"
	ViewNotify    ViewID = "notifications"
)

var viewSwitchKeys = map[string]ViewID{
	"o": ViewOperator,
	"t": ViewTopics,
	"r": ViewThread,
	"a": ViewAgents,
	"l": ViewLiveTail,
	"m": ViewTimeline,
	"p": ViewStats,
	"N": ViewNotify,
	"D": ViewDashboard,
	"S": ViewSearch,
}

var defaultEnterRoute = map[ViewID]ViewID{
	ViewDashboard: ViewTopics,
	ViewTopics:    ViewThread,
}

var dashboardAssignableViews = []ViewID{
	ViewAgents,
	ViewLiveTail,
	ViewTopics,
	ViewThread,
	ViewSearch,
	ViewTimeline,
	ViewStats,
	ViewBookmarks,
	ViewNotify,
}

type Config struct {
	ProjectID    string
	Root         string
	ForgedAddr   string
	Agent        string
	Operator     bool
	Theme        string
	PollInterval time.Duration
}

type ForgedClient interface {
	Addr() string
	Close() error
}

type netForgedClient struct {
	addr string
	conn net.Conn
}

func (c *netForgedClient) Addr() string {
	return c.addr
}

func (c *netForgedClient) Close() error {
	if c == nil || c.conn == nil {
		return nil
	}
	return c.conn.Close()
}

type Model struct {
	projectID            string
	root                 string
	selfAgent            string
	store                *fmail.Store
	provider             data.MessageProvider
	tuiState             *state.Manager
	notifications        *notificationCenter
	forgedClient         ForgedClient
	forgedAddr           string
	forgedErr            error
	reconnectAttempts    int
	lastReconnectAttempt time.Time
	theme                Theme
	pollInterval         time.Duration

	width        int
	height       int
	showHelp     bool
	toast        string
	toastUntil   time.Time
	flashUntil   time.Time
	spinnerFrame int
	status       statusState
	statusCh     <-chan fmail.Message
	statusCancel func()

	compose composeState
	quick   quickSendState
	layout  *layout.Manager

	layoutWindowCmd bool
	preZenMode      layout.Mode

	viewStack []ViewID
	views     map[ViewID]viewModel
}

type viewModel interface {
	Init() tea.Cmd
	Update(msg tea.Msg) tea.Cmd
	View(width, height int, theme Theme) string
}

type pushViewMsg struct {
	id ViewID
}

type popViewMsg struct{}

type openThreadMsg struct {
	target  string // topic name or "@agent"
	focusID string // optional message ID to focus within the thread
}

func pushViewCmd(id ViewID) tea.Cmd {
	return func() tea.Msg {
		return pushViewMsg{id: id}
	}
}

func popViewCmd() tea.Cmd {
	return func() tea.Msg {
		return popViewMsg{}
	}
}

func openThreadCmd(target, focusID string) tea.Cmd {
	return func() tea.Msg {
		return openThreadMsg{target: target, focusID: focusID}
	}
}

func NewModel(cfg Config) (*Model, error) {
	normalized, err := cfg.normalize()
	if err != nil {
		return nil, err
	}

	root, err := resolveRoot(normalized.Root)
	if err != nil {
		return nil, err
	}
	store, err := fmail.NewStore(root)
	if err != nil {
		return nil, fmt.Errorf("init store: %w", err)
	}
	if err := store.EnsureRoot(); err != nil {
		return nil, fmt.Errorf("ensure store root: %w", err)
	}

	projectID := strings.TrimSpace(normalized.ProjectID)
	if projectID == "" {
		projectID, err = fmail.DeriveProjectID(root)
		if err != nil {
			return nil, fmt.Errorf("derive project id: %w", err)
		}
	}
	if _, err := store.EnsureProject(projectID); err != nil {
		return nil, fmt.Errorf("ensure project: %w", err)
	}

	selfAgent := resolveSelfAgent(normalized.Agent)
	provider, err := buildProvider(root, normalized.ForgedAddr, selfAgent)
	if err != nil {
		return nil, fmt.Errorf("init data provider: %w", err)
	}

	forgedClient, err := connectForged(normalized.ForgedAddr)
	if err != nil {
		// Non-fatal: dashboard can still run in polling mode.
		forgedClient = nil
	}

	initialView := ViewDashboard
	if normalized.Operator {
		initialView = ViewOperator
	}

	m := &Model{
		projectID:    projectID,
		root:         root,
		selfAgent:    selfAgent,
		store:        store,
		provider:     provider,
		tuiState:     state.New(filepath.Join(root, ".fmail", "tui-state.json")),
		forgedClient: forgedClient,
		forgedAddr:   normalized.ForgedAddr,
		forgedErr:    err,
		theme:        Theme(normalized.Theme),
		pollInterval: normalized.PollInterval,
		quick: quickSendState{
			historyIndex: -1,
		},
		layout:     layout.NewManager(),
		preZenMode: layout.ModeSplit,
		viewStack:  []ViewID{initialView},
		views:      make(map[ViewID]viewModel),
	}
	// Non-fatal: state can be created later; fall back to in-memory defaults.
	_ = m.tuiState.Load()
	if prefTheme := strings.TrimSpace(m.tuiState.Theme()); prefTheme != "" {
		if _, ok := styles.Themes[prefTheme]; ok {
			m.theme = Theme(prefTheme)
		}
	}
	if normalized.Operator {
		m.layout.SetMode(layout.ModeSingle)
	}
	m.notifications = newNotificationCenter(selfAgent, m.tuiState)
	m.status.notificationsUnread = m.notifications.UnreadCount()
	m.restoreLayoutPreferences()
	m.initViews()
	return m, nil
}

func Run(cfg Config) error {
	model, err := NewModel(cfg)
	if err != nil {
		return err
	}
	defer model.Close()

	program := tea.NewProgram(model, tea.WithAltScreen())
	_, err = program.Run()
	return err
}

func (m *Model) Close() error {
	if m != nil && m.statusCancel != nil {
		m.statusCancel()
		m.statusCancel = nil
	}
	for _, view := range m.views {
		if closer, ok := view.(interface{ Close() }); ok {
			closer.Close()
		}
	}
	if m != nil && m.tuiState != nil {
		_ = m.tuiState.Close()
	}
	if m == nil || m.forgedClient == nil {
		return nil
	}
	return m.forgedClient.Close()
}

func (m *Model) Init() tea.Cmd {
	cmds := make([]tea.Cmd, 0, 2)
	if view := m.activeView(); view != nil {
		cmds = append(cmds, view.Init())
	}
	cmds = append(cmds, m.statusInitCmd())
	return tea.Batch(cmds...)
}

func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch typed := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = typed.Width
		m.height = typed.Height
		return m, nil
	case openThreadMsg:
		if view := m.views[ViewThread]; view != nil {
			if setter, ok := view.(interface {
				SetTarget(string) tea.Cmd
			}); ok {
				if typed.focusID != "" {
					if focuser, ok := view.(interface{ SetFocus(string) }); ok {
						focuser.SetFocus(typed.focusID)
					}
				}
				return m, setter.SetTarget(typed.target)
			}
		}
		return m, nil
	case pushViewMsg:
		m.pushView(typed.id)
		if view := m.activeView(); view != nil {
			return m, view.Init()
		}
		return m, nil
	case popViewMsg:
		m.popView()
		if view := m.activeView(); view != nil {
			return m, view.Init()
		}
		return m, nil
	case composeSendResultMsg:
		return m, m.handleComposeSendResult(typed)
	case spinnerTickMsg:
		if m.compose.sending || m.quick.sending {
			m.spinnerFrame++
			return m, spinnerTickCmd()
		}
		return m, nil
	case statusTickMsg:
		now := time.Now().UTC()
		needProbe, needMetrics := m.status.onTick(now)
		cmds := []tea.Cmd{statusTickCmd()}
		if needProbe {
			cmds = append(cmds, m.statusProbeCmd())
		}
		if needMetrics {
			cmds = append(cmds, m.statusMetricsCmd())
		}
		return m, tea.Batch(cmds...)
	case statusIncomingMsg:
		m.status.record(typed.msg, time.Now().UTC())
		cmds := []tea.Cmd{m.waitForStatusMsgCmd()}
		if m.notifications != nil {
			if actions, ok := m.notifications.ProcessMessage(typed.msg); ok {
				m.status.notificationsUnread = m.notifications.UnreadCount()
				if actions.Bell {
					cmds = append(cmds, bellCmd())
				}
				if actions.Flash {
					until := time.Now().UTC().Add(500 * time.Millisecond)
					cmds = append(cmds,
						func() tea.Msg { return flashHeaderMsg{until: until} },
						tea.Tick(500*time.Millisecond, func(time.Time) tea.Msg { return liveTailFlashClearMsg{until: until} }),
					)
				}
			}
		}
		return m, tea.Batch(cmds...)
	case statusMetricsMsg:
		m.status.applyMetrics(typed)
		return m, nil
	case statusProbeMsg:
		m.status.applyProbe(typed, time.Now().UTC())
		return m, nil
	case flashHeaderMsg:
		m.flashUntil = typed.until
		return m, nil
	case liveTailFlashClearMsg:
		if !m.flashUntil.IsZero() && !typed.until.IsZero() && (m.flashUntil.Equal(typed.until) || m.flashUntil.Before(typed.until)) {
			m.flashUntil = time.Time{}
		}
		return m, nil
	case tea.KeyMsg:
		if cmd, handled := m.handleGlobalKey(typed); handled {
			return m, cmd
		}
	}

	if focused := m.focusedView(); focused != nil {
		return m, focused.Update(msg)
	}
	return m, nil
}

func (m *Model) View() string {
	if m.activeView() != nil {
		zen := m.effectiveLayoutMode() == layout.ModeZen
		if zen {
			body := m.renderLayoutBody(m.height)
			if m.compose.active {
				body = m.renderComposeOverlay(m.width, m.height, m.theme)
			}
			if m.showHelp {
				body = m.renderHelpOverlay(m.width, m.height, m.theme)
			}
			return body
		}

		header := m.renderHeader()
		footer := m.renderFooter()
		contentHeight := m.height - lipgloss.Height(header) - lipgloss.Height(footer)
		if contentHeight < 0 {
			contentHeight = 0
		}
		body := m.renderLayoutBody(contentHeight)
		if m.compose.active {
			body = m.renderComposeOverlay(m.width, contentHeight, m.theme)
		}
		if m.showHelp {
			body = m.renderHelpOverlay(m.width, contentHeight, m.theme)
		}

		lines := []string{header, body}
		if !m.showHelp && m.quick.active {
			lines = append(lines, m.renderQuickSendBar(m.width, m.theme))
		}
		if !m.showHelp {
			if toast := m.renderToast(m.width, m.theme); strings.TrimSpace(toast) != "" {
				lines = append(lines, toast)
			}
		}
		lines = append(lines, footer)
		return lipgloss.JoinVertical(lipgloss.Left, lines...)
	}
	return "no active view"
}

func (m *Model) handleGlobalKey(msg tea.KeyMsg) (tea.Cmd, bool) {
	if m.activeViewID() == ViewTimeline {
		if timeline, ok := m.views[ViewTimeline].(*timelineView); ok {
			if timeline.wantsKey(msg.String()) {
				return nil, false
			}
		}
	}

	if m.layoutWindowCmd {
		if cmd, handled := m.handleLayoutWindowKey(msg); handled {
			return cmd, true
		}
	}

	// Help overlay: Esc closes; any other key closes then continues.
	if m.showHelp {
		switch msg.String() {
		case "?":
			m.showHelp = false
			return nil, true
		case "esc":
			m.showHelp = false
			return nil, true
		default:
			m.showHelp = false
		}
	}

	if m.activeViewID() == ViewOperator {
		switch msg.String() {
		case "q":
			return tea.Quit, true
		case "ctrl+c":
			return tea.Quit, true
		case "?":
			m.showHelp = !m.showHelp
			return nil, true
		default:
			return nil, false
		}
	}

	if cmd, handled := m.handleComposerKey(msg); handled {
		return cmd, true
	}

	switch msg.String() {
	case "tab":
		switch m.effectiveLayoutMode() {
		case layout.ModeSplit, layout.ModeDashboard:
			m.layout.CycleFocus(m.currentLayoutSpec(m.contentHeightForLayout()))
			m.saveLayoutPreferences()
			return nil, true
		}
	case "|":
		if m.effectiveLayoutMode() == layout.ModeSplit {
			m.layout.AdjustSplit(0.1)
			m.saveLayoutPreferences()
			return nil, true
		}
	case "ctrl+\\":
		if m.layout != nil {
			m.layout.SetMode(layout.ModeSplit)
			m.layout.ToggleSplitCollapsed()
			m.layout.SetExpanded(false)
			m.saveLayoutPreferences()
			return nil, true
		}
	case "ctrl+w":
		m.layoutWindowCmd = true
		return nil, true
	case "ctrl+g":
		if m.layout != nil {
			m.layout.SetMode(layout.ModeDashboard)
			grid := m.layout.CycleGrid()
			m.saveLayoutPreferences()
			m.toast = fmt.Sprintf("grid: %s", grid)
			m.toastUntil = time.Now().UTC().Add(2 * time.Second)
			return nil, true
		}
	case "ctrl+z":
		m.toggleZenLayout()
		m.saveLayoutPreferences()
		return nil, true
	case "ctrl+1":
		m.cycleDashboardSlot(0)
		return nil, true
	case "ctrl+2":
		m.cycleDashboardSlot(1)
		return nil, true
	case "ctrl+3":
		m.cycleDashboardSlot(2)
		return nil, true
	case "ctrl+4":
		m.cycleDashboardSlot(3)
		return nil, true
	}

	switch msg.String() {
	case "esc":
		m.layoutWindowCmd = false
		return popViewCmd(), true
	case "n":
		return nil, m.maybeOpenComposeForNewMessage()
	case "r":
		if m.maybeOpenComposeReply(false) {
			return nil, true
		}
	case "R":
		if m.maybeOpenComposeReply(true) {
			return nil, true
		}
	case ":":
		m.openQuickSendBar()
		return nil, true
	case "ctrl+r":
		if view := m.activeView(); view != nil {
			return view.Init(), true
		}
		return nil, true
	case "ctrl+t":
		m.theme = nextTheme(m.theme)
		if m.tuiState != nil {
			m.tuiState.UpdatePreferences(func(p *state.Preferences) {
				p.Theme = string(m.theme)
			})
		}
		m.toast = fmt.Sprintf("theme: %s", m.theme)
		m.toastUntil = time.Now().UTC().Add(2 * time.Second)
		return nil, true
	case "ctrl+b":
		return pushViewCmd(ViewBookmarks), true
	case "ctrl+n":
		return pushViewCmd(ViewNotify), true
	case "1":
		return pushViewCmd(ViewDashboard), true
	case "2":
		return pushViewCmd(ViewTopics), true
	case "3":
		return pushViewCmd(ViewAgents), true
	case "/":
		if m.activeViewID() != ViewTopics && m.activeViewID() != ViewAgents {
			return pushViewCmd(ViewSearch), true
		}
	case "q":
		return tea.Quit, true
	case "ctrl+c":
		return tea.Quit, true
	case "?":
		m.showHelp = !m.showHelp
		return nil, true
	}

	if next, ok := viewSwitchKeys[msg.String()]; ok {
		m.pushView(next)
		if view := m.activeView(); view != nil {
			return view.Init(), true
		}
		return nil, true
	}
	return nil, false
}

func (m *Model) handleLayoutWindowKey(msg tea.KeyMsg) (tea.Cmd, bool) {
	m.layoutWindowCmd = false
	if m == nil || m.layout == nil {
		return nil, false
	}
	spec := m.currentLayoutSpec(m.contentHeightForLayout())
	switch msg.String() {
	case "+", "=":
		m.layout.SetMode(layout.ModeSplit)
		m.layout.AdjustSplit(0.1)
	case "-":
		m.layout.SetMode(layout.ModeSplit)
		m.layout.AdjustSplit(-0.1)
	case "h":
		m.layout.MoveFocusHorizontal(spec, -1)
	case "l":
		m.layout.MoveFocusHorizontal(spec, 1)
	case "j":
		m.layout.MoveFocusVertical(spec, 1)
	case "k":
		m.layout.MoveFocusVertical(spec, -1)
	case "o":
		m.layout.ToggleExpanded()
	default:
		return nil, false
	}
	m.saveLayoutPreferences()
	return nil, true
}

func (m *Model) effectiveLayoutMode() layout.Mode {
	if m == nil || m.layout == nil {
		return layout.ModeSingle
	}
	width := m.width
	if width <= 0 {
		width = 120
	}
	return m.layout.EffectiveMode(width)
}

func (m *Model) contentHeightForLayout() int {
	if m == nil {
		return 1
	}
	if m.effectiveLayoutMode() == layout.ModeZen {
		return maxInt(1, m.height)
	}
	return maxInt(1, m.height-2)
}

func (m *Model) currentLayoutSpec(contentHeight int) layout.Spec {
	if contentHeight <= 0 {
		contentHeight = 1
	}
	width := m.width
	if width <= 0 {
		width = 120
	}
	primary, secondary := m.layoutPairViews()
	spec := layout.Spec{
		Width:     width,
		Height:    contentHeight,
		Primary:   string(primary),
		Secondary: string(secondary),
	}
	if m.layout != nil {
		spec.Dashboard = m.layout.DashboardViews()
	}
	return spec
}

func (m *Model) layoutPairViews() (ViewID, ViewID) {
	secondary := m.activeViewID()
	primary := ViewTopics
	if len(m.viewStack) >= 2 {
		primary = m.viewStack[len(m.viewStack)-2]
	}
	if primary == secondary {
		switch secondary {
		case ViewThread:
			primary = ViewTopics
		case ViewAgents:
			primary = ViewTopics
		case ViewLiveTail:
			primary = ViewTopics
		case ViewSearch:
			primary = ViewTopics
		default:
			primary = ViewAgents
		}
	}
	if _, ok := m.views[primary]; !ok {
		primary = ViewTopics
	}
	if _, ok := m.views[secondary]; !ok {
		secondary = ViewDashboard
	}
	return primary, secondary
}

func (m *Model) renderLayoutBody(contentHeight int) string {
	if m == nil {
		return ""
	}
	if m.layout == nil {
		if active := m.activeView(); active != nil {
			return active.View(m.width, contentHeight, m.theme)
		}
		return ""
	}
	spec := m.currentLayoutSpec(contentHeight)
	panes := m.layout.Panes(spec)
	if len(panes) == 0 {
		if active := m.activeView(); active != nil {
			return active.View(m.width, contentHeight, m.theme)
		}
		return ""
	}

	switch m.layout.EffectiveMode(spec.Width) {
	case layout.ModeSplit:
		return m.renderSplitPanes(spec, panes)
	case layout.ModeDashboard:
		return m.renderDashboardPanes(spec, panes)
	default:
		return m.renderPane(panes[0])
	}
}

func (m *Model) renderSplitPanes(spec layout.Spec, panes []layout.Pane) string {
	if len(panes) <= 1 {
		return m.renderPane(panes[0])
	}
	left := m.renderPane(panes[0])
	right := m.renderPane(panes[1])
	divider := m.verticalDivider(maxInt(panes[0].Height, panes[1].Height))
	joined := lipgloss.JoinHorizontal(lipgloss.Top, left, divider, right)
	return lipgloss.NewStyle().Width(spec.Width).Height(spec.Height).Render(joined)
}

func (m *Model) renderDashboardPanes(spec layout.Spec, panes []layout.Pane) string {
	if len(panes) <= 1 {
		return m.renderPane(panes[0])
	}
	switch len(panes) {
	case 2:
		if panes[0].Y == panes[1].Y {
			return lipgloss.NewStyle().Width(spec.Width).Height(spec.Height).Render(
				lipgloss.JoinHorizontal(lipgloss.Top, m.renderPane(panes[0]), m.verticalDivider(maxInt(panes[0].Height, panes[1].Height)), m.renderPane(panes[1])),
			)
		}
		return lipgloss.NewStyle().Width(spec.Width).Height(spec.Height).Render(
			lipgloss.JoinVertical(lipgloss.Left, m.renderPane(panes[0]), m.horizontalDivider(spec.Width), m.renderPane(panes[1])),
		)
	case 3:
		if panes[0].Y == panes[1].Y && panes[1].Y == panes[2].Y {
			out := m.renderPane(panes[0])
			for _, pane := range panes[1:] {
				out = lipgloss.JoinHorizontal(lipgloss.Top, out, m.verticalDivider(spec.Height), m.renderPane(pane))
			}
			return lipgloss.NewStyle().Width(spec.Width).Height(spec.Height).Render(out)
		}
		out := m.renderPane(panes[0])
		for _, pane := range panes[1:] {
			out = lipgloss.JoinVertical(lipgloss.Left, out, m.horizontalDivider(spec.Width), m.renderPane(pane))
		}
		return lipgloss.NewStyle().Width(spec.Width).Height(spec.Height).Render(out)
	default:
		top := lipgloss.JoinHorizontal(lipgloss.Top, m.renderPane(panes[0]), m.verticalDivider(maxInt(panes[0].Height, panes[1].Height)), m.renderPane(panes[1]))
		bottom := lipgloss.JoinHorizontal(lipgloss.Top, m.renderPane(panes[2]), m.verticalDivider(maxInt(panes[2].Height, panes[3].Height)), m.renderPane(panes[3]))
		return lipgloss.NewStyle().Width(spec.Width).Height(spec.Height).Render(
			lipgloss.JoinVertical(lipgloss.Left, top, m.horizontalDivider(spec.Width), bottom),
		)
	}
}

func (m *Model) renderPane(pane layout.Pane) string {
	if pane.Width <= 0 || pane.Height <= 0 {
		return ""
	}
	view := m.viewForLayoutID(pane.ViewID)
	if view == nil {
		view = m.activeView()
	}
	minW, minH := 24, 6
	if ms, ok := view.(interface{ MinSize() (int, int) }); ok {
		minW, minH = ms.MinSize()
	}
	if pane.Width < minW || pane.Height < minH {
		return m.renderPaneTooSmall(pane, minW, minH)
	}

	content := view.View(pane.Width, pane.Height, m.theme)
	return lipgloss.NewStyle().Width(pane.Width).Height(pane.Height).Render(content)
}

func (m *Model) renderPaneTooSmall(pane layout.Pane, minW, minH int) string {
	label := fmt.Sprintf("pane too small (%dx%d, need %dx%d)", pane.Width, pane.Height, minW, minH)
	content := truncateVis(label, maxInt(1, pane.Width-2))
	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(themePalette(m.theme).Base.Muted)).
		Width(pane.Width).
		Height(pane.Height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(content)
}

func (m *Model) viewForLayoutID(id string) viewModel {
	if m == nil {
		return nil
	}
	viewID := ViewID(strings.TrimSpace(id))
	if viewID == "" {
		return nil
	}
	return m.views[viewID]
}

func (m *Model) verticalDivider(height int) string {
	if height <= 0 {
		return ""
	}
	lines := make([]string, height)
	style := lipgloss.NewStyle().Foreground(lipgloss.Color(themePalette(m.theme).Borders.Divider))
	for i := range lines {
		lines[i] = style.Render("│")
	}
	return strings.Join(lines, "\n")
}

func (m *Model) horizontalDivider(width int) string {
	if width <= 0 {
		return ""
	}
	style := lipgloss.NewStyle().Foreground(lipgloss.Color(themePalette(m.theme).Borders.Divider))
	return style.Render(strings.Repeat("─", width))
}

func (m *Model) focusedView() viewModel {
	if m == nil {
		return nil
	}
	if m.layout == nil {
		return m.activeView()
	}
	panes := m.layout.Panes(m.currentLayoutSpec(m.contentHeightForLayout()))
	if len(panes) == 0 {
		return m.activeView()
	}
	for _, pane := range panes {
		if !pane.Focused {
			continue
		}
		if view := m.viewForLayoutID(pane.ViewID); view != nil {
			return view
		}
	}
	return m.activeView()
}

func (m *Model) restoreLayoutPreferences() {
	if m == nil || m.layout == nil || m.tuiState == nil {
		return
	}
	pref := m.tuiState.Preferences()
	hasLayoutPrefs := pref.DefaultLayout != "" ||
		pref.LayoutSplitRatio > 0 ||
		pref.LayoutSplitCollapsed ||
		pref.LayoutFocus != 0 ||
		pref.LayoutExpanded ||
		pref.DashboardGrid != "" ||
		len(pref.DashboardViews) > 0
	if !hasLayoutPrefs {
		return
	}
	if pref.DefaultLayout != "" {
		m.layout.SetMode(layout.ParseMode(pref.DefaultLayout))
	}
	if pref.LayoutSplitRatio > 0 {
		m.layout.SetSplitRatio(pref.LayoutSplitRatio)
	}
	m.layout.SetSplitCollapsed(pref.LayoutSplitCollapsed)
	// Preserve layout manager default focus when no layout preferences are set.
	if pref.LayoutFocus != 0 ||
		pref.DefaultLayout != "" ||
		pref.LayoutSplitRatio > 0 ||
		pref.LayoutSplitCollapsed ||
		pref.LayoutExpanded ||
		pref.DashboardGrid != "" ||
		len(pref.DashboardViews) > 0 {
		m.layout.SetFocus(pref.LayoutFocus)
	}
	m.layout.SetExpanded(pref.LayoutExpanded)
	if pref.DashboardGrid != "" {
		m.layout.SetGrid(layout.ParseGrid(pref.DashboardGrid))
	}
	if len(pref.DashboardViews) > 0 {
		var views [4]string
		copy(views[:], pref.DashboardViews)
		m.layout.SetDashboardViews(views)
	}
	if mode := m.layout.Mode(); mode != layout.ModeZen {
		m.preZenMode = mode
	}
}

func (m *Model) saveLayoutPreferences() {
	if m == nil || m.layout == nil || m.tuiState == nil {
		return
	}
	dashboard := m.layout.DashboardViews()
	dashList := make([]string, 0, len(dashboard))
	for _, id := range dashboard {
		if strings.TrimSpace(id) == "" {
			continue
		}
		dashList = append(dashList, id)
	}
	m.tuiState.UpdatePreferences(func(p *state.Preferences) {
		p.DefaultLayout = string(m.layout.Mode())
		p.LayoutSplitRatio = m.layout.SplitRatio()
		p.LayoutSplitCollapsed = m.layout.SplitCollapsed()
		p.LayoutFocus = m.layout.Focus()
		p.LayoutExpanded = m.layout.Expanded()
		p.DashboardGrid = string(m.layout.Grid())
		p.DashboardViews = append([]string(nil), dashList...)
	})
}

func (m *Model) toggleZenLayout() {
	if m == nil || m.layout == nil {
		return
	}
	if m.layout.Mode() == layout.ModeZen {
		next := m.preZenMode
		if next == "" || next == layout.ModeZen {
			next = layout.ModeSplit
		}
		m.layout.SetMode(next)
		return
	}
	current := m.layout.Mode()
	if current == "" || current == layout.ModeZen {
		current = layout.ModeSplit
	}
	m.preZenMode = current
	m.layout.SetMode(layout.ModeZen)
}

func (m *Model) cycleDashboardSlot(slot int) {
	if m == nil || m.layout == nil || slot < 0 || slot > 3 {
		return
	}
	currentViews := m.layout.DashboardViews()
	current := ViewID(currentViews[slot])
	next := dashboardAssignableViews[0]
	for idx, candidate := range dashboardAssignableViews {
		if candidate == current {
			next = dashboardAssignableViews[(idx+1)%len(dashboardAssignableViews)]
			break
		}
	}
	m.layout.SetMode(layout.ModeDashboard)
	m.layout.SetDashboardView(slot, string(next))
	m.saveLayoutPreferences()
	m.toast = fmt.Sprintf("dashboard slot %d: %s", slot+1, next)
	m.toastUntil = time.Now().UTC().Add(2 * time.Second)
}

func nextTheme(current Theme) Theme {
	switch current {
	case ThemeDefault:
		return ThemeHighContrast
	default:
		return ThemeDefault
	}
}

func (m *Model) activeView() viewModel {
	id := m.activeViewID()
	return m.views[id]
}

func (m *Model) activeViewID() ViewID {
	if len(m.viewStack) == 0 {
		return ViewDashboard
	}
	return m.viewStack[len(m.viewStack)-1]
}

func (m *Model) pushView(id ViewID) {
	if id == "" {
		return
	}
	if _, ok := m.views[id]; !ok {
		return
	}
	if m.activeViewID() == id {
		return
	}
	m.viewStack = append(m.viewStack, id)
}

func (m *Model) popView() {
	if len(m.viewStack) <= 1 {
		return
	}
	m.viewStack = m.viewStack[:len(m.viewStack)-1]
}

func (m *Model) initViews() {
	m.views[ViewDashboard] = newDashboardView(m.root, m.projectID, m.provider, m.notifications)
	m.views[ViewTopics] = newTopicsView(m.root, m.provider, m.tuiState)
	m.views[ViewThread] = newThreadView(m.root, m.provider, m.tuiState)
	m.views[ViewAgents] = newAgentsView(m.root, m.provider)
	m.views[ViewOperator] = newOperatorView(m.root, m.projectID, m.selfAgent, m.store, m.provider, m.tuiState)
	m.views[ViewSearch] = newSearchView(m.root, m.selfAgent, m.provider, m.tuiState)
	m.views[ViewLiveTail] = newLiveTailView(m.root, m.selfAgent, m.provider, m.tuiState)
	m.views[ViewTimeline] = newTimelineView(m.root, m.selfAgent, m.provider, m.tuiState)
	m.views[ViewStats] = newStatsView(m.root, m.selfAgent, m.provider)
	m.views[ViewBookmarks] = newBookmarksView(m.root, m.store, m.provider, m.tuiState)
	m.views[ViewNotify] = newNotificationsView(m.selfAgent, m.provider, m.notifications)
}

func (c Config) normalize() (Config, error) {
	c.ProjectID = strings.TrimSpace(c.ProjectID)
	c.Root = strings.TrimSpace(c.Root)
	c.ForgedAddr = strings.TrimSpace(c.ForgedAddr)
	c.Agent = strings.TrimSpace(c.Agent)
	if c.PollInterval <= 0 {
		c.PollInterval = defaultPollInterval
	}
	if strings.TrimSpace(c.Theme) == "" {
		c.Theme = string(ThemeDefault)
	}
	switch Theme(c.Theme) {
	case ThemeDefault, ThemeHighContrast:
	default:
		return Config{}, fmt.Errorf("invalid theme %q", c.Theme)
	}
	return c, nil
}

func resolveRoot(root string) (string, error) {
	if strings.TrimSpace(root) == "" {
		discovered, err := fmail.DiscoverProjectRoot("")
		if err != nil {
			return "", fmt.Errorf("resolve project root: %w", err)
		}
		return discovered, nil
	}
	abs, err := filepath.Abs(root)
	if err != nil {
		return "", err
	}
	return abs, nil
}

func connectForged(addr string) (ForgedClient, error) {
	trimmed := strings.TrimSpace(addr)
	if trimmed == "" {
		return nil, nil
	}

	network, target := forgedEndpoint(trimmed)
	conn, err := net.DialTimeout(network, target, forgedDialTimeout)
	if err != nil {
		return nil, err
	}
	return &netForgedClient{addr: trimmed, conn: conn}, nil
}

func forgedEndpoint(addr string) (network string, target string) {
	switch {
	case strings.HasPrefix(addr, "unix://"):
		return "unix", strings.TrimPrefix(addr, "unix://")
	case strings.HasPrefix(addr, "tcp://"):
		return "tcp", strings.TrimPrefix(addr, "tcp://")
	case strings.Contains(addr, string(os.PathSeparator)):
		return "unix", addr
	default:
		return "tcp", addr
	}
}

func resolveSelfAgent(agent string) string {
	agent = strings.TrimSpace(agent)
	if agent == "" {
		agent = strings.TrimSpace(os.Getenv("FMAIL_AGENT"))
	}
	if agent == "" {
		agent = fmt.Sprintf("tui-%d", os.Getpid())
	}
	normalized, err := fmail.NormalizeAgentName(agent)
	if err != nil {
		return defaultSelfAgent
	}
	return normalized
}

type placeholderView struct {
	id      ViewID
	title   string
	lastKey string
}

func newPlaceholderView(id ViewID, title string) *placeholderView {
	return &placeholderView{
		id:    id,
		title: title,
	}
}

func (p *placeholderView) Init() tea.Cmd {
	return nil
}

func (p *placeholderView) Update(msg tea.Msg) tea.Cmd {
	keyMsg, ok := msg.(tea.KeyMsg)
	if !ok {
		return nil
	}
	p.lastKey = keyMsg.String()

	if keyMsg.String() == "enter" {
		if next, ok := defaultEnterRoute[p.id]; ok {
			return pushViewCmd(next)
		}
	}
	if keyMsg.String() == "backspace" || keyMsg.String() == "esc" {
		return popViewCmd()
	}
	return nil
}

func (p *placeholderView) View(_ int, _ int, _ Theme) string {
	return fmt.Sprintf("%s view\npress enter for drill-down where available", p.title)
}
