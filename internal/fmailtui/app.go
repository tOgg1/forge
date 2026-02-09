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
	"github.com/tOgg1/forge/internal/fmailtui/state"
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
)

var viewSwitchKeys = map[string]ViewID{
	"o": ViewOperator,
	"t": ViewTopics,
	"r": ViewThread,
	"a": ViewAgents,
	"l": ViewLiveTail,
	"m": ViewTimeline,
	"D": ViewDashboard,
	"S": ViewSearch,
}

var defaultEnterRoute = map[ViewID]ViewID{
	ViewDashboard: ViewTopics,
	ViewTopics:    ViewThread,
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
	spinnerFrame int
	status       statusState
	statusCh     <-chan fmail.Message
	statusCancel func()

	compose composeState
	quick   quickSendState

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
	target string // topic name or "@agent"
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

func openThreadCmd(target string) tea.Cmd {
	return func() tea.Msg {
		return openThreadMsg{target: target}
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
		viewStack: []ViewID{initialView},
		views:     make(map[ViewID]viewModel),
	}
	// Non-fatal: state can be created later; fall back to in-memory defaults.
	_ = m.tuiState.Load()
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
		return m, m.waitForStatusMsgCmd()
	case statusMetricsMsg:
		m.status.applyMetrics(typed)
		return m, nil
	case statusProbeMsg:
		m.status.applyProbe(typed, time.Now().UTC())
		return m, nil
	case tea.KeyMsg:
		if cmd, handled := m.handleGlobalKey(typed); handled {
			return m, cmd
		}
	}

	if active := m.activeView(); active != nil {
		return m, active.Update(msg)
	}
	return m, nil
}

func (m *Model) View() string {
	if active := m.activeView(); active != nil {
		header := m.renderHeader()
		footer := m.renderFooter()
		contentHeight := m.height - lipgloss.Height(header) - lipgloss.Height(footer)
		if contentHeight < 0 {
			contentHeight = 0
		}
		body := active.View(m.width, contentHeight, m.theme)
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
	case "esc":
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
	m.views[ViewDashboard] = newDashboardView(m.root, m.projectID, m.provider)
	m.views[ViewTopics] = newTopicsView(m.root, m.provider, m.tuiState)
	m.views[ViewThread] = newThreadView(m.root, m.provider, m.tuiState)
	m.views[ViewAgents] = newAgentsView(m.root, m.provider)
	m.views[ViewOperator] = newOperatorView(m.root, m.projectID, m.selfAgent, m.store, m.provider, m.tuiState)
	m.views[ViewSearch] = newPlaceholderView(ViewSearch, "Search")
	m.views[ViewLiveTail] = newPlaceholderView(ViewLiveTail, "Live Tail")
	m.views[ViewTimeline] = newPlaceholderView(ViewTimeline, "Timeline")
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
