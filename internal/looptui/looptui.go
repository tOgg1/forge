package looptui

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/rand"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"syscall"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/loop"
	"github.com/tOgg1/forge/internal/models"
	"github.com/tOgg1/forge/internal/names"
	"github.com/tOgg1/forge/internal/procutil"
)

const (
	defaultRefreshInterval = 2 * time.Second
	defaultLogLines        = 12
	defaultStatusTTL       = 5 * time.Second
	maxTailReadBytes       = 2 * 1024 * 1024
	defaultLogBackfill     = 1200
	maxLogBackfill         = 8000
	logScrollStep          = 20

	minWindowWidth  = 80
	minWindowHeight = 22
)

const (
	multiHeaderRows    = 2
	multiCellGap       = 1
	multiMinCellWidth  = 38
	multiMinCellHeight = 8
)

var filterStatusOptions = []string{"all", "running", "sleeping", "waiting", "stopped", "error"}

// Config controls loop TUI behavior.
type Config struct {
	DataDir          string
	RefreshInterval  time.Duration
	Theme            string
	LogLines         int
	DefaultInterval  time.Duration
	DefaultPrompt    string
	DefaultPromptMsg string
	ConfigFile       string
}

// Run starts the loop TUI.
func Run(database *db.DB, cfg Config) error {
	if cfg.RefreshInterval <= 0 {
		cfg.RefreshInterval = defaultRefreshInterval
	}
	if cfg.LogLines <= 0 {
		cfg.LogLines = defaultLogLines
	}

	model := newModel(database, cfg)
	program := tea.NewProgram(model, tea.WithAltScreen())
	_, err := program.Run()
	return err
}

type uiMode int

const (
	modeMain uiMode = iota
	modeFilter
	modeExpandedLogs
	modeConfirm
	modeWizard
	modeHelp
)

type statusKind int

const (
	statusInfo statusKind = iota
	statusOK
	statusErr
)

type filterFocus int

const (
	filterFocusText filterFocus = iota
	filterFocusStatus
)

type actionType int

const (
	actionNone actionType = iota
	actionStop
	actionKill
	actionDelete
	actionResume
	actionCreate
)

type mainTab int

const (
	tabOverview mainTab = iota
	tabLogs
	tabRuns
	tabMultiLogs
)

var tabOrder = []mainTab{tabOverview, tabLogs, tabRuns, tabMultiLogs}

type logSource int

const (
	logSourceLive logSource = iota
	logSourceLatestRun
	logSourceRunSelection
)

type logLayer int

const (
	logLayerRaw logLayer = iota
	logLayerEvents
	logLayerErrors
	logLayerTools
	logLayerDiff
)

type loopView struct {
	Loop           *models.Loop
	Runs           int
	QueueDepth     int
	ProfileName    string
	ProfileHarness models.Harness
	ProfileAuth    string
	PoolName       string
}

type logTailView struct {
	Lines   []string
	Message string
}

type logDisplay struct {
	Title   string
	Source  string
	Lines   []string
	Message string
	Harness models.Harness
}

type confirmState struct {
	Action actionType
	LoopID string
	Prompt string
}

type wizardValues struct {
	Name          string
	NamePrefix    string
	Count         string
	Pool          string
	Profile       string
	Prompt        string
	PromptMsg     string
	Interval      string
	MaxRuntime    string
	MaxIterations string
	Tags          string
}

type wizardState struct {
	Step   int
	Field  int
	Values wizardValues
	Error  string
}

type model struct {
	db               *db.DB
	dataDir          string
	refreshInterval  time.Duration
	logLines         int
	defaultInterval  time.Duration
	defaultPrompt    string
	defaultPromptMsg string
	configFile       string
	palette          tuiPalette

	width  int
	height int

	loops       []loopView
	filtered    []loopView
	selectedID  string
	selectedIdx int
	selectedLog logTailView
	runHistory  []runView
	selectedRun int
	tab         mainTab
	logSource   logSource
	logLayer    logLayer
	logScroll   int
	focusRight  bool
	pinned      map[string]struct{}
	layoutIdx   int
	multiPage   int
	multiLogs   map[string]logTailView

	mode        uiMode
	helpReturn  uiMode
	filterText  string
	filterState string
	filterFocus filterFocus
	confirm     *confirmState
	wizard      wizardState

	err           error
	statusText    string
	statusKind    statusKind
	statusExpires time.Time
	actionBusy    bool
	quitting      bool
}

type refreshMsg struct {
	loops      []loopView
	selectedID string
	selected   logTailView
	runs       []runView
	multiLogs  map[string]logTailView
	err        error
}

type tickMsg struct{}

type actionRequest struct {
	Kind        actionType
	LoopID      string
	ForceDelete bool
	Wizard      wizardValues
}

type actionResultMsg struct {
	Kind           actionType
	LoopID         string
	SelectedLoopID string
	Message        string
	Err            error
}

var startLoopProcessFn = startLoopProcess

func newModel(database *db.DB, cfg Config) model {
	palette := resolvePalette(cfg.Theme)
	m := model{
		db:               database,
		dataDir:          cfg.DataDir,
		refreshInterval:  cfg.RefreshInterval,
		logLines:         cfg.LogLines,
		defaultInterval:  cfg.DefaultInterval,
		defaultPrompt:    cfg.DefaultPrompt,
		defaultPromptMsg: cfg.DefaultPromptMsg,
		configFile:       cfg.ConfigFile,
		palette:          palette,
		mode:             modeMain,
		filterState:      "all",
		filterFocus:      filterFocusText,
		tab:              tabOverview,
		logSource:        logSourceLive,
		logLayer:         logLayerRaw,
		pinned:           make(map[string]struct{}),
		layoutIdx:        layoutIndexFor(2, 2),
		multiPage:        0,
		multiLogs:        make(map[string]logTailView),
	}
	m.wizard = newWizardState(cfg.DefaultInterval, cfg.DefaultPrompt, cfg.DefaultPromptMsg)
	return m
}

func (m model) Init() tea.Cmd {
	return tea.Batch(m.fetchCmd(), m.tickCmd())
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		if m.tab == tabMultiLogs {
			m.clampMultiPage()
		}
		return m, m.fetchCmd()
	case tickMsg:
		if !m.statusExpires.IsZero() && time.Now().After(m.statusExpires) {
			m.statusText = ""
		}
		return m, tea.Batch(m.fetchCmd(), m.tickCmd())
	case refreshMsg:
		m.err = msg.err
		if msg.err == nil {
			m.loops = msg.loops
			oldSelectedID := m.selectedID
			oldSelectedIdx := m.selectedIdx
			m.applyFilters(oldSelectedID, oldSelectedIdx)
			if m.selectedID == msg.selectedID {
				m.selectedLog = msg.selected
			} else if m.selectedID != "" {
				return m, m.fetchCmd()
			} else {
				m.selectedLog = logTailView{}
			}
			m.runHistory = msg.runs
			if len(m.runHistory) == 0 {
				m.selectedRun = 0
				m.logSource = logSourceLive
			} else if m.selectedRun >= len(m.runHistory) {
				m.selectedRun = len(m.runHistory) - 1
			}
			if msg.multiLogs != nil {
				m.multiLogs = msg.multiLogs
			} else {
				m.multiLogs = make(map[string]logTailView)
			}
		}
		return m, nil
	case actionResultMsg:
		m.actionBusy = false
		if msg.Err != nil {
			m.setStatus(statusErr, msg.Err.Error())
			if msg.Kind == actionCreate {
				m.mode = modeWizard
				m.wizard.Error = msg.Err.Error()
			}
			return m, nil
		}

		if msg.Kind == actionCreate {
			m.mode = modeMain
			m.wizard.Error = ""
			if msg.SelectedLoopID != "" {
				m.selectedID = msg.SelectedLoopID
			}
		}

		if msg.Message != "" {
			m.setStatus(statusOK, msg.Message)
		}
		return m, m.fetchCmd()
	case tea.KeyMsg:
		if msg.String() == "ctrl+c" {
			m.quitting = true
			return m, tea.Quit
		}

		switch m.mode {
		case modeFilter:
			return m.updateFilterMode(msg)
		case modeExpandedLogs:
			return m.updateExpandedLogsMode(msg)
		case modeConfirm:
			return m.updateConfirmMode(msg)
		case modeWizard:
			return m.updateWizardMode(msg)
		case modeHelp:
			return m.updateHelpMode(msg)
		default:
			return m.updateMainMode(msg)
		}
	}

	return m, nil
}

func (m model) View() string {
	if m.quitting {
		return ""
	}

	width := m.effectiveWidth()
	height := m.effectiveHeight()

	header := m.renderHeader()
	tabBar := m.renderTabBar(width)
	overhead := 4
	if m.mode == modeFilter || m.mode == modeConfirm || m.mode == modeWizard || m.mode == modeHelp {
		overhead += 3
	}
	if m.statusText != "" {
		overhead++
	}
	paneHeight := maxInt(10, height-overhead)

	var body string
	if m.focusRight {
		body = m.renderRightPane(width, paneHeight)
	} else {
		leftWidth, rightWidth := paneWidthsForTab(width, m.tab)
		leftPane := m.renderLeftPane(leftWidth, paneHeight)
		rightPane := m.renderRightPane(rightWidth, paneHeight)
		body = lipgloss.JoinHorizontal(lipgloss.Top, leftPane, rightPane)
	}

	parts := []string{header, tabBar, body}
	if m.mode == modeFilter {
		parts = append(parts, m.renderFilterBar(width))
	}
	if m.mode == modeConfirm && m.confirm != nil {
		parts = append(parts, m.renderConfirmDialog(width))
	}
	if m.mode == modeWizard {
		parts = append(parts, m.renderWizard(width))
	}
	if m.mode == modeHelp {
		parts = append(parts, m.renderHelpDialog(width))
	}
	if m.statusText != "" {
		parts = append(parts, m.renderStatusLine(width))
	}

	if m.err != nil {
		errStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.Error)).Bold(true)
		parts = append(parts, errStyle.Render("Error: "+m.err.Error()))
	}

	return strings.Join(parts, "\n")
}

func (m model) updateMainMode(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "q":
		m.quitting = true
		return m, tea.Quit
	case "?":
		m.helpReturn = modeMain
		m.mode = modeHelp
		return m, nil
	case "1":
		m.setTab(tabOverview)
		return m, m.fetchCmd()
	case "2":
		m.setTab(tabLogs)
		return m, m.fetchCmd()
	case "3":
		m.setTab(tabRuns)
		return m, m.fetchCmd()
	case "4":
		m.setTab(tabMultiLogs)
		return m, m.fetchCmd()
	case "]":
		m.cycleTab(1)
		return m, m.fetchCmd()
	case "[":
		m.cycleTab(-1)
		return m, m.fetchCmd()
	case "t":
		m.palette = cyclePalette(m.palette.Name, 1)
		m.setStatus(statusInfo, "Theme: "+m.palette.Name)
		return m, nil
	case "z":
		m.focusRight = !m.focusRight
		if m.tab == tabMultiLogs {
			m.clampMultiPage()
		}
		if m.focusRight {
			m.setStatus(statusInfo, "Zen mode: right pane focus")
		} else {
			m.setStatus(statusInfo, "Zen mode: split view")
		}
		if m.tab == tabMultiLogs {
			return m, m.fetchCmd()
		}
		return m, nil
	case "/":
		m.mode = modeFilter
		m.filterFocus = filterFocusText
		return m, nil
	case "j", "down":
		m.moveSelection(1)
		return m, m.fetchCmd()
	case "k", "up":
		m.moveSelection(-1)
		return m, m.fetchCmd()
	case "pgup", "ctrl+u", "u":
		if m.tab == tabLogs || m.tab == tabRuns {
			m.scrollLogs(m.logScrollPageSize())
			return m, m.fetchCmd()
		}
		return m, nil
	case "pgdown", "ctrl+d", "d":
		if m.tab == tabLogs || m.tab == tabRuns {
			m.scrollLogs(-m.logScrollPageSize())
			return m, m.fetchCmd()
		}
		return m, nil
	case "home":
		if m.tab == tabLogs || m.tab == tabRuns {
			m.scrollLogsToTop()
			return m, m.fetchCmd()
		}
		if m.tab == tabMultiLogs {
			m.moveMultiPageToStart()
			return m, m.fetchCmd()
		}
		return m, nil
	case "end":
		if m.tab == tabLogs || m.tab == tabRuns {
			m.scrollLogsToBottom()
			return m, nil
		}
		if m.tab == tabMultiLogs {
			m.moveMultiPageToEnd()
			return m, m.fetchCmd()
		}
		return m, nil
	case "space":
		if view, ok := m.selectedView(); ok && view.Loop != nil {
			m.togglePinned(view.Loop.ID)
		}
		return m, m.fetchCmd()
	case "c":
		m.clearPinned()
		return m, m.fetchCmd()
	case "m":
		if m.tab == tabMultiLogs {
			m.cycleLayout(1)
			m.setStatus(statusInfo, "Layout: "+m.currentLayout().Label())
			return m, m.fetchCmd()
		}
		return m, nil
	case "v":
		if m.tab == tabLogs {
			m.cycleLogSource(1)
			return m, m.fetchCmd()
		}
		return m, nil
	case "x":
		if m.tab == tabLogs || m.tab == tabRuns || m.tab == tabMultiLogs {
			m.cycleLogLayer(1)
			return m, nil
		}
		return m, nil
	case ",":
		if m.tab == tabLogs || m.tab == tabRuns {
			m.moveRunSelection(-1)
			return m, m.fetchCmd()
		}
		if m.tab == tabMultiLogs {
			m.moveMultiPage(-1)
			return m, m.fetchCmd()
		}
		return m, nil
	case ".":
		if m.tab == tabLogs || m.tab == tabRuns {
			m.moveRunSelection(1)
			return m, m.fetchCmd()
		}
		if m.tab == tabMultiLogs {
			m.moveMultiPage(1)
			return m, m.fetchCmd()
		}
		return m, nil
	case "l":
		if _, ok := m.selectedView(); !ok {
			m.setStatus(statusInfo, "No loop selected")
			return m, nil
		}
		m.mode = modeExpandedLogs
		return m, m.fetchCmd()
	case "n":
		m.mode = modeWizard
		m.wizard = newWizardState(m.defaultInterval, m.defaultPrompt, m.defaultPromptMsg)
		return m, nil
	case "r":
		view, ok := m.selectedView()
		if !ok {
			m.setStatus(statusInfo, "No loop selected")
			return m, nil
		}
		return m.runAction(actionRequest{Kind: actionResume, LoopID: view.Loop.ID})
	case "S":
		return m.enterConfirm(actionStop)
	case "K":
		return m.enterConfirm(actionKill)
	case "D":
		return m.enterConfirm(actionDelete)
	default:
		return m, nil
	}
}

func (m model) updateFilterMode(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "q", "esc":
		m.mode = modeMain
		m.filterFocus = filterFocusText
		return m, nil
	case "?":
		m.helpReturn = modeFilter
		m.mode = modeHelp
		return m, nil
	case "tab":
		if m.filterFocus == filterFocusText {
			m.filterFocus = filterFocusStatus
		} else {
			m.filterFocus = filterFocusText
		}
		return m, nil
	}

	if m.filterFocus == filterFocusStatus {
		switch msg.String() {
		case "left", "up", "k":
			m.cycleFilterStatus(-1)
			return m, nil
		case "right", "down", "j", "enter":
			m.cycleFilterStatus(1)
			return m, nil
		default:
			return m, nil
		}
	}

	switch msg.String() {
	case "backspace", "ctrl+h", "delete":
		if m.filterText != "" {
			m.filterText = removeLastRune(m.filterText)
			oldID, oldIdx := m.selectedID, m.selectedIdx
			m.applyFilters(oldID, oldIdx)
			return m, m.fetchCmd()
		}
		return m, nil
	case "space":
		m.filterText += " "
		oldID, oldIdx := m.selectedID, m.selectedIdx
		m.applyFilters(oldID, oldIdx)
		return m, m.fetchCmd()
	default:
		if len(msg.Runes) > 0 {
			m.filterText += string(msg.Runes)
			oldID, oldIdx := m.selectedID, m.selectedIdx
			m.applyFilters(oldID, oldIdx)
			return m, m.fetchCmd()
		}
		return m, nil
	}
}

func (m model) updateExpandedLogsMode(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "q", "esc":
		m.mode = modeMain
		return m, m.fetchCmd()
	case "?":
		m.helpReturn = modeExpandedLogs
		m.mode = modeHelp
		return m, nil
	case "]":
		m.cycleTab(1)
		return m, m.fetchCmd()
	case "[":
		m.cycleTab(-1)
		return m, m.fetchCmd()
	case "t":
		m.palette = cyclePalette(m.palette.Name, 1)
		return m, nil
	case "z":
		m.focusRight = !m.focusRight
		return m, nil
	case "j", "down":
		m.moveSelection(1)
		return m, m.fetchCmd()
	case "k", "up":
		m.moveSelection(-1)
		return m, m.fetchCmd()
	case "v":
		if m.tab == tabLogs {
			m.cycleLogSource(1)
			return m, m.fetchCmd()
		}
		return m, nil
	case "x":
		m.cycleLogLayer(1)
		return m, nil
	case ",":
		m.moveRunSelection(-1)
		return m, m.fetchCmd()
	case ".":
		m.moveRunSelection(1)
		return m, m.fetchCmd()
	case "/":
		m.mode = modeFilter
		m.filterFocus = filterFocusText
		return m, nil
	case "S":
		m.mode = modeMain
		return m.enterConfirm(actionStop)
	case "K":
		m.mode = modeMain
		return m.enterConfirm(actionKill)
	case "D":
		m.mode = modeMain
		return m.enterConfirm(actionDelete)
	case "r":
		view, ok := m.selectedView()
		if !ok {
			m.setStatus(statusInfo, "No loop selected")
			return m, nil
		}
		m.mode = modeMain
		return m.runAction(actionRequest{Kind: actionResume, LoopID: view.Loop.ID})
	default:
		return m, nil
	}
}

func (m model) updateConfirmMode(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	if m.confirm == nil {
		m.mode = modeMain
		return m, nil
	}

	switch msg.String() {
	case "q", "esc", "n", "N", "enter":
		m.mode = modeMain
		m.confirm = nil
		m.setStatus(statusInfo, "Action cancelled")
		return m, nil
	case "?":
		m.helpReturn = modeConfirm
		m.mode = modeHelp
		return m, nil
	case "y", "Y":
		confirm := m.confirm
		m.mode = modeMain
		m.confirm = nil
		req := actionRequest{Kind: confirm.Action, LoopID: confirm.LoopID, ForceDelete: confirm.Action == actionDelete && strings.Contains(confirm.Prompt, "Force delete")}
		return m.runAction(req)
	default:
		return m, nil
	}
}

func (m model) updateWizardMode(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "q", "esc":
		m.mode = modeMain
		m.wizard.Error = ""
		return m, nil
	case "?":
		m.helpReturn = modeWizard
		m.mode = modeHelp
		return m, nil
	case "tab", "down", "j":
		m.wizardNextField()
		return m, nil
	case "shift+tab", "up", "k":
		m.wizardPrevField()
		return m, nil
	case "enter":
		if m.wizard.Step < 4 {
			if err := validateWizardStep(m.wizard.Step, m.wizard.Values, m.defaultInterval); err != nil {
				m.wizard.Error = err.Error()
				return m, nil
			}
			m.wizard.Step++
			m.wizard.Field = 0
			m.wizard.Error = ""
			return m, nil
		}
		return m.runAction(actionRequest{Kind: actionCreate, Wizard: m.wizard.Values})
	case "b", "left":
		if m.wizard.Step > 1 {
			m.wizard.Step--
			m.wizard.Field = 0
			m.wizard.Error = ""
		}
		return m, nil
	case "backspace", "ctrl+h", "delete":
		if m.wizard.Step > 3 {
			return m, nil
		}
		key := wizardFieldKey(m.wizard.Step, m.wizard.Field)
		if key == "" {
			return m, nil
		}
		value := wizardGet(&m.wizard.Values, key)
		wizardSet(&m.wizard.Values, key, removeLastRune(value))
		return m, nil
	case "space":
		if m.wizard.Step > 3 {
			return m, nil
		}
		key := wizardFieldKey(m.wizard.Step, m.wizard.Field)
		if key == "" {
			return m, nil
		}
		wizardSet(&m.wizard.Values, key, wizardGet(&m.wizard.Values, key)+" ")
		return m, nil
	default:
		if m.wizard.Step > 3 || len(msg.Runes) == 0 {
			return m, nil
		}
		key := wizardFieldKey(m.wizard.Step, m.wizard.Field)
		if key == "" {
			return m, nil
		}
		wizardSet(&m.wizard.Values, key, wizardGet(&m.wizard.Values, key)+string(msg.Runes))
		return m, nil
	}
}

func (m model) updateHelpMode(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "q", "esc", "?":
		if m.helpReturn == modeHelp {
			m.mode = modeMain
		} else {
			m.mode = m.helpReturn
		}
		return m, nil
	default:
		return m, nil
	}
}

func (m model) runAction(req actionRequest) (tea.Model, tea.Cmd) {
	if m.actionBusy {
		m.setStatus(statusInfo, "Another action is still running")
		return m, nil
	}

	m.actionBusy = true
	switch req.Kind {
	case actionCreate:
		m.setStatus(statusInfo, "Creating loop(s)...")
	case actionResume:
		m.setStatus(statusInfo, "Resuming loop...")
	case actionStop:
		m.setStatus(statusInfo, "Requesting graceful stop...")
	case actionKill:
		m.setStatus(statusInfo, "Killing loop...")
	case actionDelete:
		m.setStatus(statusInfo, "Deleting loop record...")
	default:
		m.setStatus(statusInfo, "Running action...")
	}
	return m, m.actionCmd(req)
}

func (m model) actionCmd(req actionRequest) tea.Cmd {
	database := m.db
	dataDir := m.dataDir
	configFile := m.configFile
	defaultInterval := m.defaultInterval
	defaultPrompt := m.defaultPrompt
	defaultPromptMsg := m.defaultPromptMsg

	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()

		result := actionResultMsg{Kind: req.Kind, LoopID: req.LoopID}
		var err error
		switch req.Kind {
		case actionResume:
			result.Message, err = resumeLoop(ctx, database, configFile, req.LoopID)
		case actionStop:
			result.Message, err = stopLoop(ctx, database, req.LoopID)
		case actionKill:
			result.Message, err = killLoop(ctx, database, req.LoopID)
		case actionDelete:
			result.Message, err = deleteLoop(ctx, database, req.LoopID, req.ForceDelete)
		case actionCreate:
			result.SelectedLoopID, result.Message, err = createLoops(ctx, database, dataDir, configFile, defaultInterval, defaultPrompt, defaultPromptMsg, req.Wizard)
		default:
			err = errors.New("unsupported action")
		}
		if err != nil {
			result.Err = err
		}
		return result
	}
}

func (m model) enterConfirm(action actionType) (tea.Model, tea.Cmd) {
	view, ok := m.selectedView()
	if !ok {
		m.setStatus(statusInfo, "No loop selected")
		return m, nil
	}

	loopID := loopDisplayID(view.Loop)
	confirm := &confirmState{Action: action, LoopID: view.Loop.ID}
	switch action {
	case actionStop:
		confirm.Prompt = fmt.Sprintf("Stop loop %s after current iteration? [y/N]", loopID)
	case actionKill:
		confirm.Prompt = fmt.Sprintf("Kill loop %s immediately? [y/N]", loopID)
	case actionDelete:
		if view.Loop.State == models.LoopStateStopped {
			confirm.Prompt = fmt.Sprintf("Delete loop record %s? [y/N]", loopID)
		} else {
			confirm.Prompt = fmt.Sprintf("Loop is still running. Force delete record %s? [y/N]", loopID)
		}
	default:
		m.setStatus(statusErr, "Unsupported destructive action")
		return m, nil
	}

	m.confirm = confirm
	m.mode = modeConfirm
	return m, nil
}

func (m *model) moveSelection(delta int) {
	if len(m.filtered) == 0 {
		m.selectedIdx = 0
		m.selectedID = ""
		m.logScroll = 0
		return
	}
	m.selectedIdx += delta
	if m.selectedIdx < 0 {
		m.selectedIdx = 0
	}
	if m.selectedIdx >= len(m.filtered) {
		m.selectedIdx = len(m.filtered) - 1
	}
	m.selectedID = m.filtered[m.selectedIdx].Loop.ID
	m.logScroll = 0
}

func (m *model) cycleFilterStatus(delta int) {
	idx := 0
	for i, candidate := range filterStatusOptions {
		if candidate == m.filterState {
			idx = i
			break
		}
	}
	idx += delta
	if idx < 0 {
		idx = len(filterStatusOptions) - 1
	}
	if idx >= len(filterStatusOptions) {
		idx = 0
	}
	m.filterState = filterStatusOptions[idx]
	oldID, oldIdx := m.selectedID, m.selectedIdx
	m.applyFilters(oldID, oldIdx)
}

func (m *model) applyFilters(previousID string, previousIdx int) {
	filtered := make([]loopView, 0, len(m.loops))
	query := strings.ToLower(strings.TrimSpace(m.filterText))
	state := strings.ToLower(strings.TrimSpace(m.filterState))

	for _, view := range m.loops {
		if view.Loop == nil {
			continue
		}
		loopState := strings.ToLower(string(view.Loop.State))
		if state != "" && state != "all" && loopState != state {
			continue
		}
		if query != "" {
			idCandidate := strings.ToLower(loopDisplayID(view.Loop))
			fullID := strings.ToLower(view.Loop.ID)
			name := strings.ToLower(view.Loop.Name)
			repoPath := strings.ToLower(view.Loop.RepoPath)
			if !strings.Contains(idCandidate, query) && !strings.Contains(fullID, query) && !strings.Contains(name, query) && !strings.Contains(repoPath, query) {
				continue
			}
		}
		filtered = append(filtered, view)
	}

	m.filtered = filtered
	if len(filtered) == 0 {
		m.selectedIdx = 0
		m.selectedID = ""
		m.multiPage = 0
		return
	}

	if previousID != "" {
		for i := range filtered {
			if filtered[i].Loop != nil && filtered[i].Loop.ID == previousID {
				m.selectedIdx = i
				m.selectedID = previousID
				return
			}
		}
	}

	if previousIdx < 0 {
		previousIdx = 0
	}
	if previousIdx >= len(filtered) {
		previousIdx = len(filtered) - 1
	}
	m.selectedIdx = previousIdx
	m.selectedID = filtered[previousIdx].Loop.ID
	m.clampMultiPage()
}

func (m model) selectedView() (loopView, bool) {
	if len(m.filtered) == 0 {
		return loopView{}, false
	}
	idx := m.selectedIdx
	if idx < 0 {
		idx = 0
	}
	if idx >= len(m.filtered) {
		idx = len(m.filtered) - 1
	}
	return m.filtered[idx], true
}

func (m model) tabLabel(tab mainTab) string {
	switch tab {
	case tabOverview:
		return "Overview"
	case tabLogs:
		return "Logs"
	case tabRuns:
		return "Runs"
	case tabMultiLogs:
		return "Multi Logs"
	default:
		return "Overview"
	}
}

func (m model) tabShortLabel(tab mainTab) string {
	switch tab {
	case tabOverview:
		return "ov"
	case tabLogs:
		return "logs"
	case tabRuns:
		return "runs"
	case tabMultiLogs:
		return "multi"
	default:
		return "ov"
	}
}

func (m *model) setTab(tab mainTab) {
	if m.tab == tab {
		return
	}
	m.tab = tab
	m.logScroll = 0
	if tab == tabMultiLogs {
		m.focusRight = true
		m.clampMultiPage()
	} else if m.focusRight {
		m.focusRight = false
	}
}

func (m *model) cycleTab(delta int) {
	idx := 0
	for i, tab := range tabOrder {
		if tab == m.tab {
			idx = i
			break
		}
	}
	idx += delta
	for idx < 0 {
		idx += len(tabOrder)
	}
	m.setTab(tabOrder[idx%len(tabOrder)])
}

func (m model) currentLayout() paneLayout {
	if len(paneLayouts) == 0 {
		return paneLayout{Rows: 1, Cols: 1}
	}
	return paneLayouts[normalizeLayoutIndex(m.layoutIdx)]
}

func (m *model) cycleLayout(delta int) {
	m.layoutIdx = normalizeLayoutIndex(m.layoutIdx + delta)
	m.clampMultiPage()
}

func (m *model) togglePinned(loopID string) {
	if strings.TrimSpace(loopID) == "" {
		return
	}
	if _, ok := m.pinned[loopID]; ok {
		delete(m.pinned, loopID)
		m.setStatus(statusInfo, "Unpinned "+loopID)
		return
	}
	m.pinned[loopID] = struct{}{}
	m.setStatus(statusInfo, "Pinned "+loopID)
}

func (m model) isPinned(loopID string) bool {
	if strings.TrimSpace(loopID) == "" {
		return false
	}
	_, ok := m.pinned[loopID]
	return ok
}

func (m *model) clearPinned() {
	m.pinned = make(map[string]struct{})
	m.setStatus(statusInfo, "Cleared pinned loops")
}

func (m model) orderedMultiTargetViews() []loopView {
	ordered := make([]loopView, 0, len(m.filtered))
	added := make(map[string]struct{}, len(m.filtered))

	for _, view := range m.filtered {
		if view.Loop == nil {
			continue
		}
		if _, ok := m.pinned[view.Loop.ID]; !ok {
			continue
		}
		ordered = append(ordered, view)
		added[view.Loop.ID] = struct{}{}
	}
	for _, view := range m.filtered {
		if view.Loop == nil {
			continue
		}
		if _, ok := added[view.Loop.ID]; ok {
			continue
		}
		ordered = append(ordered, view)
	}
	return ordered
}

func multiPageBounds(total, pageSize, page int) (int, int, int, int) {
	if pageSize < 1 {
		pageSize = 1
	}
	if total < 0 {
		total = 0
	}
	totalPages := 1
	if total > 0 {
		totalPages = (total + pageSize - 1) / pageSize
	}
	if page < 0 {
		page = 0
	}
	if page >= totalPages {
		page = totalPages - 1
	}
	if page < 0 {
		page = 0
	}

	start := page * pageSize
	if start > total {
		start = total
	}
	end := start + pageSize
	if end > total {
		end = total
	}
	return page, totalPages, start, end
}

func (m model) multiViewportSize() (int, int) {
	width := m.effectiveWidth()
	height := m.effectiveHeight()

	overhead := 4
	if m.mode == modeFilter || m.mode == modeConfirm || m.mode == modeWizard || m.mode == modeHelp {
		overhead += 3
	}
	if m.statusText != "" {
		overhead++
	}
	paneHeight := maxInt(10, height-overhead)
	rightWidth := width
	if !m.focusRight {
		_, rightWidth = paneWidthsForTab(width, m.tab)
	}
	return maxInt(1, rightWidth-2), maxInt(1, paneHeight-2)
}

func (m model) effectiveMultiLayout() paneLayout {
	width, height := m.multiViewportSize()
	gridHeight := height - multiHeaderRows
	if gridHeight < multiMinCellHeight {
		gridHeight = multiMinCellHeight
	}
	return fitPaneLayout(m.currentLayout(), width, gridHeight, multiCellGap, multiMinCellWidth, multiMinCellHeight)
}

func (m model) multiPageSize() int {
	return maxInt(1, m.effectiveMultiLayout().Capacity())
}

func (m *model) clampMultiPage() {
	page, _, _, _ := multiPageBounds(len(m.orderedMultiTargetViews()), m.multiPageSize(), m.multiPage)
	m.multiPage = page
}

func (m *model) moveMultiPage(delta int) {
	page, totalPages, _, _ := multiPageBounds(len(m.orderedMultiTargetViews()), m.multiPageSize(), m.multiPage+delta)
	m.multiPage = page
	m.setStatus(statusInfo, fmt.Sprintf("Matrix page %d/%d", page+1, totalPages))
}

func (m *model) moveMultiPageToStart() {
	m.multiPage = 0
	m.clampMultiPage()
}

func (m *model) moveMultiPageToEnd() {
	page, _, _, _ := multiPageBounds(len(m.orderedMultiTargetViews()), m.multiPageSize(), 1<<30)
	m.multiPage = page
}

func (m model) multiPageTargets(page, pageSize int) ([]loopView, int, int, int, int) {
	ordered := m.orderedMultiTargetViews()
	clamped, totalPages, start, end := multiPageBounds(len(ordered), pageSize, page)
	if start >= len(ordered) {
		return nil, clamped, totalPages, start, end
	}
	return ordered[start:end], clamped, totalPages, start, end
}

func (m model) multiTargetViews(limit int) []loopView {
	views, _, _, _, _ := m.multiPageTargets(0, limit)
	return views
}

func (m model) multiTargetIDs(page, pageSize int) []string {
	views, _, _, _, _ := m.multiPageTargets(page, pageSize)
	ids := make([]string, 0, len(views))
	for _, view := range views {
		if view.Loop == nil {
			continue
		}
		ids = append(ids, view.Loop.ID)
	}
	return ids
}

func (m *model) moveRunSelection(delta int) {
	if len(m.runHistory) == 0 {
		m.selectedRun = 0
		return
	}
	m.selectedRun += delta
	if m.selectedRun < 0 {
		m.selectedRun = 0
	}
	if m.selectedRun >= len(m.runHistory) {
		m.selectedRun = len(m.runHistory) - 1
	}
	m.logScroll = 0
}

func (m *model) cycleLogSource(delta int) {
	options := []logSource{logSourceLive, logSourceLatestRun, logSourceRunSelection}
	idx := 0
	for i, option := range options {
		if option == m.logSource {
			idx = i
			break
		}
	}
	idx += delta
	for idx < 0 {
		idx += len(options)
	}
	m.logSource = options[idx%len(options)]
	m.logScroll = 0
	m.setStatus(statusInfo, "Log source: "+m.logSourceLabel())
}

func (m *model) scrollLogs(delta int) {
	m.logScroll += delta
	if m.logScroll < 0 {
		m.logScroll = 0
	}
}

func (m *model) scrollLogsToTop() {
	m.logScroll = maxLogBackfill
}

func (m *model) scrollLogsToBottom() {
	m.logScroll = 0
}

func (m model) logScrollPageSize() int {
	estimate := m.effectiveHeight()/2 + logScrollStep
	if estimate < logScrollStep {
		return logScrollStep
	}
	return estimate
}

func (m model) logSourceLabel() string {
	switch m.logSource {
	case logSourceLatestRun:
		return "latest-run"
	case logSourceRunSelection:
		return "selected-run"
	default:
		return "live"
	}
}

func (m model) logLayerLabel() string {
	switch m.logLayer {
	case logLayerEvents:
		return "events"
	case logLayerErrors:
		return "errors"
	case logLayerTools:
		return "tools"
	case logLayerDiff:
		return "diff"
	default:
		return "raw"
	}
}

func (m *model) cycleLogLayer(delta int) {
	options := []logLayer{logLayerRaw, logLayerEvents, logLayerErrors, logLayerTools, logLayerDiff}
	idx := 0
	for i, option := range options {
		if option == m.logLayer {
			idx = i
			break
		}
	}
	idx += delta
	for idx < 0 {
		idx += len(options)
	}
	m.logLayer = options[idx%len(options)]
	m.setStatus(statusInfo, "Log layer: "+m.logLayerLabel())
}

func (m model) selectedRunView() (runView, bool) {
	if len(m.runHistory) == 0 {
		return runView{}, false
	}
	idx := m.selectedRun
	if idx < 0 {
		idx = 0
	}
	if idx >= len(m.runHistory) {
		idx = len(m.runHistory) - 1
	}
	return m.runHistory[idx], true
}

func (m model) currentLogDisplay(view loopView) logDisplay {
	source := m.logSourceLabel()
	switch m.logSource {
	case logSourceLatestRun:
		if len(m.runHistory) == 0 || m.runHistory[0].Run == nil {
			return logDisplay{
				Title:   "No run history available.",
				Source:  source,
				Message: "No historical run output available.",
				Harness: view.ProfileHarness,
			}
		}
		run := m.runHistory[0]
		return logDisplay{
			Title:   fmt.Sprintf("Latest run %s (%s)", shortRunID(run.Run.ID), strings.ToUpper(string(run.Run.Status))),
			Source:  source,
			Lines:   runOutputLines(run.Run, m.desiredSelectedLogLines()),
			Message: "Run output is empty.",
			Harness: run.Harness,
		}
	case logSourceRunSelection:
		if run, ok := m.selectedRunView(); ok && run.Run != nil {
			return logDisplay{
				Title:   fmt.Sprintf("Run %s (%s)", shortRunID(run.Run.ID), strings.ToUpper(string(run.Run.Status))),
				Source:  source,
				Lines:   runOutputLines(run.Run, m.desiredSelectedLogLines()),
				Message: "Run output is empty.",
				Harness: run.Harness,
			}
		}
		return logDisplay{
			Title:   "No selected run.",
			Source:  source,
			Message: "Select a run with ,/.",
			Harness: view.ProfileHarness,
		}
	default:
		return logDisplay{
			Title:   fmt.Sprintf("Live loop log for %s", loopDisplayID(view.Loop)),
			Source:  source,
			Lines:   m.selectedLog.Lines,
			Message: m.selectedLog.Message,
			Harness: view.ProfileHarness,
		}
	}
}

func (m model) currentRunDisplay(view loopView) logDisplay {
	run, ok := m.selectedRunView()
	if !ok || run.Run == nil {
		return logDisplay{
			Title:   "No run selected.",
			Source:  "runs",
			Message: "No run output available.",
			Harness: view.ProfileHarness,
		}
	}
	return logDisplay{
		Title:   fmt.Sprintf("Run %s | profile=%s | started=%s", shortRunID(run.Run.ID), displayName(run.ProfileName, run.Run.ProfileID), run.Run.StartedAt.UTC().Format(time.RFC3339)),
		Source:  "runs",
		Lines:   runOutputLines(run.Run, m.desiredSelectedLogLines()),
		Message: "Run output is empty.",
		Harness: run.Harness,
	}
}

func (m model) renderLogBlock(display logDisplay, width, available, scroll int) []string {
	if available <= 0 {
		return nil
	}
	if len(display.Lines) == 0 {
		message := strings.TrimSpace(display.Message)
		if message == "" {
			message = "Log is empty."
		}
		return []string{truncateLine(message, width)}
	}
	lines := display.Lines
	start, end, _ := logWindowBounds(len(lines), available, scroll)
	lines = lines[start:end]
	highlighter := newHarnessLogHighlighter(display.Harness)
	rendered := make([]string, 0, len(lines))
	for _, line := range lines {
		if !lineMatchesLayer(display.Harness, line, m.logLayer) {
			continue
		}
		highlighted := highlighter.HighlightLine(m.palette, line)
		rendered = append(rendered, truncateLine(highlighted, width))
	}
	if len(rendered) == 0 {
		return []string{truncateLine("No lines matched layer="+m.logLayerLabel(), width)}
	}
	return rendered
}

func logWindowBounds(totalLines, available, scroll int) (int, int, int) {
	if totalLines <= 0 {
		return 0, 0, 0
	}
	if available < 1 {
		available = 1
	}
	maxScroll := totalLines - 1
	if maxScroll < 0 {
		maxScroll = 0
	}
	if scroll < 0 {
		scroll = 0
	}
	if scroll > maxScroll {
		scroll = maxScroll
	}
	end := totalLines - scroll
	if end < 0 {
		end = 0
	}
	if end > totalLines {
		end = totalLines
	}
	start := end - available
	if start < 0 {
		start = 0
	}
	return start, end, scroll
}

func shortRunID(id string) string {
	if len(id) <= 8 {
		return id
	}
	return id[:8]
}

func (m *model) setStatus(kind statusKind, text string) {
	m.statusKind = kind
	m.statusText = strings.TrimSpace(text)
	m.statusExpires = time.Now().Add(defaultStatusTTL)
}

func (m model) fetchCmd() tea.Cmd {
	database := m.db
	dataDir := m.dataDir
	selectedID := m.selectedID
	selectedLogLines := m.desiredSelectedLogLines()
	multiLogLines := m.desiredMultiLogLines()
	multiTargets := m.multiTargetIDs(m.multiPage, m.multiPageSize())

	if selectedID == "" && len(m.filtered) > 0 && m.selectedIdx >= 0 && m.selectedIdx < len(m.filtered) {
		selectedID = m.filtered[m.selectedIdx].Loop.ID
	}

	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancel()

		views, err := loadLoopViews(ctx, database)
		if err != nil {
			return refreshMsg{err: err}
		}

		logLoopID, tail := loadSelectedLogTail(views, selectedID, dataDir, selectedLogLines)
		runViews, _ := loadRunViews(ctx, database, logLoopID)
		multiLogs := loadLoopLogTails(views, multiTargets, dataDir, multiLogLines)
		return refreshMsg{
			loops:      views,
			selectedID: logLoopID,
			selected:   tail,
			runs:       runViews,
			multiLogs:  multiLogs,
		}
	}
}

func (m model) desiredSelectedLogLines() int {
	lines := m.logLines
	if lines <= 0 {
		lines = defaultLogLines
	}
	backfill := defaultLogBackfill + m.logScroll + m.logScrollPageSize()*2
	if backfill > maxLogBackfill {
		backfill = maxLogBackfill
	}

	switch {
	case m.mode == modeExpandedLogs:
		if m.tab == tabRuns {
			return maxInt(lines, 180)
		}
		return maxInt(backfill, 600)
	case m.tab == tabLogs:
		if m.logSource == logSourceLive {
			return maxInt(backfill, 400)
		}
		return maxInt(lines, 200)
	case m.tab == tabRuns:
		return maxInt(lines, 180)
	default:
		return maxInt(lines, 80)
	}
}

func (m model) desiredMultiLogLines() int {
	lines := m.logLines
	if lines <= 0 {
		lines = defaultLogLines
	}
	return maxInt(lines, 220)
}

func (m model) tickCmd() tea.Cmd {
	return tea.Tick(m.refreshInterval, func(time.Time) tea.Msg {
		return tickMsg{}
	})
}

func (m model) effectiveWidth() int {
	if m.width <= 0 {
		return 120
	}
	return maxInt(m.width, minWindowWidth)
}

func (m model) effectiveHeight() int {
	if m.height <= 0 {
		return 34
	}
	return maxInt(m.height, minWindowHeight)
}

func (m model) renderHeader() string {
	modeName := "Main"
	switch m.mode {
	case modeFilter:
		modeName = "Filter"
	case modeExpandedLogs:
		modeName = "Expanded Logs"
	case modeConfirm:
		modeName = "Confirm"
	case modeWizard:
		modeName = "New Loop Wizard"
	case modeHelp:
		modeName = "Help"
	}

	total := len(m.loops)
	running := 0
	for _, view := range m.loops {
		if view.Loop != nil && view.Loop.State == models.LoopStateRunning {
			running++
		}
	}
	header := fmt.Sprintf(
		"Forge loops  mode:%s  tab:%s  loops:%d running:%d  theme:%s  keys:/ filter n new S/K/D r l ? q",
		modeName,
		m.tabLabel(m.tab),
		total,
		running,
		m.palette.Name,
	)
	if m.actionBusy {
		header += "  action:running"
	}
	if m.focusRight {
		header += "  zen:on"
	}
	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(m.palette.Text)).
		Background(lipgloss.Color(m.palette.Panel)).
		Padding(0, 1).
		Render(header)
}

func (m model) renderTabBar(width int) string {
	tabs := make([]string, 0, len(tabOrder))
	for idx, tab := range tabOrder {
		label := fmt.Sprintf("%d %s", idx+1, m.tabLabel(tab))
		style := lipgloss.NewStyle().
			Foreground(lipgloss.Color(m.palette.TextMuted)).
			BorderStyle(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color(m.palette.Border)).
			Padding(0, 1)
		if tab == m.tab {
			style = style.
				Foreground(lipgloss.Color(m.palette.Text)).
				Background(lipgloss.Color(m.palette.PanelAlt)).
				BorderForeground(lipgloss.Color(m.palette.Focus)).
				Bold(true)
		}
		tabs = append(tabs, style.Render(label))
	}

	hints := ""
	switch m.tab {
	case tabLogs:
		hints = fmt.Sprintf("  v source  x layer(%s)  ,/. run  pgup/pgdn home/end  z zen  l expanded  ? help", m.logLayerLabel())
	case tabRuns:
		hints = fmt.Sprintf("  x layer(%s)  ,/. run  pgup/pgdn home/end  z zen  l expanded  ? help", m.logLayerLabel())
	case tabMultiLogs:
		hints = fmt.Sprintf("  x layer(%s)  space pin  c clear  m layout(%s)  ,/. page  home/end  z zen  ? help", m.logLayerLabel(), m.currentLayout().Label())
	default:
		hints = "  ]/[ tabs  t theme  z zen  space pin  ? help"
	}
	targetWidth := maxInt(1, width-1)
	line := lipgloss.JoinHorizontal(lipgloss.Top, tabs...)
	full := line
	if lipgloss.Width(line+hints) <= targetWidth {
		full = line + hints
	} else if lipgloss.Width(line) > targetWidth {
		compact := make([]string, 0, len(tabOrder))
		for i, tab := range tabOrder {
			label := fmt.Sprintf("%d:%s", i+1, m.tabShortLabel(tab))
			if tab == m.tab {
				label = "[" + label + "]"
			}
			compact = append(compact, label)
		}
		full = truncateLine(strings.Join(compact, " "), targetWidth)
	}
	return lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.TextMuted)).Render(full)
}

func paneWidthsForTab(width int, tab mainTab) (int, int) {
	ratio := 0.44
	if tab == tabMultiLogs {
		ratio = 0.30
	}
	left := int(float64(width) * ratio)
	if left < 34 {
		left = 34
	}
	right := width - left - 1
	if right < 34 {
		right = 34
		left = width - right - 1
		if left < 34 {
			left = 34
		}
	}
	return left, right
}

func (m model) renderLeftPane(width, height int) string {
	style := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(m.palette.Border)).
		Background(lipgloss.Color(m.palette.Panel)).
		Padding(0, 1).
		Width(width).
		Height(height)

	contentWidth := maxInt(1, width-2)
	rows := make([]string, 0, height)
	rows = append(rows, lipgloss.NewStyle().
		Foreground(lipgloss.Color(m.palette.Text)).
		Bold(true).
		Render(truncateLine("P STATUS   ID       RUNS HARNESS   DIR", contentWidth)))

	if len(m.filtered) == 0 {
		empty := []string{
			"No loops matched.",
			"Start one: forge up --count 1",
			"Press / to clear filter.",
		}
		for _, line := range empty {
			rows = append(rows, truncateLine(line, contentWidth))
		}
		return style.Render(strings.Join(rows, "\n"))
	}

	available := maxInt(1, height-4)
	start := 0
	if len(m.filtered) > available {
		start = m.selectedIdx - available/2
		if start < 0 {
			start = 0
		}
		if start > len(m.filtered)-available {
			start = len(m.filtered) - available
		}
	}
	end := minInt(len(m.filtered), start+available)

	for i := start; i < end; i++ {
		view := m.filtered[i]
		line := m.renderListRow(view, contentWidth-2)
		marker := "  "
		if i == m.selectedIdx {
			marker = lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.Focus)).Bold(true).Render("> ")
			line = lipgloss.NewStyle().
				Background(lipgloss.Color(m.palette.PanelAlt)).
				Foreground(lipgloss.Color(m.palette.Text)).
				Render(truncateLine(line, contentWidth-2))
		} else {
			line = truncateLine(line, contentWidth-2)
		}
		rows = append(rows, marker+line)
	}

	if start > 0 {
		rows = append(rows, truncateLine("...", contentWidth))
	}
	if end < len(m.filtered) {
		rows = append(rows, truncateLine(fmt.Sprintf("... %d more", len(m.filtered)-end), contentWidth))
	}

	return style.Render(strings.Join(rows, "\n"))
}

func (m model) renderListRow(view loopView, width int) string {
	if view.Loop == nil {
		return ""
	}
	status := strings.ToUpper(string(view.Loop.State))
	status = truncateLine(status, 7)
	statusStyled := statusStyleForPalette(m.palette, view.Loop.State).Render(padRight(status, 7))
	pin := " "
	if m.isPinned(view.Loop.ID) {
		pin = lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.Warning)).Bold(true).Render("P")
	}
	id := truncateLine(loopDisplayID(view.Loop), 9)
	runs := fmt.Sprintf("%d", view.Runs)
	harness := truncateLine(strings.ToLower(string(view.ProfileHarness)), 9)
	if strings.TrimSpace(harness) == "" {
		harness = "-"
	}
	dir := filepath.Base(view.Loop.RepoPath)

	base := fmt.Sprintf("%s %s %-9s %4s %-9s %s", pin, statusStyled, id, runs, harness, dir)
	return truncateLine(base, width)
}

func (m model) renderRightPane(width, height int) string {
	style := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(m.palette.Accent)).
		Background(lipgloss.Color(m.palette.Panel)).
		Padding(0, 1).
		Width(width).
		Height(height)

	if m.mode == modeExpandedLogs {
		return style.Render(m.renderExpandedLogs(width-2, height-2))
	}

	view, ok := m.selectedView()
	if !ok || view.Loop == nil {
		content := []string{
			"No loop selected.",
			"Use j/k or arrow keys to choose a loop.",
			"Start one: forge up --count 1",
		}
		return style.Render(strings.Join(content, "\n"))
	}

	switch m.tab {
	case tabOverview:
		return style.Render(m.renderOverviewPane(view, width-2, height-2))
	case tabLogs:
		return style.Render(m.renderLogsPane(view, width-2, height-2))
	case tabRuns:
		return style.Render(m.renderRunsPane(view, width-2, height-2))
	case tabMultiLogs:
		return style.Render(m.renderMultiLogsPane(width-2, height-2))
	default:
		return style.Render(m.renderOverviewPane(view, width-2, height-2))
	}
}

func (m model) renderOverviewPane(view loopView, width, height int) string {
	lines := make([]string, 0, 16)
	loopEntry := view.Loop
	lines = append(lines, fmt.Sprintf("ID: %s", loopDisplayID(loopEntry)))
	lines = append(lines, fmt.Sprintf("Name: %s", loopEntry.Name))
	lines = append(lines, fmt.Sprintf("Status: %s", strings.ToUpper(string(loopEntry.State))))
	lines = append(lines, fmt.Sprintf("Runs: %d", view.Runs))
	lines = append(lines, fmt.Sprintf("Dir: %s", loopEntry.RepoPath))
	lines = append(lines, fmt.Sprintf("Pool: %s", displayName(view.PoolName, loopEntry.PoolID)))
	lines = append(lines, fmt.Sprintf("Profile: %s", displayName(view.ProfileName, loopEntry.ProfileID)))
	lines = append(lines, fmt.Sprintf("Harness/Auth: %s / %s", displayName(string(view.ProfileHarness), "-"), displayName(view.ProfileAuth, "-")))
	lines = append(lines, fmt.Sprintf("Last Run: %s", formatTime(loopEntry.LastRunAt)))
	lines = append(lines, fmt.Sprintf("Queue Depth: %d", view.QueueDepth))
	lines = append(lines, fmt.Sprintf("Interval: %s", formatDurationSeconds(loopEntry.IntervalSeconds)))
	lines = append(lines, fmt.Sprintf("Max Runtime: %s", formatDurationSeconds(loopEntry.MaxRuntimeSeconds)))
	lines = append(lines, fmt.Sprintf("Max Iterations: %s", formatIterations(loopEntry.MaxIterations)))
	if strings.TrimSpace(loopEntry.LastError) != "" {
		lines = append(lines, fmt.Sprintf("Last Error: %s", loopEntry.LastError))
	}
	successCount := 0
	errorCount := 0
	killedCount := 0
	runningCount := 0
	for _, run := range m.runHistory {
		if run.Run == nil {
			continue
		}
		switch run.Run.Status {
		case models.LoopRunStatusSuccess:
			successCount++
		case models.LoopRunStatusError:
			errorCount++
		case models.LoopRunStatusKilled:
			killedCount++
		case models.LoopRunStatusRunning:
			runningCount++
		}
	}

	contentWidth := maxInt(1, width-2)
	content := make([]string, 0, len(lines)+12)
	for _, line := range lines {
		content = append(content, truncateLine(line, contentWidth))
	}
	content = append(content, "")
	content = append(content, lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.TextMuted)).Render("Run snapshot:"))
	content = append(content, truncateLine(fmt.Sprintf("  total=%d success=%d error=%d killed=%d running=%d", len(m.runHistory), successCount, errorCount, killedCount, runningCount), contentWidth))
	if run, ok := m.selectedRunView(); ok && run.Run != nil {
		content = append(content, truncateLine(fmt.Sprintf("  latest=%s status=%s exit=%s duration=%s", shortRunID(run.Run.ID), strings.ToUpper(string(run.Run.Status)), runExitCode(run.Run), formatRunDuration(run.Run)), contentWidth))
	}
	content = append(content, "")
	content = append(content, lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.TextMuted)).Render("Workflow: 2=Logs (deep scroll) | 3=Runs | 4=Multi Logs"))
	return strings.Join(trimToHeight(content, maxInt(1, height-1)), "\n")
}

func (m model) renderLogsPane(view loopView, width, height int) string {
	display := m.currentLogDisplay(view)
	width = maxInt(1, width-2)
	available := maxInt(1, height-3)
	start, end, clamped := logWindowBounds(len(display.Lines), available, m.logScroll)
	content := []string{
		fmt.Sprintf("Logs: %s  source=%s  layer=%s", loopDisplayID(view.Loop), m.logSourceLabel(), m.logLayerLabel()),
		truncateLine(fmt.Sprintf("%s | %s | pgup/pgdn home/end u/d", display.Title, formatLineWindow(start, end, len(display.Lines), clamped)), width),
	}
	content = append(content, m.renderLogBlock(display, width, available, m.logScroll)...)
	return strings.Join(content, "\n")
}

func (m model) renderRunsPane(view loopView, width, height int) string {
	contentWidth := maxInt(1, width-2)
	content := []string{
		fmt.Sprintf("Run history: %s  layer=%s", loopDisplayID(view.Loop), m.logLayerLabel()),
		",/. select run | x layer | pgup/pgdn scroll output | l expanded",
		"",
	}
	if len(m.runHistory) == 0 {
		content = append(content, "No recorded runs yet.")
		return strings.Join(content, "\n")
	}

	listLimit := minInt(len(m.runHistory), maxInt(3, height/3))
	for i := 0; i < listLimit; i++ {
		run := m.runHistory[i]
		if run.Run == nil {
			continue
		}
		prefix := "  "
		if i == m.selectedRun {
			prefix = lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.Focus)).Bold(true).Render("> ")
		}
		exit := "-"
		if run.Run.ExitCode != nil {
			exit = strconv.Itoa(*run.Run.ExitCode)
		}
		label := fmt.Sprintf(
			"%s %-7s exit=%s dur=%s %s",
			shortRunID(run.Run.ID),
			strings.ToUpper(string(run.Run.Status)),
			exit,
			formatRunDuration(run.Run),
			displayName(run.ProfileName, run.Run.ProfileID),
		)
		content = append(content, prefix+truncateLine(label, contentWidth-2))
	}
	if len(m.runHistory) > listLimit {
		content = append(content, truncateLine(fmt.Sprintf("... %d more runs", len(m.runHistory)-listLimit), contentWidth))
	}
	content = append(content, "")

	display := m.currentRunDisplay(view)
	content = append(content, lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.TextMuted)).Render(display.Title))
	available := maxInt(1, height-len(content)-2)
	start, end, clamped := logWindowBounds(len(display.Lines), available, m.logScroll)
	content = append(content, truncateLine("output "+formatLineWindow(start, end, len(display.Lines), clamped), contentWidth))
	content = append(content, m.renderLogBlock(display, contentWidth, available, m.logScroll)...)
	return strings.Join(content, "\n")
}

func (m model) renderMultiLogsPane(width, height int) string {
	width = maxInt(1, width)
	height = maxInt(1, height)

	gridHeight := height - multiHeaderRows
	if gridHeight < multiMinCellHeight {
		gridHeight = multiMinCellHeight
	}

	requested := m.currentLayout()
	layout := fitPaneLayout(requested, width, gridHeight, multiCellGap, multiMinCellWidth, multiMinCellHeight)
	cellWidth, cellHeight := layoutCellSize(layout, width, gridHeight, multiCellGap)
	cellWidth = maxInt(1, cellWidth)
	cellHeight = maxInt(1, cellHeight)

	targets, page, totalPages, start, end := m.multiPageTargets(m.multiPage, layout.Capacity())
	totalTargets := len(m.orderedMultiTargetViews())
	if totalTargets == 0 {
		return "No loops selected. Pin with <space> or create loops."
	}
	if len(targets) == 0 {
		return "No loops on this page. Use ,/. or home/end."
	}

	rows := make([]string, 0, layout.Rows*2)
	index := 0

	for row := 0; row < layout.Rows; row++ {
		cells := make([]string, 0, layout.Cols*2)
		for col := 0; col < layout.Cols; col++ {
			if index < len(targets) {
				cells = append(cells, m.renderMiniLogPane(targets[index], cellWidth, cellHeight))
			} else {
				cells = append(cells, m.renderMiniLogEmptyPane(cellWidth, cellHeight))
			}
			if col < layout.Cols-1 {
				cells = append(cells, strings.Repeat(" ", multiCellGap))
			}
			index++
		}
		rows = append(rows, lipgloss.JoinHorizontal(lipgloss.Top, cells...))
		if row < layout.Rows-1 {
			for i := 0; i < multiCellGap; i++ {
				rows = append(rows, "")
			}
		}
	}

	header := lipgloss.NewStyle().
		Foreground(lipgloss.Color(m.palette.Text)).
		Bold(true).
		Render(truncateLine(fmt.Sprintf("View 4 Matrix  requested=%s effective=%s  page=%d/%d  showing=%d-%d/%d", requested.Label(), layout.Label(), page+1, totalPages, start+1, end, totalTargets), width))
	subheader := lipgloss.NewStyle().
		Foreground(lipgloss.Color(m.palette.TextMuted)).
		Render(truncateLine(fmt.Sprintf("layer:%s  pin:<space> clear:c  layout:m  page:,/. home/end  order:pinned first", m.logLayerLabel()), width))
	return strings.Join(append([]string{header, subheader}, rows...), "\n")
}

func (m model) renderMiniLogPane(view loopView, width, height int) string {
	if view.Loop == nil {
		return m.renderMiniLogEmptyPane(width, height)
	}

	borderColor := m.palette.Border
	headerBG := m.palette.Panel
	headerFG := m.palette.Text
	switch view.Loop.State {
	case models.LoopStateRunning:
		borderColor = m.palette.Success
		headerBG = m.palette.Success
		headerFG = m.palette.Panel
	case models.LoopStateError:
		borderColor = m.palette.Error
		headerBG = m.palette.Error
		headerFG = m.palette.Panel
	case models.LoopStateWaiting, models.LoopStateSleeping:
		borderColor = m.palette.Warning
		headerBG = m.palette.Warning
		headerFG = m.palette.Panel
	}

	panelStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(borderColor)).
		Background(lipgloss.Color(m.palette.PanelAlt)).
		Padding(0, 1)
	innerWidth := maxInt(1, width-panelStyle.GetHorizontalFrameSize())
	innerHeight := maxInt(1, height-panelStyle.GetVerticalFrameSize())

	header := fmt.Sprintf("%s %s", loopDisplayID(view.Loop), view.Loop.Name)
	if m.isPinned(view.Loop.ID) {
		header += " [PIN]"
	}
	status := strings.ToUpper(string(view.Loop.State))
	meta := fmt.Sprintf("%-8s harness=%s runs=%d", status, strings.ToLower(displayName(string(view.ProfileHarness), "-")), view.Runs)

	lines := []string{
		lipgloss.NewStyle().
			Foreground(lipgloss.Color(headerFG)).
			Background(lipgloss.Color(headerBG)).
			Bold(true).
			Render(padRight(truncateLine(header, innerWidth), innerWidth)),
		lipgloss.NewStyle().
			Foreground(lipgloss.Color(m.palette.TextMuted)).
			Render(truncateLine(meta, innerWidth)),
		lipgloss.NewStyle().
			Foreground(lipgloss.Color(m.palette.Border)).
			Render(strings.Repeat("-", innerWidth)),
	}

	tail := m.multiLogs[view.Loop.ID]
	display := logDisplay{
		Source:  "live",
		Lines:   tail.Lines,
		Message: tail.Message,
		Harness: view.ProfileHarness,
	}
	lines = append(lines, m.renderLogBlock(display, innerWidth, maxInt(1, innerHeight-len(lines)), 0)...)
	if len(lines) > innerHeight {
		lines = lines[:innerHeight]
	}
	for len(lines) < innerHeight {
		lines = append(lines, "")
	}
	block := strings.Join(lines, "\n")

	return panelStyle.
		Width(innerWidth).
		Height(innerHeight).
		Render(block)
}

func (m model) renderMiniLogEmptyPane(width, height int) string {
	panelStyle := lipgloss.NewStyle().
		Border(lipgloss.NormalBorder()).
		BorderForeground(lipgloss.Color(m.palette.Border)).
		Background(lipgloss.Color(m.palette.PanelAlt)).
		Padding(0, 1)
	innerWidth := maxInt(1, width-panelStyle.GetHorizontalFrameSize())
	innerHeight := maxInt(1, height-panelStyle.GetVerticalFrameSize())
	lines := []string{
		lipgloss.NewStyle().
			Foreground(lipgloss.Color(m.palette.TextMuted)).
			Bold(true).
			Render(padRight("empty", innerWidth)),
		"Pin loops with <space>.",
		"Change layout with m.",
	}
	for len(lines) < innerHeight {
		lines = append(lines, "")
	}
	if len(lines) > innerHeight {
		lines = lines[:innerHeight]
	}
	for i := range lines {
		lines[i] = truncateLine(lines[i], innerWidth)
	}
	return panelStyle.
		Width(innerWidth).
		Height(innerHeight).
		Render(strings.Join(lines, "\n"))
}

func (m model) renderExpandedLogs(width, height int) string {
	view, ok := m.selectedView()
	if !ok || view.Loop == nil {
		return "No loop selected."
	}
	var display logDisplay
	switch m.tab {
	case tabRuns:
		display = m.currentRunDisplay(view)
	default:
		display = m.currentLogDisplay(view)
	}
	content := []string{
		fmt.Sprintf("Expanded logs for %s", loopDisplayID(view.Loop)),
		fmt.Sprintf("tab=%s source=%s layer=%s  q/esc close", m.tabLabel(m.tab), display.Source, m.logLayerLabel()),
		"",
	}
	available := maxInt(1, height-len(content)-2)
	start, end, clamped := logWindowBounds(len(display.Lines), available, m.logScroll)
	content = append(content, truncateLine(formatLineWindow(start, end, len(display.Lines), clamped)+"  pgup/pgdn home/end u/d", maxInt(1, width-2)))
	content = append(content, lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.TextMuted)).Render(display.Title))
	content = append(content, m.renderLogBlock(display, maxInt(1, width-2), maxInt(1, height-len(content)-1), m.logScroll)...)
	for i := range content {
		content[i] = truncateLine(content[i], maxInt(1, width-2))
	}
	return strings.Join(content, "\n")
}

func (m model) renderFilterBar(width int) string {
	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(m.palette.Border)).
		Background(lipgloss.Color(m.palette.PanelAlt)).
		Padding(0, 1).
		Width(maxInt(40, width))

	textStyle := lipgloss.NewStyle()
	statusStyle := lipgloss.NewStyle()
	if m.filterFocus == filterFocusText {
		textStyle = textStyle.Foreground(lipgloss.Color(m.palette.Focus)).Bold(true)
	} else {
		statusStyle = statusStyle.Foreground(lipgloss.Color(m.palette.Focus)).Bold(true)
	}

	textField := textStyle.Render(fmt.Sprintf("text=%q", m.filterText))
	statusParts := make([]string, 0, len(filterStatusOptions))
	for _, option := range filterStatusOptions {
		partStyle := lipgloss.NewStyle()
		if option == m.filterState {
			partStyle = partStyle.
				Foreground(lipgloss.Color(m.palette.Text)).
				Background(lipgloss.Color(m.palette.Panel))
		}
		part := partStyle.Render(option)
		statusParts = append(statusParts, part)
	}
	statusField := statusStyle.Render("status=" + strings.Join(statusParts, " "))

	line := fmt.Sprintf("Filter mode | %s | %s | tab switches focus | esc exits", textField, statusField)
	return box.Render(truncateLine(line, maxInt(1, width-6)))
}

func (m model) renderConfirmDialog(width int) string {
	if m.confirm == nil {
		return ""
	}
	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(m.palette.Warning)).
		Padding(0, 1).
		Width(maxInt(40, width))

	title := lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.Error)).Bold(true).Render("Confirm destructive action")
	text := []string{
		title,
		m.confirm.Prompt,
		"Press y to confirm. Press n, Enter, q, or Esc to cancel.",
	}
	return box.Render(strings.Join(text, "\n"))
}

func (m model) renderWizard(width int) string {
	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(m.palette.Accent)).
		Background(lipgloss.Color(m.palette.PanelAlt)).
		Padding(0, 1).
		Width(maxInt(40, width))

	stepLabels := []string{"1) Identity+Count", "2) Pool/Profile", "3) Prompt+Runtime", "4) Review+Submit"}
	for i := range stepLabels {
		if i+1 == m.wizard.Step {
			stepLabels[i] = lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.Focus)).Bold(true).Render(stepLabels[i])
		}
	}

	content := []string{
		"New loop wizard",
		strings.Join(stepLabels, "  "),
		"",
	}

	switch m.wizard.Step {
	case 1:
		content = append(content,
			renderWizardField(m.palette, "name", m.wizard.Values.Name, m.wizard.Field == 0),
			renderWizardField(m.palette, "name-prefix", m.wizard.Values.NamePrefix, m.wizard.Field == 1),
			renderWizardField(m.palette, "count", m.wizard.Values.Count, m.wizard.Field == 2),
		)
	case 2:
		content = append(content,
			renderWizardField(m.palette, "pool", m.wizard.Values.Pool, m.wizard.Field == 0),
			renderWizardField(m.palette, "profile", m.wizard.Values.Profile, m.wizard.Field == 1),
		)
	case 3:
		content = append(content,
			renderWizardField(m.palette, "prompt", m.wizard.Values.Prompt, m.wizard.Field == 0),
			renderWizardField(m.palette, "prompt-msg", m.wizard.Values.PromptMsg, m.wizard.Field == 1),
			renderWizardField(m.palette, "interval", m.wizard.Values.Interval, m.wizard.Field == 2),
			renderWizardField(m.palette, "max-runtime", m.wizard.Values.MaxRuntime, m.wizard.Field == 3),
			renderWizardField(m.palette, "max-iterations", m.wizard.Values.MaxIterations, m.wizard.Field == 4),
			renderWizardField(m.palette, "tags", m.wizard.Values.Tags, m.wizard.Field == 5),
		)
	case 4:
		content = append(content,
			"Review:",
			fmt.Sprintf("  name=%q", m.wizard.Values.Name),
			fmt.Sprintf("  name-prefix=%q", m.wizard.Values.NamePrefix),
			fmt.Sprintf("  count=%q", m.wizard.Values.Count),
			fmt.Sprintf("  pool=%q", m.wizard.Values.Pool),
			fmt.Sprintf("  profile=%q", m.wizard.Values.Profile),
			fmt.Sprintf("  prompt=%q", m.wizard.Values.Prompt),
			fmt.Sprintf("  prompt-msg=%q", m.wizard.Values.PromptMsg),
			fmt.Sprintf("  interval=%q", m.wizard.Values.Interval),
			fmt.Sprintf("  max-runtime=%q", m.wizard.Values.MaxRuntime),
			fmt.Sprintf("  max-iterations=%q", m.wizard.Values.MaxIterations),
			fmt.Sprintf("  tags=%q", m.wizard.Values.Tags),
		)
	}

	content = append(content, "")
	content = append(content, "tab/down/up navigate fields, enter next/submit, b back, esc cancel")
	if m.wizard.Error != "" {
		content = append(content, lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.Error)).Render("Error: "+m.wizard.Error))
	}

	for i := range content {
		content[i] = truncateLine(content[i], maxInt(1, width-6))
	}

	return box.Render(strings.Join(content, "\n"))
}

func (m model) renderHelpDialog(width int) string {
	box := lipgloss.NewStyle().
		Border(lipgloss.DoubleBorder()).
		BorderForeground(lipgloss.Color(m.palette.Focus)).
		Background(lipgloss.Color(m.palette.PanelAlt)).
		Padding(0, 1).
		Width(maxInt(56, width))

	lines := []string{
		lipgloss.NewStyle().Foreground(lipgloss.Color(m.palette.Text)).Bold(true).Render("Forge TUI Help"),
		"",
		"Global:",
		"  q quit | ? toggle help | ]/[ tab cycle | 1..4 jump tabs | t theme | z zen",
		"  j/k or arrows move loop | / filter | l expanded logs | n new loop wizard",
		"  S/K/D stop/kill/delete | r resume | space pin/unpin | c clear pins",
		"",
		"Logs + Runs:",
		"  v source cycle (live/latest-run/selected-run)",
		"  x semantic layer cycle (raw/events/errors/tools/diff)",
		"  ,/. previous/next run",
		"  pgup/pgdn/home/end/u/d scroll log output",
		"",
		"Multi Logs:",
		"  m cycle layouts (1x1 -> 4x4)",
		"  ,/. previous/next page | home/end first/last page",
		"",
		"Press q, esc, or ? to close help.",
	}
	for i := range lines {
		lines[i] = truncateLine(lines[i], maxInt(1, width-8))
	}
	return box.Render(strings.Join(lines, "\n"))
}

func renderWizardField(palette tuiPalette, label, value string, focused bool) string {
	display := value
	if strings.TrimSpace(display) == "" {
		display = "<empty>"
	}
	if focused {
		display = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Focus)).Render(display + "_")
	}
	return fmt.Sprintf("%s: %s", label, display)
}

func (m model) renderStatusLine(width int) string {
	style := lipgloss.NewStyle()
	switch m.statusKind {
	case statusOK:
		style = style.Foreground(lipgloss.Color(m.palette.Success)).Bold(true)
	case statusErr:
		style = style.Foreground(lipgloss.Color(m.palette.Error)).Bold(true)
	default:
		style = style.Foreground(lipgloss.Color(m.palette.Info))
	}
	return style.Render(truncateLine(m.statusText, maxInt(1, width-1)))
}

func statusStyleForPalette(palette tuiPalette, state models.LoopState) lipgloss.Style {
	switch state {
	case models.LoopStateRunning:
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Success)).Bold(true)
	case models.LoopStateWaiting, models.LoopStateSleeping:
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Warning)).Bold(true)
	case models.LoopStateStopped:
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.TextMuted)).Bold(true)
	case models.LoopStateError:
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Error)).Bold(true)
	default:
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Info))
	}
}

func loadLoopViews(ctx context.Context, database *db.DB) ([]loopView, error) {
	if database == nil {
		return nil, errors.New("database is nil")
	}

	loopRepo := db.NewLoopRepository(database)
	queueRepo := db.NewLoopQueueRepository(database)
	profileRepo := db.NewProfileRepository(database)
	poolRepo := db.NewPoolRepository(database)
	runRepo := db.NewLoopRunRepository(database)

	loops, err := loopRepo.List(ctx)
	if err != nil {
		return nil, err
	}

	profiles, _ := profileRepo.List(ctx)
	pools, _ := poolRepo.List(ctx)
	profileNames := make(map[string]string)
	profileHarness := make(map[string]models.Harness)
	profileAuth := make(map[string]string)
	poolNames := make(map[string]string)
	for _, profile := range profiles {
		if profile == nil {
			continue
		}
		profileNames[profile.ID] = profile.Name
		profileHarness[profile.ID] = profile.Harness
		profileAuth[profile.ID] = profile.AuthKind
	}
	for _, pool := range pools {
		if pool == nil {
			continue
		}
		poolNames[pool.ID] = pool.Name
	}

	views := make([]loopView, 0, len(loops))
	for _, loopEntry := range loops {
		if loopEntry == nil {
			continue
		}

		runs, _ := runRepo.CountByLoop(ctx, loopEntry.ID)
		queueItems, _ := queueRepo.List(ctx, loopEntry.ID)
		queueDepth := 0
		for _, item := range queueItems {
			if item.Status == models.LoopQueueStatusPending || item.Status == models.LoopQueueStatusDispatched {
				queueDepth++
			}
		}

		views = append(views, loopView{
			Loop:           loopEntry,
			Runs:           runs,
			QueueDepth:     queueDepth,
			ProfileName:    profileNames[loopEntry.ProfileID],
			ProfileHarness: profileHarness[loopEntry.ProfileID],
			ProfileAuth:    profileAuth[loopEntry.ProfileID],
			PoolName:       poolNames[loopEntry.PoolID],
		})
	}

	sort.Slice(views, func(i, j int) bool {
		left := views[i].Loop
		right := views[j].Loop
		if left == nil || right == nil {
			return i < j
		}
		return left.CreatedAt.Before(right.CreatedAt)
	})

	return views, nil
}

func loadSelectedLogTail(views []loopView, selectedID, dataDir string, maxLines int) (string, logTailView) {
	if maxLines <= 0 {
		maxLines = defaultLogLines
	}

	var selected *models.Loop
	if selectedID != "" {
		for _, view := range views {
			if view.Loop != nil && view.Loop.ID == selectedID {
				selected = view.Loop
				break
			}
		}
	}
	if selected == nil && len(views) > 0 {
		selected = views[0].Loop
	}
	if selected == nil {
		return "", logTailView{}
	}

	path := selected.LogPath
	if path == "" {
		path = loop.LogPath(dataDir, selected.Name, selected.ID)
	}

	return selected.ID, loadLoopLogTail(path, maxLines)
}

func loadLoopLogTails(views []loopView, loopIDs []string, dataDir string, maxLines int) map[string]logTailView {
	if len(loopIDs) == 0 {
		return map[string]logTailView{}
	}

	viewByID := make(map[string]loopView, len(views))
	for _, view := range views {
		if view.Loop == nil {
			continue
		}
		viewByID[view.Loop.ID] = view
	}

	result := make(map[string]logTailView, len(loopIDs))
	for _, loopID := range loopIDs {
		view, ok := viewByID[loopID]
		if !ok || view.Loop == nil {
			continue
		}
		path := view.Loop.LogPath
		if path == "" {
			path = loop.LogPath(dataDir, view.Loop.Name, view.Loop.ID)
		}
		result[loopID] = loadLoopLogTail(path, maxLines)
	}
	return result
}

func loadLoopLogTail(path string, maxLines int) logTailView {
	tail, err := tailFile(path, maxLines)
	if err != nil {
		if os.IsNotExist(err) {
			return logTailView{Message: "Log file not found."}
		}
		return logTailView{Message: "Failed to read log: " + err.Error()}
	}
	if len(tail) == 0 {
		return logTailView{Message: "Log is empty."}
	}
	return logTailView{Lines: tail}
}

func tailFile(path string, maxLines int) ([]string, error) {
	if maxLines <= 0 {
		return nil, nil
	}
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()

	info, err := file.Stat()
	if err != nil {
		return nil, err
	}
	size := info.Size()
	if size <= 0 {
		return nil, nil
	}

	const chunkSize int64 = 4096
	offset := size
	capSize := maxTailReadBytes
	if size < int64(capSize) {
		capSize = int(size)
	}
	buffer := make([]byte, 0, capSize)
	newlineCount := 0
	readTotal := 0

	for offset > 0 && newlineCount <= maxLines && readTotal < maxTailReadBytes {
		readSize := chunkSize
		if offset < readSize {
			readSize = offset
		}
		if remaining := maxTailReadBytes - readTotal; int(readSize) > remaining {
			readSize = int64(remaining)
		}
		offset -= readSize

		chunk := make([]byte, readSize)
		n, readErr := file.ReadAt(chunk, offset)
		if readErr != nil && !errors.Is(readErr, io.EOF) {
			return nil, readErr
		}
		chunk = chunk[:n]
		if len(chunk) == 0 {
			break
		}
		newlineCount += bytes.Count(chunk, []byte{'\n'})
		readTotal += len(chunk)
		buffer = append(chunk, buffer...)
	}

	trimmed := strings.TrimRight(string(buffer), "\n")
	if strings.TrimSpace(trimmed) == "" {
		return nil, nil
	}
	lines := strings.Split(trimmed, "\n")
	if len(lines) > maxLines {
		lines = lines[len(lines)-maxLines:]
	}
	return lines, nil
}

func stopLoop(ctx context.Context, database *db.DB, loopID string) (string, error) {
	loopRepo := db.NewLoopRepository(database)
	queueRepo := db.NewLoopQueueRepository(database)
	loopEntry, err := loopRepo.Get(ctx, loopID)
	if err != nil {
		return "", err
	}

	payload, err := json.Marshal(models.StopPayload{Reason: "operator"})
	if err != nil {
		return "", err
	}
	item := &models.LoopQueueItem{Type: models.LoopQueueItemStopGraceful, Payload: payload}
	if err := queueRepo.Enqueue(ctx, loopEntry.ID, item); err != nil {
		return "", err
	}

	return fmt.Sprintf("Stop requested for loop %s", loopDisplayID(loopEntry)), nil
}

func killLoop(ctx context.Context, database *db.DB, loopID string) (string, error) {
	loopRepo := db.NewLoopRepository(database)
	queueRepo := db.NewLoopQueueRepository(database)
	loopEntry, err := loopRepo.Get(ctx, loopID)
	if err != nil {
		return "", err
	}

	payload, err := json.Marshal(models.KillPayload{Reason: "operator"})
	if err != nil {
		return "", err
	}
	item := &models.LoopQueueItem{Type: models.LoopQueueItemKillNow, Payload: payload}
	if err := queueRepo.Enqueue(ctx, loopEntry.ID, item); err != nil {
		return "", err
	}

	_ = killLoopProcess(loopEntry)
	loopEntry.State = models.LoopStateStopped
	_ = loopRepo.Update(ctx, loopEntry)
	return fmt.Sprintf("Killed loop %s", loopDisplayID(loopEntry)), nil
}

func resumeLoop(ctx context.Context, database *db.DB, configFile, loopID string) (string, error) {
	loopRepo := db.NewLoopRepository(database)
	loopEntry, err := loopRepo.Get(ctx, loopID)
	if err != nil {
		return "", err
	}

	switch loopEntry.State {
	case models.LoopStateStopped, models.LoopStateError:
	default:
		return "", fmt.Errorf("loop %q is %s; only stopped or errored loops can be resumed", loopEntry.Name, loopEntry.State)
	}

	if err := startLoopProcessFn(loopEntry.ID, configFile); err != nil {
		return "", err
	}
	if err := setLoopRunnerMetadata(ctx, loopRepo, loopEntry.ID, "local", ""); err != nil {
		return "", err
	}
	return fmt.Sprintf("Loop %q resumed (%s)", loopEntry.Name, loopDisplayID(loopEntry)), nil
}

func deleteLoop(ctx context.Context, database *db.DB, loopID string, force bool) (string, error) {
	loopRepo := db.NewLoopRepository(database)
	loopEntry, err := loopRepo.Get(ctx, loopID)
	if err != nil {
		return "", err
	}
	if loopEntry.State != models.LoopStateStopped && !force {
		return "", fmt.Errorf("loop %q is %s; force delete required", loopEntry.Name, loopEntry.State)
	}
	if err := loopRepo.Delete(ctx, loopEntry.ID); err != nil {
		return "", err
	}
	return fmt.Sprintf("Loop record %s deleted", loopDisplayID(loopEntry)), nil
}

func createLoops(ctx context.Context, database *db.DB, dataDir, configFile string, defaultInterval time.Duration, defaultPrompt, defaultPromptMsg string, values wizardValues) (string, string, error) {
	spec, err := buildWizardSpec(values, defaultInterval, defaultPrompt, defaultPromptMsg)
	if err != nil {
		return "", "", err
	}

	repoPath, err := resolveRepoPath("")
	if err != nil {
		return "", "", err
	}

	loopRepo := db.NewLoopRepository(database)
	poolRepo := db.NewPoolRepository(database)
	profileRepo := db.NewProfileRepository(database)

	poolID := ""
	if spec.PoolRef != "" {
		pool, err := resolvePoolByRef(ctx, poolRepo, spec.PoolRef)
		if err != nil {
			return "", "", err
		}
		poolID = pool.ID
	}

	profileID := ""
	if spec.ProfileRef != "" {
		profile, err := resolveProfileByRef(ctx, profileRepo, spec.ProfileRef)
		if err != nil {
			return "", "", err
		}
		profileID = profile.ID
	}

	promptPath := ""
	if spec.PromptRef != "" {
		resolved, err := resolvePromptPath(repoPath, spec.PromptRef)
		if err != nil {
			return "", "", err
		}
		promptPath = resolved
	}

	existing, err := loopRepo.List(ctx)
	if err != nil {
		return "", "", err
	}
	existingNames := make(map[string]struct{}, len(existing))
	for _, item := range existing {
		existingNames[item.Name] = struct{}{}
	}

	createdIDs := make([]string, 0, spec.Count)
	for i := 0; i < spec.Count; i++ {
		name := spec.Name
		if name == "" {
			if spec.NamePrefix != "" {
				name = fmt.Sprintf("%s-%d", spec.NamePrefix, i+1)
			} else {
				name = generateLoopName(existingNames)
			}
		}
		if _, exists := existingNames[name]; exists {
			return "", "", fmt.Errorf("loop name %q already exists", name)
		}
		existingNames[name] = struct{}{}

		entry := &models.Loop{
			Name:              name,
			RepoPath:          repoPath,
			BasePromptPath:    promptPath,
			BasePromptMsg:     spec.PromptMsg,
			IntervalSeconds:   int(spec.Interval.Round(time.Second).Seconds()),
			MaxIterations:     spec.MaxIterations,
			MaxRuntimeSeconds: int(spec.MaxRuntime.Round(time.Second).Seconds()),
			PoolID:            poolID,
			ProfileID:         profileID,
			Tags:              spec.Tags,
			State:             models.LoopStateStopped,
		}

		if err := loopRepo.Create(ctx, entry); err != nil {
			return "", "", err
		}
		entry.LogPath = loop.LogPath(dataDir, entry.Name, entry.ID)
		entry.LedgerPath = loop.LedgerPath(repoPath, entry.Name, entry.ID)
		if err := loopRepo.Update(ctx, entry); err != nil {
			return "", "", err
		}
		if err := startLoopProcessFn(entry.ID, configFile); err != nil {
			return "", "", err
		}
		if err := setLoopRunnerMetadata(ctx, loopRepo, entry.ID, "local", ""); err != nil {
			return "", "", err
		}
		createdIDs = append(createdIDs, entry.ID)
	}

	selectedID := ""
	if len(createdIDs) > 0 {
		selectedID = createdIDs[len(createdIDs)-1]
	}
	return selectedID, fmt.Sprintf("Created %d loop(s)", len(createdIDs)), nil
}

type wizardSpec struct {
	Name          string
	NamePrefix    string
	Count         int
	PoolRef       string
	ProfileRef    string
	PromptRef     string
	PromptMsg     string
	Interval      time.Duration
	MaxRuntime    time.Duration
	MaxIterations int
	Tags          []string
}

func buildWizardSpec(values wizardValues, defaultInterval time.Duration, defaultPrompt, defaultPromptMsg string) (wizardSpec, error) {
	count, err := parsePositiveInt(defaultString(values.Count, "1"), "count")
	if err != nil {
		return wizardSpec{}, err
	}

	name := strings.TrimSpace(values.Name)
	namePrefix := strings.TrimSpace(values.NamePrefix)
	if name != "" && count > 1 {
		return wizardSpec{}, fmt.Errorf("name requires count=1")
	}

	poolRef := strings.TrimSpace(values.Pool)
	profileRef := strings.TrimSpace(values.Profile)
	if poolRef != "" && profileRef != "" {
		return wizardSpec{}, fmt.Errorf("use either pool or profile, not both")
	}

	interval, err := parseDurationInput(values.Interval, defaultInterval)
	if err != nil {
		return wizardSpec{}, err
	}
	if interval < 0 {
		return wizardSpec{}, fmt.Errorf("interval must be >= 0")
	}

	maxRuntime, err := parseDurationInput(values.MaxRuntime, 0)
	if err != nil {
		return wizardSpec{}, err
	}
	if maxRuntime < 0 {
		return wizardSpec{}, fmt.Errorf("max runtime must be >= 0")
	}
	// Zero maxRuntime/maxIterations means "no limit" (unset).
	maxIterations := 0
	if strings.TrimSpace(values.MaxIterations) != "" {
		parsed, err := strconv.Atoi(strings.TrimSpace(values.MaxIterations))
		if err != nil {
			return wizardSpec{}, fmt.Errorf("invalid max-iterations %q", values.MaxIterations)
		}
		if parsed < 0 {
			return wizardSpec{}, fmt.Errorf("max-iterations must be >= 0")
		}
		maxIterations = parsed
	}

	promptRef := strings.TrimSpace(values.Prompt)
	if promptRef == "" {
		promptRef = strings.TrimSpace(defaultPrompt)
	}

	promptMsg := strings.TrimSpace(values.PromptMsg)
	if promptMsg == "" {
		promptMsg = strings.TrimSpace(defaultPromptMsg)
	}

	return wizardSpec{
		Name:          name,
		NamePrefix:    namePrefix,
		Count:         count,
		PoolRef:       poolRef,
		ProfileRef:    profileRef,
		PromptRef:     promptRef,
		PromptMsg:     promptMsg,
		Interval:      interval,
		MaxRuntime:    maxRuntime,
		MaxIterations: maxIterations,
		Tags:          parseTags(values.Tags),
	}, nil
}

func validateWizardStep(step int, values wizardValues, defaultInterval time.Duration) error {
	switch step {
	case 1:
		_, err := parsePositiveInt(defaultString(values.Count, "1"), "count")
		if err != nil {
			return err
		}
		if strings.TrimSpace(values.Name) != "" && strings.TrimSpace(values.Count) != "" {
			count, _ := strconv.Atoi(strings.TrimSpace(values.Count))
			if count > 1 {
				return fmt.Errorf("name requires count=1")
			}
		}
	case 2:
		if strings.TrimSpace(values.Pool) != "" && strings.TrimSpace(values.Profile) != "" {
			return fmt.Errorf("use either pool or profile, not both")
		}
	case 3:
		if _, err := parseDurationInput(values.Interval, defaultInterval); err != nil {
			return err
		}
		maxRuntime, err := parseDurationInput(values.MaxRuntime, 0)
		if err != nil {
			return err
		}
		if maxRuntime < 0 {
			return fmt.Errorf("max runtime must be >= 0")
		}
		if strings.TrimSpace(values.MaxIterations) != "" {
			parsed, err := strconv.Atoi(strings.TrimSpace(values.MaxIterations))
			if err != nil {
				return fmt.Errorf("invalid max-iterations %q", values.MaxIterations)
			}
			if parsed < 0 {
				return fmt.Errorf("max-iterations must be >= 0")
			}
		}
	}
	return nil
}

func newWizardState(defaultInterval time.Duration, defaultPrompt, defaultPromptMsg string) wizardState {
	interval := ""
	if defaultInterval > 0 {
		interval = defaultInterval.String()
	}
	return wizardState{
		Step:  1,
		Field: 0,
		Values: wizardValues{
			Count:      "1",
			Prompt:     strings.TrimSpace(defaultPrompt),
			PromptMsg:  strings.TrimSpace(defaultPromptMsg),
			Interval:   interval,
			MaxRuntime: "",
		},
	}
}

func (m *model) wizardNextField() {
	count := wizardFieldCount(m.wizard.Step)
	if count <= 0 {
		return
	}
	m.wizard.Field = (m.wizard.Field + 1) % count
}

func (m *model) wizardPrevField() {
	count := wizardFieldCount(m.wizard.Step)
	if count <= 0 {
		return
	}
	m.wizard.Field--
	if m.wizard.Field < 0 {
		m.wizard.Field = count - 1
	}
}

func wizardFieldCount(step int) int {
	switch step {
	case 1:
		return 3
	case 2:
		return 2
	case 3:
		return 6
	default:
		return 0
	}
}

func wizardFieldKey(step, field int) string {
	switch step {
	case 1:
		switch field {
		case 0:
			return "name"
		case 1:
			return "name_prefix"
		case 2:
			return "count"
		}
	case 2:
		switch field {
		case 0:
			return "pool"
		case 1:
			return "profile"
		}
	case 3:
		switch field {
		case 0:
			return "prompt"
		case 1:
			return "prompt_msg"
		case 2:
			return "interval"
		case 3:
			return "max_runtime"
		case 4:
			return "max_iterations"
		case 5:
			return "tags"
		}
	}
	return ""
}

func wizardGet(values *wizardValues, key string) string {
	switch key {
	case "name":
		return values.Name
	case "name_prefix":
		return values.NamePrefix
	case "count":
		return values.Count
	case "pool":
		return values.Pool
	case "profile":
		return values.Profile
	case "prompt":
		return values.Prompt
	case "prompt_msg":
		return values.PromptMsg
	case "interval":
		return values.Interval
	case "max_runtime":
		return values.MaxRuntime
	case "max_iterations":
		return values.MaxIterations
	case "tags":
		return values.Tags
	default:
		return ""
	}
}

func wizardSet(values *wizardValues, key, value string) {
	switch key {
	case "name":
		values.Name = value
	case "name_prefix":
		values.NamePrefix = value
	case "count":
		values.Count = value
	case "pool":
		values.Pool = value
	case "profile":
		values.Profile = value
	case "prompt":
		values.Prompt = value
	case "prompt_msg":
		values.PromptMsg = value
	case "interval":
		values.Interval = value
	case "max_runtime":
		values.MaxRuntime = value
	case "max_iterations":
		values.MaxIterations = value
	case "tags":
		values.Tags = value
	}
}

func parsePositiveInt(value, field string) (int, error) {
	parsed, err := strconv.Atoi(strings.TrimSpace(value))
	if err != nil {
		return 0, fmt.Errorf("invalid %s %q", field, value)
	}
	if parsed < 1 {
		return 0, fmt.Errorf("%s must be at least 1", field)
	}
	return parsed, nil
}

func parseDurationInput(value string, fallback time.Duration) (time.Duration, error) {
	if strings.TrimSpace(value) == "" {
		return fallback, nil
	}
	parsed, err := time.ParseDuration(strings.TrimSpace(value))
	if err != nil {
		return 0, fmt.Errorf("invalid duration %q", value)
	}
	return parsed, nil
}

func resolveRepoPath(path string) (string, error) {
	if path == "" {
		cwd, err := os.Getwd()
		if err != nil {
			return "", fmt.Errorf("failed to get current directory: %w", err)
		}
		return filepath.Abs(cwd)
	}
	return filepath.Abs(path)
}

func resolvePromptPath(repoPath, value string) (string, error) {
	if strings.TrimSpace(value) == "" {
		return "", errors.New("prompt is required")
	}

	candidate := value
	if !filepath.IsAbs(candidate) {
		candidate = filepath.Join(repoPath, candidate)
	}
	if exists(candidate) {
		return candidate, nil
	}

	if !strings.HasSuffix(value, ".md") {
		candidate = filepath.Join(repoPath, ".forge", "prompts", value+".md")
	} else {
		candidate = filepath.Join(repoPath, ".forge", "prompts", value)
	}
	if exists(candidate) {
		return candidate, nil
	}

	return "", fmt.Errorf("prompt not found: %s", value)
}

func parseTags(value string) []string {
	if strings.TrimSpace(value) == "" {
		return nil
	}
	parts := strings.Split(value, ",")
	seen := make(map[string]struct{})
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		tag := strings.TrimSpace(part)
		if tag == "" {
			continue
		}
		if _, ok := seen[tag]; ok {
			continue
		}
		seen[tag] = struct{}{}
		out = append(out, tag)
	}
	return out
}

func setLoopRunnerMetadata(ctx context.Context, loopRepo *db.LoopRepository, loopID, owner, instanceID string) error {
	loopEntry, err := loopRepo.Get(ctx, loopID)
	if err != nil {
		return err
	}
	if loopEntry.Metadata == nil {
		loopEntry.Metadata = make(map[string]any)
	}
	loopEntry.Metadata["runner_owner"] = owner
	if strings.TrimSpace(instanceID) == "" {
		delete(loopEntry.Metadata, "runner_instance_id")
	} else {
		loopEntry.Metadata["runner_instance_id"] = instanceID
	}
	return loopRepo.Update(ctx, loopEntry)
}

func resolvePoolByRef(ctx context.Context, repo *db.PoolRepository, ref string) (*models.Pool, error) {
	pool, err := repo.GetByName(ctx, ref)
	if err == nil {
		return pool, nil
	}
	return repo.Get(ctx, ref)
}

func resolveProfileByRef(ctx context.Context, repo *db.ProfileRepository, ref string) (*models.Profile, error) {
	profile, err := repo.GetByName(ctx, ref)
	if err == nil {
		return profile, nil
	}
	return repo.Get(ctx, ref)
}

func startLoopProcess(loopID, configFile string) error {
	args := []string{"loop", "run", loopID}
	if strings.TrimSpace(configFile) != "" {
		args = append([]string{"--config", configFile}, args...)
	}

	cmd := exec.Command(os.Args[0], args...)
	cmd.Stdout = nil
	cmd.Stderr = nil
	cmd.Stdin = nil
	procutil.ConfigureDetached(cmd)
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("failed to start loop process: %w", err)
	}
	if cmd.Process != nil {
		_ = cmd.Process.Release()
	}
	return nil
}

func killLoopProcess(loopEntry *models.Loop) error {
	pid, ok := loopPID(loopEntry)
	if !ok {
		return nil
	}
	process, err := os.FindProcess(pid)
	if err != nil {
		return err
	}
	if err := process.Signal(syscall.SIGKILL); err != nil {
		_ = process.Kill()
	}
	return nil
}

func loopPID(loopEntry *models.Loop) (int, bool) {
	if loopEntry == nil || loopEntry.Metadata == nil {
		return 0, false
	}
	value, ok := loopEntry.Metadata["pid"]
	if !ok {
		return 0, false
	}
	switch v := value.(type) {
	case float64:
		return int(v), true
	case int:
		return v, true
	case int64:
		return int(v), true
	case string:
		parsed, err := strconv.Atoi(v)
		if err != nil {
			return 0, false
		}
		return parsed, true
	default:
		return 0, false
	}
}

func generateLoopName(existing map[string]struct{}) string {
	rng := rand.New(rand.NewSource(time.Now().UnixNano()))
	maxAttempts := names.LoopNameCountTwoPart() * 2

	for i := 0; i < maxAttempts; i++ {
		candidate := strings.TrimSpace(names.RandomLoopNameTwoPart(rng))
		if candidate == "" {
			continue
		}
		if _, ok := existing[candidate]; ok {
			continue
		}
		return candidate
	}

	maxAttempts = names.LoopNameCountThreePart() * 2
	for i := 0; i < maxAttempts; i++ {
		candidate := strings.TrimSpace(names.RandomLoopNameThreePart(rng))
		if candidate == "" {
			continue
		}
		if _, ok := existing[candidate]; ok {
			continue
		}
		return candidate
	}

	fallback := "loop-" + time.Now().Format("150405")
	if _, ok := existing[fallback]; !ok {
		return fallback
	}

	counter := 1
	for {
		candidate := fmt.Sprintf("%s-%d", fallback, counter)
		if _, ok := existing[candidate]; !ok {
			return candidate
		}
		counter++
	}
}

func loopDisplayID(loopEntry *models.Loop) string {
	if loopEntry == nil {
		return ""
	}
	if strings.TrimSpace(loopEntry.ShortID) != "" {
		return loopEntry.ShortID
	}
	if len(loopEntry.ID) <= 8 {
		return loopEntry.ID
	}
	return loopEntry.ID[:8]
}

func displayName(name, fallback string) string {
	if strings.TrimSpace(name) != "" {
		return name
	}
	if strings.TrimSpace(fallback) != "" {
		return fallback
	}
	return "-"
}

func formatDurationSeconds(seconds int) string {
	if seconds <= 0 {
		return "-"
	}
	return (time.Duration(seconds) * time.Second).String()
}

func formatIterations(max int) string {
	if max <= 0 {
		return "unlimited"
	}
	return strconv.Itoa(max)
}

func formatTime(value *time.Time) string {
	if value == nil {
		return "-"
	}
	return value.UTC().Format(time.RFC3339)
}

func formatRunDuration(run *models.LoopRun) string {
	if run == nil || run.StartedAt.IsZero() {
		return "-"
	}
	if run.FinishedAt == nil || run.FinishedAt.IsZero() {
		return "running"
	}
	duration := run.FinishedAt.Sub(run.StartedAt).Round(time.Second)
	if duration < 0 {
		duration = 0
	}
	return duration.String()
}

func runExitCode(run *models.LoopRun) string {
	if run == nil || run.ExitCode == nil {
		return "-"
	}
	return strconv.Itoa(*run.ExitCode)
}

func formatLineWindow(start, end, total, scroll int) string {
	if total <= 0 {
		return fmt.Sprintf("lines 0/0 scroll=%d", scroll)
	}
	visibleStart := start + 1
	if visibleStart < 1 {
		visibleStart = 1
	}
	if end < 0 {
		end = 0
	}
	if end > total {
		end = total
	}
	return fmt.Sprintf("lines %d-%d/%d scroll=%d", visibleStart, end, total, scroll)
}

func trimToHeight(lines []string, height int) []string {
	if height <= 0 || len(lines) <= height {
		return lines
	}
	return lines[:height]
}

func truncateLine(text string, width int) string {
	if width <= 0 {
		return ""
	}
	if lipgloss.Width(text) <= width {
		return text
	}
	if width <= 3 {
		return text[:width]
	}
	plain := []rune(stripANSI(text))
	if len(plain) <= width {
		return string(plain)
	}
	return string(plain[:width-3]) + "..."
}

func padRight(text string, width int) string {
	if width <= 0 {
		return ""
	}
	if len(text) >= width {
		return text[:width]
	}
	return text + strings.Repeat(" ", width-len(text))
}

func removeLastRune(value string) string {
	runes := []rune(value)
	if len(runes) == 0 {
		return ""
	}
	return string(runes[:len(runes)-1])
}

func defaultString(value, fallback string) string {
	if strings.TrimSpace(value) == "" {
		return fallback
	}
	return value
}

func exists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func stripANSI(in string) string {
	builder := strings.Builder{}
	builder.Grow(len(in))
	for i := 0; i < len(in); i++ {
		c := in[i]
		if c != 0x1b {
			builder.WriteByte(c)
			continue
		}
		if i+1 >= len(in) {
			break
		}
		next := in[i+1]
		switch next {
		case '[':
			i += 2
			for ; i < len(in); i++ {
				b := in[i]
				if b >= 0x40 && b <= 0x7E {
					break
				}
			}
		case ']':
			i += 2
			for ; i < len(in); i++ {
				if in[i] == 0x07 {
					break
				}
				if in[i] == 0x1b && i+1 < len(in) && in[i+1] == '\\' {
					i++
					break
				}
			}
		default:
			i++
		}
	}
	return builder.String()
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}
