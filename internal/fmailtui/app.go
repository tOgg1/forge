package fmailtui

import (
	"fmt"
	"net"
	"os"
	"path/filepath"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/tOgg1/forge/internal/fmail"
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
	ViewSearch    ViewID = "search"
	ViewTimeline  ViewID = "timeline"
)

var viewSwitchKeys = map[string]ViewID{
	"d": ViewDashboard,
	"t": ViewTopics,
	"r": ViewThread,
	"a": ViewAgents,
	"s": ViewSearch,
	"l": ViewTimeline,
}

var defaultEnterRoute = map[ViewID]ViewID{
	ViewDashboard: ViewTopics,
	ViewTopics:    ViewThread,
}

type Config struct {
	ProjectID    string
	Root         string
	ForgedAddr   string
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
	projectID    string
	root         string
	store        *fmail.Store
	forgedClient ForgedClient
	theme        Theme
	pollInterval time.Duration

	width    int
	height   int
	showHelp bool

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

	forgedClient, err := connectForged(normalized.ForgedAddr)
	if err != nil {
		return nil, fmt.Errorf("connect forged: %w", err)
	}

	m := &Model{
		projectID:    projectID,
		root:         root,
		store:        store,
		forgedClient: forgedClient,
		theme:        Theme(normalized.Theme),
		pollInterval: normalized.PollInterval,
		viewStack:    []ViewID{ViewDashboard},
		views:        make(map[ViewID]viewModel),
	}
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
	if m == nil || m.forgedClient == nil {
		return nil
	}
	return m.forgedClient.Close()
}

func (m *Model) Init() tea.Cmd {
	if view := m.activeView(); view != nil {
		return view.Init()
	}
	return nil
}

func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch typed := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = typed.Width
		m.height = typed.Height
		return m, nil
	case pushViewMsg:
		m.pushView(typed.id)
		return m, nil
	case popViewMsg:
		m.popView()
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
	var b strings.Builder
	b.WriteString(fmt.Sprintf("fmail tui  project=%s  root=%s\n", m.projectID, m.root))
	b.WriteString(fmt.Sprintf("view=%s  size=%dx%d  theme=%s\n", m.activeViewID(), m.width, m.height, m.theme))
	if m.forgedClient != nil {
		b.WriteString(fmt.Sprintf("forged=%s  poll=%s\n", m.forgedClient.Addr(), m.pollInterval))
	} else {
		b.WriteString(fmt.Sprintf("forged=offline  poll=%s\n", m.pollInterval))
	}

	if m.showHelp {
		b.WriteString("keys: q/ctrl+c quit, ? help, esc/backspace back, d/t/r/a/s/l switch view\n\n")
	}

	if active := m.activeView(); active != nil {
		b.WriteString(active.View(m.width, m.height, m.theme))
		return b.String()
	}
	b.WriteString("no active view")
	return b.String()
}

func (m *Model) handleGlobalKey(msg tea.KeyMsg) (tea.Cmd, bool) {
	switch msg.String() {
	case "q", "ctrl+c":
		return tea.Quit, true
	case "?":
		m.showHelp = !m.showHelp
		return nil, true
	case "esc", "backspace":
		m.popView()
		return nil, true
	}

	if next, ok := viewSwitchKeys[msg.String()]; ok {
		m.pushView(next)
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
	m.views[ViewDashboard] = newPlaceholderView(ViewDashboard, "Dashboard")
	m.views[ViewTopics] = newPlaceholderView(ViewTopics, "Topics")
	m.views[ViewThread] = newPlaceholderView(ViewThread, "Thread")
	m.views[ViewAgents] = newPlaceholderView(ViewAgents, "Agents")
	m.views[ViewSearch] = newPlaceholderView(ViewSearch, "Search")
	m.views[ViewTimeline] = newPlaceholderView(ViewTimeline, "Timeline")
}

func (c Config) normalize() (Config, error) {
	c.ProjectID = strings.TrimSpace(c.ProjectID)
	c.Root = strings.TrimSpace(c.Root)
	c.ForgedAddr = strings.TrimSpace(c.ForgedAddr)
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
