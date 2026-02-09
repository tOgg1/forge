package fmailtui

import (
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
	"github.com/tOgg1/forge/internal/fmailtui/threading"
)

const (
	threadPageSize        = 100
	threadMaxDepth        = 6
	threadMaxBodyLines    = 50
	threadRefreshInterval = 2 * time.Second
)

type threadMode int

const (
	threadModeThreaded threadMode = iota
	threadModeFlat
)

type threadTickMsg struct{}

type threadLoadedMsg struct {
	now    time.Time
	topics []data.TopicInfo
	topic  string
	msgs   []fmail.Message
	total  int
	err    error
}

type threadExportResultMsg struct {
	path string
	err  error
}

type threadRow struct {
	msg         fmail.Message
	hasChildren bool
	connector   string
	depth       int
	overflow    bool
	groupGap    bool
	replyTo     string
	crossTarget string
	truncated   bool
	hiddenLines int
}

type threadView struct {
	root     string
	provider data.MessageProvider
	state    *tuistate.Manager

	now     time.Time
	lastErr error

	topics []data.TopicInfo
	topic  string

	mode threadMode

	limit        int
	total        int
	allMsgs      []fmail.Message
	msgByID      map[string]fmail.Message
	rows         []threadRow
	rowIndexByID map[string]int

	collapsed      map[string]bool
	expandedBodies map[string]bool
	readMarkers    map[string]string
	bookmarkedIDs  map[string]bool
	annotations    map[string]string

	selected int
	top      int

	lastWidth    int
	lastHeight   int
	viewportRows int
	pendingNew   int
	newestID     string

	rowCardCache      map[string][]string
	rowCardCacheTheme string
	rowCardCacheWidth int

	bookmarkConfirmID string

	editActive   bool
	editKind     string // "bookmark-note" | "annotation"
	editTargetID string
	editInput    string
	statusLine   string
	statusErr    bool

	initialized bool
}

var _ composeContextView = (*threadView)(nil)

func newThreadView(root string, provider data.MessageProvider, st *tuistate.Manager) *threadView {
	return &threadView{
		root:           root,
		provider:       provider,
		state:          st,
		mode:           threadModeThreaded,
		limit:          threadPageSize,
		collapsed:      make(map[string]bool),
		expandedBodies: make(map[string]bool),
		readMarkers:    make(map[string]string),
		bookmarkedIDs:  make(map[string]bool),
		annotations:    make(map[string]string),
		rowIndexByID:   make(map[string]int),
	}
}

func (v *threadView) ComposeTarget() string {
	return strings.TrimSpace(v.topic)
}

func (v *threadView) ComposeReplySeed(dmDirect bool) (composeReplySeed, bool) {
	row := v.selectedRow()
	if row == nil {
		return composeReplySeed{}, false
	}
	id := strings.TrimSpace(row.msg.ID)
	if id == "" {
		return composeReplySeed{}, false
	}

	target := strings.TrimSpace(v.topic)
	if dmDirect {
		from := strings.TrimSpace(row.msg.From)
		if from == "" {
			return composeReplySeed{}, false
		}
		if strings.HasPrefix(from, "@") {
			target = from
		} else {
			target = "@" + from
		}
	}
	if target == "" {
		return composeReplySeed{}, false
	}

	return composeReplySeed{
		Target:     target,
		ReplyTo:    id,
		ParentLine: firstNonEmptyLine(messageBodyString(row.msg.Body)),
	}, true
}

func (v *threadView) Init() tea.Cmd {
	v.loadState()
	return tea.Batch(v.loadCmd(), threadTickCmd())
}

func (v *threadView) SetTarget(target string) tea.Cmd {
	next := strings.TrimSpace(target)
	if next == "" {
		return nil
	}
	if next == v.topic {
		v.loadState()
		return v.loadCmd()
	}
	v.loadState()
	v.topic = next
	v.limit = threadPageSize
	v.total = 0
	v.pendingNew = 0
	v.selected = 0
	v.top = 0
	v.initialized = false
	return v.loadCmd()
}

func (v *threadView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case threadTickMsg:
		return tea.Batch(v.loadCmd(), threadTickCmd())
	case threadLoadedMsg:
		v.applyLoaded(typed)
		return nil
	case threadExportResultMsg:
		if typed.err != nil {
			v.statusLine = "export failed: " + typed.err.Error()
			v.statusErr = true
			return nil
		}
		v.statusLine = "exported: " + typed.path
		v.statusErr = false
		return nil
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *threadView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}

	v.lastWidth = width
	v.lastHeight = height

	palette := themePalette(theme)
	header := v.renderHeader(width, palette)
	meta := v.renderMeta(width, palette)

	reserved := 0
	if v.editActive {
		reserved += 4
	}
	if strings.TrimSpace(v.statusLine) != "" {
		reserved++
	}

	bodyHeight := height - lipgloss.Height(header) - lipgloss.Height(meta)
	bodyHeight -= reserved
	if bodyHeight < 1 {
		bodyHeight = 1
	}
	v.viewportRows = maxInt(1, bodyHeight/4)

	body := v.renderRows(width, bodyHeight, palette)
	lines := []string{header, meta, body}
	if v.editActive {
		lines = append(lines, v.renderEditPrompt(width, palette))
	}
	if strings.TrimSpace(v.statusLine) != "" {
		style := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
		if v.statusErr {
			style = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Bold(true)
		}
		lines = append(lines, style.Render(truncateVis(v.statusLine, width)))
	}
	content := lipgloss.JoinVertical(lipgloss.Left, lines...)
	if v.lastErr != nil {
		errLine := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Render("data error: " + truncate(v.lastErr.Error(), maxInt(0, width-2)))
		content = lipgloss.JoinVertical(lipgloss.Left, content, errLine)
	}

	base := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Background(lipgloss.Color(palette.Base.Background))
	return base.Render(content)
}

func (v *threadView) MinSize() (int, int) {
	return 40, 10
}

func (v *threadView) handleKey(msg tea.KeyMsg) tea.Cmd {
	if v.editActive {
		return v.handleEditKey(msg)
	}

	switch msg.String() {
	case "esc", "backspace":
		return popViewCmd()
	case "j", "down":
		v.bookmarkConfirmID = ""
		v.moveSelection(1)
		return nil
	case "k", "up":
		if cmd := v.maybeLoadOlder(); cmd != nil {
			return cmd
		}
		v.bookmarkConfirmID = ""
		v.moveSelection(-1)
		return nil
	case "ctrl+d":
		v.bookmarkConfirmID = ""
		v.moveSelection(maxInt(1, v.pageStep()))
		return nil
	case "ctrl+u":
		v.bookmarkConfirmID = ""
		v.moveSelection(-maxInt(1, v.pageStep()))
		return nil
	case "g":
		v.bookmarkConfirmID = ""
		v.selected = 0
		v.top = 0
		v.advanceReadMarker()
		return nil
	case "G", "end":
		v.bookmarkConfirmID = ""
		v.jumpBottom()
		return nil
	case "f":
		v.bookmarkConfirmID = ""
		if v.mode == threadModeThreaded {
			v.mode = threadModeFlat
		} else {
			v.mode = threadModeThreaded
		}
		anchor := v.selectedID()
		v.rebuildRows(anchor, false)
		v.ensureVisible()
		return nil
	case "b":
		v.toggleBookmark()
		return nil
	case "B":
		v.openBookmarkNoteEditor()
		return nil
	case "a":
		v.openAnnotationEditor()
		return nil
	case "X":
		return v.exportThreadCmd()
	case "[":
		v.bookmarkConfirmID = ""
		return v.switchTopic(-1)
	case "]":
		v.bookmarkConfirmID = ""
		return v.switchTopic(1)
	case "enter":
		v.bookmarkConfirmID = ""
		return v.handleEnter()
	}
	v.bookmarkConfirmID = ""
	return nil
}

func (v *threadView) switchTopic(delta int) tea.Cmd {
	if len(v.topics) == 0 {
		return nil
	}
	if strings.HasPrefix(strings.TrimSpace(v.topic), "@") {
		return nil
	}
	idx := v.topicIndex(v.topic)
	if idx < 0 {
		idx = 0
	}
	idx = (idx + delta + len(v.topics)) % len(v.topics)
	v.topic = v.topics[idx].Name
	v.limit = threadPageSize
	v.total = 0
	v.pendingNew = 0
	v.selected = 0
	v.top = 0
	v.initialized = false
	return v.loadCmd()
}

func (v *threadView) handleEnter() tea.Cmd {
	row := v.selectedRow()
	if row == nil {
		return nil
	}
	id := strings.TrimSpace(row.msg.ID)
	if id == "" {
		return nil
	}

	if row.truncated {
		v.expandedBodies[id] = true
		v.rebuildRows(id, false)
		v.ensureVisible()
		return nil
	}

	if v.mode == threadModeThreaded && row.hasChildren {
		v.collapsed[id] = !v.collapsed[id]
		v.rebuildRows(id, false)
		v.ensureVisible()
	}
	return nil
}

func (v *threadView) moveSelection(delta int) {
	if len(v.rows) == 0 {
		v.selected = 0
		v.top = 0
		return
	}
	v.selected = clampInt(v.selected+delta, 0, len(v.rows)-1)
	v.ensureVisible()
	v.advanceReadMarker()
}

func (v *threadView) pageStep() int {
	if v.viewportRows > 0 {
		return maxInt(1, v.viewportRows/2)
	}
	if v.lastHeight > 0 {
		return maxInt(1, v.lastHeight/8)
	}
	return 6
}

func (v *threadView) maybeLoadOlder() tea.Cmd {
	if v.selected > 0 {
		return nil
	}
	if v.limit <= 0 {
		v.limit = threadPageSize
	}
	// If we know the total and already have it, nothing to do.
	if v.total > 0 && len(v.allMsgs) >= v.total {
		return nil
	}
	// If we don't know total, assume "no more" when provider returned fewer than requested.
	if v.total == 0 && len(v.allMsgs) < v.limit {
		return nil
	}

	v.limit += threadPageSize
	return v.loadCmd()
}

func (v *threadView) jumpBottom() {
	if len(v.rows) == 0 {
		v.selected = 0
		v.top = 0
		v.pendingNew = 0
		return
	}
	v.selected = len(v.rows) - 1
	v.top = maxInt(0, v.selected-v.viewportRows+1)
	v.pendingNew = 0
	v.advanceReadMarker()
}

func (v *threadView) ensureVisible() {
	if len(v.rows) == 0 {
		v.selected = 0
		v.top = 0
		return
	}
	v.selected = clampInt(v.selected, 0, len(v.rows)-1)
	if v.selected < v.top {
		v.top = v.selected
	}
	visible := maxInt(1, v.viewportRows)
	if v.selected >= v.top+visible {
		v.top = v.selected - visible + 1
	}
	maxTop := maxInt(0, len(v.rows)-1)
	v.top = clampInt(v.top, 0, maxTop)
}

func threadTickCmd() tea.Cmd {
	return tea.Tick(threadRefreshInterval, func(time.Time) tea.Msg {
		return threadTickMsg{}
	})
}

func (v *threadView) rebuildRows(anchorID string, preferBottom bool) {
	msgs := v.allMsgs
	rows := make([]threadRow, 0, len(msgs))

	// Rebuild msg index + clear render cache (connectors/collapse state change).
	if v.msgByID == nil {
		v.msgByID = make(map[string]fmail.Message, len(msgs))
	} else {
		for k := range v.msgByID {
			delete(v.msgByID, k)
		}
	}
	for i := range msgs {
		if id := strings.TrimSpace(msgs[i].ID); id != "" {
			v.msgByID[id] = msgs[i]
		}
	}
	v.rowCardCache = nil
	v.rowCardCacheTheme = ""
	v.rowCardCacheWidth = 0

	if v.mode == threadModeFlat {
		sorted := append([]fmail.Message(nil), msgs...)
		sortMessages(sorted)
		for _, msg := range sorted {
			truncated, hidden := v.bodyTruncation(msg)
			rows = append(rows, threadRow{msg: msg, truncated: truncated, hiddenLines: hidden})
		}
	} else {
		threads := threading.BuildThreads(msgs)
		for tIdx, thread := range threads {
			if thread == nil {
				continue
			}
			nodes := threading.FlattenThread(thread)
			for _, node := range nodes {
				if node == nil || node.Message == nil {
					continue
				}
				if v.hiddenByCollapsedAncestor(node) {
					continue
				}

				depth := minInt(node.Depth, threadMaxDepth)
				connector, clamped := prefixForNode(node, threadMaxDepth)
				overflow := node.Depth > threadMaxDepth || clamped
				crossTarget := ""
				if threading.IsCrossTargetReply(node) && node.Parent != nil && node.Parent.Message != nil {
					crossTarget = strings.TrimSpace(node.Parent.Message.To)
				}
				truncated, hidden := v.bodyTruncation(*node.Message)
				rows = append(rows, threadRow{
					msg:         *node.Message,
					hasChildren: len(node.Children) > 0,
					connector:   connector,
					depth:       depth,
					overflow:    overflow,
					groupGap:    tIdx > 0 && node.Parent == nil,
					replyTo:     strings.TrimSpace(node.Message.ReplyTo),
					crossTarget: crossTarget,
					truncated:   truncated,
					hiddenLines: hidden,
				})
			}
		}
	}

	v.rows = rows
	v.rowIndexByID = make(map[string]int, len(rows))
	for idx := range rows {
		if id := strings.TrimSpace(rows[idx].msg.ID); id != "" {
			v.rowIndexByID[id] = idx
		}
	}

	if len(rows) == 0 {
		v.selected = 0
		v.top = 0
		return
	}
	if preferBottom {
		v.selected = len(rows) - 1
		v.top = maxInt(0, v.selected-v.viewportRows+1)
		return
	}
	if idx := v.indexForID(anchorID); idx >= 0 {
		v.selected = idx
	} else {
		v.selected = clampInt(v.selected, 0, len(rows)-1)
	}
}

func (v *threadView) hiddenByCollapsedAncestor(node *threading.ThreadNode) bool {
	for p := node.Parent; p != nil; p = p.Parent {
		if p.Message == nil {
			continue
		}
		if v.collapsed[p.Message.ID] {
			return true
		}
	}
	return false
}
