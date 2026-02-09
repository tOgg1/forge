package fmailtui

import (
	"fmt"
	"os"
	"sort"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

const (
	topicsRefreshInterval = 2 * time.Second
	topicsMetadataRefresh = 10 * time.Second
	topicsPreviewLimit    = 10
	topicsHotThreshold    = 5 * time.Minute
	topicsWarmThreshold   = time.Hour
	defaultSelfAgent      = "tui-viewer"
)

type topicsMode int

const (
	topicsModeTopics topicsMode = iota
	topicsModeDM
)

type topicSortKey int

const (
	topicSortActivity topicSortKey = iota
	topicSortName
	topicSortCount
	topicSortParticipants
)

type topicsTickMsg struct {
	now time.Time
}

type topicsLoadedMsg struct {
	now          time.Time
	topics       []data.TopicInfo
	dms          []data.DMConversation
	unreadByTop  map[string]int
	unreadByDM   map[string]int
	topicUpdated map[string]time.Time
	dmUpdated    map[string]time.Time
	err          error
}

type topicsPreviewLoadedMsg struct {
	target string
	msgs   []fmail.Message
	err    error
}

type topicsUnreadSnapshotMsg struct {
	unreadByTop map[string]int
	unreadByDM  map[string]int
	err         error
}

type topicsIncomingMsg struct {
	msg fmail.Message
}

type topicsSentMsg struct {
	target string
	msg    fmail.Message
	err    error
}

type topicsItem struct {
	target       string
	label        string
	messageCount int
	lastActivity time.Time
	participants []string
	unread       int
}

type topicsView struct {
	root     string
	provider data.MessageProvider
	state    *tuistate.Manager
	self     string

	now     time.Time
	lastErr error

	mode    topicsMode
	sortKey topicSortKey

	topics []data.TopicInfo
	dms    []data.DMConversation

	unreadByTop map[string]int
	unreadByDM  map[string]int

	filter       string
	filterActive bool

	items    []topicsItem
	selected int

	previewCache  map[string][]fmail.Message
	previewTarget string
	previewOffset int

	starred     map[string]bool
	readMarkers map[string]string
	statePath   string // legacy (kept for migration; set empty when using state manager)

	composeActive  bool
	composeSending bool
	composeTarget  string
	composeBody    string
	composeErr     error

	subCh     <-chan fmail.Message
	subCancel func()

	lastLoad time.Time
}

var _ composeContextView = (*topicsView)(nil)

func newTopicsView(root string, provider data.MessageProvider, st *tuistate.Manager) *topicsView {
	self := strings.TrimSpace(os.Getenv("FMAIL_AGENT"))
	if self == "" {
		self = defaultSelfAgent
	}

	return &topicsView{
		root:         root,
		provider:     provider,
		state:        st,
		self:         self,
		mode:         topicsModeTopics,
		sortKey:      topicSortActivity,
		unreadByTop:  make(map[string]int),
		unreadByDM:   make(map[string]int),
		previewCache: make(map[string][]fmail.Message),
		starred:      make(map[string]bool),
		readMarkers:  make(map[string]string),
		statePath:    "",
	}
}

func (v *topicsView) ComposeTarget() string {
	return v.selectedTarget()
}

func (v *topicsView) ComposeReplySeed(_ bool) (composeReplySeed, bool) {
	return composeReplySeed{}, false
}

func (v *topicsView) Init() tea.Cmd {
	v.loadState()
	v.startSubscription()
	return tea.Batch(v.loadCmd(), topicsTickCmd(), v.waitForMessageCmd())
}

func (v *topicsView) Close() {
	if v.subCancel != nil {
		v.subCancel()
		v.subCancel = nil
	}
	v.subCh = nil
}

func (v *topicsView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case topicsTickMsg:
		// Refresh read markers/starred topics in case another view updated the state file.
		prevMarkers := cloneStringMap(v.readMarkers)
		v.loadState()
		cmds := []tea.Cmd{topicsTickCmd()}
		if !stringMapEqual(prevMarkers, v.readMarkers) {
			cmds = append(cmds, v.recomputeUnreadTargetsCmd(changedMarkerKeys(prevMarkers, v.readMarkers)))
		}
		if v.shouldRefresh(typed.now) {
			cmds = append(cmds, v.loadCmd())
		}
		return tea.Batch(cmds...)
	case topicsLoadedMsg:
		v.applyLoaded(typed)
		return v.ensurePreviewCmd()
	case topicsPreviewLoadedMsg:
		v.applyPreview(typed)
		return nil
	case topicsUnreadSnapshotMsg:
		if typed.err != nil {
			v.lastErr = typed.err
			return nil
		}
		if v.unreadByTop == nil {
			v.unreadByTop = make(map[string]int)
		}
		for key, value := range typed.unreadByTop {
			v.unreadByTop[key] = value
		}
		if v.unreadByDM == nil {
			v.unreadByDM = make(map[string]int)
		}
		for key, value := range typed.unreadByDM {
			v.unreadByDM[key] = value
		}
		v.rebuildItems()
		return nil
	case topicsIncomingMsg:
		v.applyIncoming(typed.msg)
		return tea.Batch(v.waitForMessageCmd(), v.ensurePreviewCmd())
	case topicsSentMsg:
		v.composeSending = false
		v.composeErr = typed.err
		if typed.err != nil {
			return nil
		}

		v.composeActive = false
		v.composeBody = ""
		v.composeErr = nil

		if typed.target != "" && typed.msg.ID != "" {
			// Treat own send as read.
			v.loadState()
			if v.readMarkers == nil {
				v.readMarkers = make(map[string]string)
			}
			v.readMarkers[typed.target] = typed.msg.ID
			_ = v.saveState()

			// Ensure preview shows the new message before next refresh.
			v.previewCache[typed.target] = append(v.previewCache[typed.target], typed.msg)
			if len(v.previewCache[typed.target]) > topicsPreviewLimit {
				v.previewCache[typed.target] = v.previewCache[typed.target][len(v.previewCache[typed.target])-topicsPreviewLimit:]
			}
		}
		v.rebuildItems()
		return v.ensurePreviewCmd()
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *topicsView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}

	palette := themePalette(theme)
	if v.now.IsZero() {
		v.now = time.Now().UTC()
	}

	var body string
	if width < 96 {
		listHeight := maxInt(8, height/2)
		previewHeight := maxInt(6, height-listHeight)
		listPanel := v.renderListPanel(width, listHeight, palette)
		previewPanel := v.renderPreviewPanel(width, previewHeight, palette)
		body = lipgloss.JoinVertical(lipgloss.Left, listPanel, previewPanel)
	} else {
		listWidth := maxInt(38, width/2)
		previewWidth := maxInt(28, width-listWidth-1)
		listPanel := v.renderListPanel(listWidth, height, palette)
		previewPanel := v.renderPreviewPanel(previewWidth, height, palette)
		body = lipgloss.JoinHorizontal(lipgloss.Top, listPanel, previewPanel)
	}

	if v.lastErr != nil {
		errLine := lipgloss.NewStyle().
			Foreground(lipgloss.Color(palette.Priority.High)).
			Render("data error: " + truncate(v.lastErr.Error(), maxInt(0, width-2)))
		body = lipgloss.JoinVertical(lipgloss.Left, body, errLine)
	}

	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Background(lipgloss.Color(palette.Base.Background)).
		Render(body)
}

func (v *topicsView) MinSize() (int, int) {
	return 44, 10
}

func (v *topicsView) handleKey(msg tea.KeyMsg) tea.Cmd {
	if v.composeActive {
		switch msg.Type {
		case tea.KeyEsc:
			if v.composeSending {
				return nil
			}
			v.composeActive = false
			v.composeSending = false
			v.composeTarget = ""
			v.composeBody = ""
			v.composeErr = nil
			return nil
		case tea.KeyBackspace, tea.KeyDelete:
			if v.composeSending {
				return nil
			}
			if len(v.composeBody) == 0 {
				return nil
			}
			runes := []rune(v.composeBody)
			v.composeBody = string(runes[:len(runes)-1])
			return nil
		case tea.KeyEnter:
			if v.composeSending {
				return nil
			}
			v.composeSending = true
			v.composeErr = nil
			return v.sendCmd(v.composeTarget, v.composeBody)
		case tea.KeyRunes:
			if v.composeSending {
				return nil
			}
			v.composeBody += string(msg.Runes)
			return nil
		}
		return nil
	}

	switch msg.Type {
	case tea.KeyEsc:
		if v.filterActive {
			v.filterActive = false
			return nil
		}
		return popViewCmd()
	case tea.KeyBackspace, tea.KeyDelete:
		if v.filterActive && len(v.filter) > 0 {
			runes := []rune(v.filter)
			v.filter = string(runes[:len(runes)-1])
			v.rebuildItems()
			return nil
		}
	case tea.KeyEnter:
		if v.filterActive {
			v.filterActive = false
			return nil
		}
	case tea.KeyRunes:
		if v.filterActive {
			v.filter += string(msg.Runes)
			v.rebuildItems()
			return nil
		}
	}

	switch msg.String() {
	case "/":
		v.filterActive = true
		return nil
	case "j", "down":
		v.moveSelection(1)
		return v.ensurePreviewCmd()
	case "k", "up":
		v.moveSelection(-1)
		return v.ensurePreviewCmd()
	case "ctrl+d", "pgdown":
		v.scrollPreview(5)
		return nil
	case "ctrl+u", "pgup":
		v.scrollPreview(-5)
		return nil
	case "s":
		v.sortKey = nextTopicSortKey(v.sortKey)
		v.rebuildItems()
		return nil
	case "d":
		if v.mode == topicsModeTopics {
			v.mode = topicsModeDM
		} else {
			v.mode = topicsModeTopics
		}
		v.previewOffset = 0
		v.rebuildItems()
		return v.ensurePreviewCmd()
	case "*":
		if v.mode == topicsModeTopics {
			// Reload first to avoid clobbering read markers updated by other views.
			v.loadState()
			v.toggleStarSelected()
			v.rebuildItems()
		}
		return nil
	case "enter":
		target := v.selectedTarget()
		if target == "" {
			return pushViewCmd(ViewThread)
		}
		return tea.Batch(openThreadCmd(target, ""), pushViewCmd(ViewThread))
	case "n":
		target := v.selectedTarget()
		if target == "" {
			return nil
		}
		v.composeActive = true
		v.composeSending = false
		v.composeTarget = target
		v.composeBody = ""
		v.composeErr = nil
		return nil
	}

	return nil
}

func (v *topicsView) renderListPanel(width, height int, palette styles.Theme) string {
	panel := styles.PanelStyle(palette, true)
	innerW := maxInt(0, width-(styles.LayoutInnerPadding*2)-2)
	innerH := maxInt(1, height-(styles.LayoutInnerPadding*2)-2)

	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb))
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))

	title := "Topics"
	if v.mode == topicsModeDM {
		title = "DM Browser"
	}
	sortLabel := topicSortLabel(v.sortKey)
	filterSuffix := ""
	if v.filterActive {
		filterSuffix = "_"
	}
	titleLine := titleStyle.Render(title) + muted.Render(truncateVis(fmt.Sprintf("  (%d)  sort:%s", len(v.items), sortLabel), maxInt(0, innerW-lipgloss.Width(title))))

	hints := "j/k move  Enter open  / filter  d toggle  s sort  n compose  Esc back"
	if v.mode == topicsModeTopics {
		hints = "j/k move  Enter open  / filter  d toggle  s sort  * star  n compose  Esc back"
	}
	keyLine := muted.Render(truncateVis(hints, innerW))

	filterLabel := "Filter: " + v.filter + filterSuffix + "  (/ to edit)"
	filterStyle := muted
	if v.filterActive {
		filterStyle = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true)
	}
	filterLine := filterStyle.Render(truncateVis(filterLabel, innerW))

	header := "TOPIC                H  MSGS  LAST ACTIVE  AGENTS               UNRD"
	if v.mode == topicsModeDM {
		header = "DM                   H  MSGS  LAST ACTIVE  UNRD"
	}
	header = lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color(palette.Base.Muted)).
		Render(truncateVis(header, innerW))

	used := lipgloss.Height(titleLine) + lipgloss.Height(keyLine) + lipgloss.Height(filterLine) + lipgloss.Height(header) + 1
	rowsH := maxInt(1, innerH-used)
	rows := v.renderRows(innerW, rowsH, palette)
	content := lipgloss.JoinVertical(lipgloss.Left, titleLine, keyLine, filterLine, header, rows)
	return panel.Width(width).Height(height).Render(content)
}

func (v *topicsView) renderRows(width, maxRows int, palette styles.Theme) string {
	if maxRows <= 0 {
		return ""
	}
	if len(v.items) == 0 {
		empty := "No topics"
		if v.mode == topicsModeDM {
			empty = "No DM conversations"
		}
		if strings.TrimSpace(v.filter) != "" {
			empty = fmt.Sprintf("No matches for %q", strings.TrimSpace(v.filter))
		}
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(empty)
	}

	v.selected = clampInt(v.selected, 0, len(v.items)-1)
	start := maxInt(0, v.selected-maxRows/2)
	if start+maxRows > len(v.items) {
		start = maxInt(0, len(v.items)-maxRows)
	}

	lines := make([]string, 0, maxRows)
	for idx := start; idx < len(v.items) && len(lines) < maxRows; idx++ {
		item := v.items[idx]
		cursor := " "
		if idx == v.selected {
			cursor = "▸"
		}
		unreadStyle := lipgloss.NewStyle()
		if item.unread > 0 {
			unreadStyle = unreadStyle.Bold(true)
		}

		lastActive := relativeTime(item.lastActivity, v.now)
		heat := v.activityHeatRune(item.lastActivity, palette)
		line := ""
		if v.mode == topicsModeTopics {
			star := " "
			if v.starred[item.label] {
				star = "★"
			}
			participants := truncate(strings.Join(item.participants, ", "), 20)
			line = fmt.Sprintf("%s%s %-20s %s %4d  %-11s %-20s %4d", cursor, star, truncate(item.label, 20), heat, item.messageCount, lastActive, participants, item.unread)
		} else {
			line = fmt.Sprintf("%s  %-20s %s %4d  %-11s %4d", cursor, truncate(item.target, 20), heat, item.messageCount, lastActive, item.unread)
		}

		line = truncateVis(line, width)
		rowStyle := unreadStyle
		if idx == v.selected {
			// Highlight the whole row; cursor-only selection gets lost in dense lists.
			rowStyle = rowStyle.Background(lipgloss.Color(palette.Chrome.SelectedItem)).Bold(true)
		}
		lines = append(lines, rowStyle.Render(line))
	}

	return strings.Join(lines, "\n")
}

func (v *topicsView) renderPreviewPanel(width, height int, palette styles.Theme) string {
	panel := styles.PanelStyle(palette, false)
	innerW := maxInt(0, width-(styles.LayoutInnerPadding*2)-2)
	innerH := maxInt(1, height-(styles.LayoutInnerPadding*2)-2)

	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb))
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))

	target := v.selectedTarget()
	title := "Preview"
	if target != "" {
		title = "Preview: " + target
	}
	titleLine := titleStyle.Render(truncateVis(title, innerW))
	meta := "ctrl+u/d scroll  Enter open  n compose  Esc back"
	if v.composeActive {
		draft := truncateVis(firstLine(v.composeBody), maxInt(0, innerW-24))
		meta = fmt.Sprintf("Compose to %s: %s_", target, draft)
		meta = meta + "  (Enter send, Esc cancel)"
		if v.composeSending {
			meta = "Sending..."
		} else if v.composeErr != nil {
			meta = "Send failed: " + v.composeErr.Error()
		}
	}
	metaLine := muted.Render(truncateVis(meta, innerW))
	if v.composeErr != nil {
		metaLine = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Bold(true).Render(truncateVis(meta, innerW))
	}

	used := lipgloss.Height(titleLine) + lipgloss.Height(metaLine) + 1
	bodyH := maxInt(1, innerH-used)
	body := v.renderPreviewLines(target, innerW, bodyH, palette)

	content := lipgloss.JoinVertical(lipgloss.Left, titleLine, body, metaLine)
	return panel.Width(width).Height(height).Render(content)
}

func (v *topicsView) renderPreviewLines(target string, width, maxLines int, palette styles.Theme) string {
	if maxLines <= 0 {
		return ""
	}
	if target == "" {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Select a topic or DM")
	}

	msgs, ok := v.previewCache[target]
	if !ok {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Loading preview...")
	}
	if len(msgs) == 0 {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("No messages")
	}

	mapper := styles.NewAgentColorMapperWithPalette(palette.AgentPalette)
	lines := make([]string, 0, len(msgs)*3)
	for _, msg := range msgs {
		ts := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(msg.Time.UTC().Format("15:04"))
		from := mapper.Foreground(msg.From).Render(msg.From)
		lines = append(lines, truncateVis(ts+" "+from, width))

		body := strings.TrimSpace(messageBodyString(msg.Body))
		if body == "" {
			body = "(empty)"
		}
		wrapped := wrapLines(firstLine(body), maxInt(8, width-2))
		for _, part := range wrapped {
			lines = append(lines, truncateVis("  "+part, width))
		}
		lines = append(lines, "")
	}
	if len(lines) > 0 && lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}

	maxOffset := maxInt(0, len(lines)-maxLines)
	start := clampInt(v.previewOffset, 0, maxOffset)
	end := minInt(len(lines), start+maxLines)
	return strings.Join(lines[start:end], "\n")
}

func (v *topicsView) loadCmd() tea.Cmd {
	if v.provider == nil {
		return func() tea.Msg {
			return topicsLoadedMsg{now: time.Now().UTC(), err: fmt.Errorf("missing provider")}
		}
	}

	self := v.self
	return func() tea.Msg {
		now := time.Now().UTC()
		topics, err := v.provider.Topics()
		if err != nil {
			return topicsLoadedMsg{now: now, err: err}
		}

		dms, err := v.provider.DMConversations(self)
		if err != nil {
			return topicsLoadedMsg{now: now, topics: topics, err: err}
		}

		topicUpdated := make(map[string]time.Time, len(topics))
		for _, topic := range topics {
			topicUpdated[topic.Name] = topic.LastActivity
		}

		dmUpdated := make(map[string]time.Time, len(dms))
		for _, conv := range dms {
			dmUpdated["@"+conv.Agent] = conv.LastActivity
		}

		return topicsLoadedMsg{
			now:          now,
			topics:       topics,
			dms:          dms,
			topicUpdated: topicUpdated,
			dmUpdated:    dmUpdated,
		}
	}
}

func topicsTickCmd() tea.Cmd {
	return tea.Tick(topicsRefreshInterval, func(ts time.Time) tea.Msg {
		return topicsTickMsg{now: ts.UTC()}
	})
}

func (v *topicsView) ensurePreviewCmd() tea.Cmd {
	target := v.selectedTarget()
	v.previewTarget = target
	if target == "" {
		return nil
	}
	if _, ok := v.previewCache[target]; ok {
		return nil
	}
	return v.loadPreviewCmd(target)
}

func (v *topicsView) loadPreviewCmd(target string) tea.Cmd {
	if strings.TrimSpace(target) == "" {
		return nil
	}
	return func() tea.Msg {
		var (
			msgs []fmail.Message
			err  error
		)
		if strings.HasPrefix(target, "@") {
			msgs, err = v.provider.DMs(strings.TrimPrefix(target, "@"), data.MessageFilter{
				To:    "@" + v.self,
				Limit: topicsPreviewLimit,
			})
		} else {
			msgs, err = v.provider.Messages(target, data.MessageFilter{Limit: topicsPreviewLimit})
		}
		return topicsPreviewLoadedMsg{target: target, msgs: msgs, err: err}
	}
}

func (v *topicsView) applyLoaded(msg topicsLoadedMsg) {
	v.now = msg.now
	v.lastErr = msg.err
	if msg.err != nil {
		return
	}
	v.lastLoad = msg.now

	prevTarget := v.selectedTarget()
	prevTopicUpdated := make(map[string]time.Time, len(v.topics))
	prevDMUpdated := make(map[string]time.Time, len(v.dms))
	prevTopicCount := make(map[string]int, len(v.topics))
	prevDMCount := make(map[string]int, len(v.dms))
	for _, topic := range v.topics {
		prevTopicUpdated[topic.Name] = topic.LastActivity
		prevTopicCount[topic.Name] = topic.MessageCount
	}
	for _, conv := range v.dms {
		prevDMUpdated["@"+conv.Agent] = conv.LastActivity
		prevDMCount[conv.Agent] = conv.MessageCount
	}

	v.topics = append([]data.TopicInfo(nil), msg.topics...)
	v.dms = append([]data.DMConversation(nil), msg.dms...)
	v.syncUnreadFromMetadata(prevTopicCount, prevDMCount)

	for key, old := range prevTopicUpdated {
		next, ok := msg.topicUpdated[key]
		if !ok || !next.Equal(old) {
			delete(v.previewCache, key)
		}
	}
	for key, old := range prevDMUpdated {
		next, ok := msg.dmUpdated[key]
		if !ok || !next.Equal(old) {
			delete(v.previewCache, key)
		}
	}

	v.rebuildItems()
	if prevTarget != "" {
		v.selectTarget(prevTarget)
	}
}

func (v *topicsView) applyPreview(msg topicsPreviewLoadedMsg) {
	if msg.err != nil {
		v.lastErr = msg.err
		return
	}
	sortMessages(msg.msgs)
	if len(msg.msgs) > topicsPreviewLimit {
		msg.msgs = msg.msgs[len(msg.msgs)-topicsPreviewLimit:]
	}
	v.previewCache[msg.target] = append([]fmail.Message(nil), msg.msgs...)
	if msg.target == v.previewTarget {
		v.previewOffset = 0
	}
}

func (v *topicsView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.subCancel = cancel
}

func (v *topicsView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return topicsIncomingMsg{msg: msg}
	}
}

func (v *topicsView) applyIncoming(msg fmail.Message) {
	target := strings.TrimSpace(msg.To)
	if target == "" {
		return
	}

	cacheTarget := target
	if strings.HasPrefix(target, "@") {
		peer := dmPeerForSelf(v.self, msg)
		if peer == "" {
			return
		}
		cacheTarget = "@" + peer
		v.bumpDMConversation(peer, msg)
		if !strings.EqualFold(strings.TrimSpace(msg.From), strings.TrimSpace(v.self)) {
			marker := readMarkerForTarget(v.readMarkers, cacheTarget)
			if marker == "" || strings.TrimSpace(msg.ID) > marker {
				v.unreadByDM[peer] = v.unreadByDM[peer] + 1
			}
		}
	} else {
		v.bumpTopic(target, msg)
		if !strings.EqualFold(strings.TrimSpace(msg.From), strings.TrimSpace(v.self)) {
			marker := readMarkerForTarget(v.readMarkers, target)
			if marker == "" || strings.TrimSpace(msg.ID) > marker {
				v.unreadByTop[target] = v.unreadByTop[target] + 1
			}
		}
	}

	v.previewCache[cacheTarget] = append(v.previewCache[cacheTarget], msg)
	if len(v.previewCache[cacheTarget]) > topicsPreviewLimit {
		v.previewCache[cacheTarget] = v.previewCache[cacheTarget][len(v.previewCache[cacheTarget])-topicsPreviewLimit:]
	}
	v.now = time.Now().UTC()
	v.rebuildItems()
}

func (v *topicsView) rebuildItems() {
	filter := strings.ToLower(strings.TrimSpace(v.filter))
	items := make([]topicsItem, 0, len(v.topics))

	if v.mode == topicsModeTopics {
		for _, topic := range v.topics {
			item := topicsItem{
				target:       topic.Name,
				label:        topic.Name,
				messageCount: topic.MessageCount,
				lastActivity: topic.LastActivity,
				participants: append([]string(nil), topic.Participants...),
				unread:       v.unreadByTop[topic.Name],
			}
			if filter != "" && !topicMatchesFilter(item, filter) {
				continue
			}
			items = append(items, item)
		}
	} else {
		items = items[:0]
		for _, conv := range v.dms {
			target := "@" + conv.Agent
			item := topicsItem{
				target:       target,
				label:        conv.Agent,
				messageCount: conv.MessageCount,
				lastActivity: conv.LastActivity,
				participants: []string{conv.Agent},
				unread:       v.unreadByDM[conv.Agent],
			}
			if filter != "" && !topicMatchesFilter(item, filter) {
				continue
			}
			items = append(items, item)
		}
	}

	sort.SliceStable(items, func(i, j int) bool {
		left := items[i]
		right := items[j]

		if v.mode == topicsModeTopics {
			leftStar := v.starred[left.label]
			rightStar := v.starred[right.label]
			if leftStar != rightStar {
				return leftStar
			}
		}

		switch v.sortKey {
		case topicSortName:
			leftName := strings.ToLower(left.label)
			rightName := strings.ToLower(right.label)
			if leftName != rightName {
				return leftName < rightName
			}
		case topicSortCount:
			if left.messageCount != right.messageCount {
				return left.messageCount > right.messageCount
			}
		case topicSortParticipants:
			if len(left.participants) != len(right.participants) {
				return len(left.participants) > len(right.participants)
			}
		default:
			if !left.lastActivity.Equal(right.lastActivity) {
				return left.lastActivity.After(right.lastActivity)
			}
		}
		return strings.ToLower(left.label) < strings.ToLower(right.label)
	})

	v.items = items
	if len(v.items) == 0 {
		v.selected = 0
		v.previewTarget = ""
		return
	}
	v.selected = clampInt(v.selected, 0, len(v.items)-1)
}

func (v *topicsView) moveSelection(delta int) {
	if len(v.items) == 0 {
		v.selected = 0
		return
	}
	next := clampInt(v.selected+delta, 0, len(v.items)-1)
	if next != v.selected {
		v.selected = next
		v.previewOffset = 0
	}
}

func (v *topicsView) scrollPreview(delta int) {
	if strings.TrimSpace(v.previewTarget) == "" {
		return
	}
	v.previewOffset = maxInt(0, v.previewOffset+delta)
}

func (v *topicsView) selectedTarget() string {
	if len(v.items) == 0 || v.selected < 0 || v.selected >= len(v.items) {
		return ""
	}
	return v.items[v.selected].target
}

func (v *topicsView) selectTarget(target string) {
	for i := range v.items {
		if v.items[i].target == target {
			v.selected = i
			v.previewTarget = target
			return
		}
	}
}

func (v *topicsView) toggleStarSelected() {
	if len(v.items) == 0 {
		return
	}
	name := strings.TrimSpace(v.items[v.selected].label)
	if name == "" {
		return
	}
	if v.starred[name] {
		delete(v.starred, name)
	} else {
		v.starred[name] = true
	}
	if err := v.saveState(); err != nil {
		v.lastErr = err
	}
}

func (v *topicsView) activityHeatRune(lastActivity time.Time, palette styles.Theme) string {
	age := v.now.Sub(lastActivity)
	switch {
	case !lastActivity.IsZero() && age <= topicsHotThreshold:
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Render("●")
	case !lastActivity.IsZero() && age <= topicsWarmThreshold:
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Status.Recent)).Render("●")
	default:
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Status.Stale)).Render("●")
	}
}

func (v *topicsView) loadState() {
	if v.state == nil {
		return
	}
	snap := v.state.Snapshot()

	v.readMarkers = make(map[string]string, len(snap.ReadMarkers))
	for key, marker := range snap.ReadMarkers {
		key = strings.TrimSpace(key)
		marker = strings.TrimSpace(marker)
		if key == "" || marker == "" {
			continue
		}
		v.readMarkers[key] = marker
	}

	v.starred = make(map[string]bool, len(snap.StarredTopics))
	for _, topic := range snap.StarredTopics {
		name := strings.TrimSpace(topic)
		if name == "" {
			continue
		}
		v.starred[name] = true
	}
}

func (v *topicsView) saveState() error {
	if v.state == nil {
		return nil
	}
	v.state.SetReadMarkers(cloneStringMap(v.readMarkers))
	v.state.SetStarredTopics(sortedStarred(v.starred))
	v.state.SaveSoon()
	return nil
}

func (v *topicsView) shouldRefresh(now time.Time) bool {
	if now.IsZero() {
		now = time.Now().UTC()
	}
	if v.lastLoad.IsZero() {
		return true
	}
	return now.Sub(v.lastLoad) >= topicsMetadataRefresh
}

func (v *topicsView) recomputeUnreadCmd() tea.Cmd {
	if v.provider == nil {
		return nil
	}
	return v.recomputeUnreadTargetsCmd(changedMarkerKeys(nil, v.readMarkers))
}

func (v *topicsView) recomputeUnreadTargetsCmd(targets []string) tea.Cmd {
	if v.provider == nil {
		return nil
	}
	targets = normalizeMarkerTargets(targets)
	if len(targets) == 0 {
		return nil
	}
	readMarkers := cloneStringMap(v.readMarkers)
	self := v.self
	return func() tea.Msg {
		outTop := make(map[string]int)
		outDM := make(map[string]int)

		topicTotals := make(map[string]int, len(v.topics))
		for _, topic := range v.topics {
			topicTotals[topic.Name] = topic.MessageCount
		}

		for _, target := range targets {
			target = strings.TrimSpace(target)
			if target == "" {
				continue
			}
			isDM := strings.HasPrefix(target, "@")
			if !isDM {
				for _, conv := range v.dms {
					if conv.Agent == target {
						isDM = true
						target = "@" + target
						break
					}
				}
			}

			if isDM {
				peer := strings.TrimPrefix(target, "@")
				if peer == "" {
					continue
				}
				marker := readMarkerForTarget(readMarkers, target)
				count, err := unreadCountForDM(v.provider, self, peer, marker)
				if err != nil {
					return topicsUnreadSnapshotMsg{err: err}
				}
				outDM[peer] = count
				continue
			}

			total := topicTotals[target]
			marker := readMarkerForTarget(readMarkers, target)
			count, err := unreadCountForTopic(v.provider, target, marker, total)
			if err != nil {
				return topicsUnreadSnapshotMsg{err: err}
			}
			outTop[target] = count
		}

		return topicsUnreadSnapshotMsg{unreadByTop: outTop, unreadByDM: outDM}
	}
}

func (v *topicsView) syncUnreadFromMetadata(prevTopicCount map[string]int, prevDMCount map[string]int) {
	if v.unreadByTop == nil {
		v.unreadByTop = make(map[string]int)
	}
	if v.unreadByDM == nil {
		v.unreadByDM = make(map[string]int)
	}

	nextTop := make(map[string]int, len(v.topics))
	for _, topic := range v.topics {
		name := topic.Name
		prevUnread := maxInt(0, v.unreadByTop[name])
		prevCount, had := prevTopicCount[name]
		if !had || topic.MessageCount < prevCount {
			marker := readMarkerForTarget(v.readMarkers, name)
			count, err := unreadCountForTopic(v.provider, name, marker, topic.MessageCount)
			if err != nil {
				v.lastErr = err
				count = prevUnread
			}
			nextTop[name] = maxInt(0, count)
			continue
		}
		if topic.MessageCount > prevCount {
			nextTop[name] = prevUnread + (topic.MessageCount - prevCount)
			continue
		}
		nextTop[name] = prevUnread
	}

	nextDM := make(map[string]int, len(v.dms))
	for _, conv := range v.dms {
		peer := conv.Agent
		prevUnread := maxInt(0, v.unreadByDM[peer])
		prevCount, had := prevDMCount[peer]
		if !had || conv.MessageCount < prevCount {
			marker := readMarkerForTarget(v.readMarkers, "@"+peer)
			count, err := unreadCountForDM(v.provider, v.self, peer, marker)
			if err != nil {
				v.lastErr = err
				count = prevUnread
			}
			nextDM[peer] = maxInt(0, count)
			continue
		}
		if conv.MessageCount > prevCount {
			// Incremental: assume new messages are unread. (Exact unread needs message bodies.)
			nextDM[peer] = prevUnread + (conv.MessageCount - prevCount)
			continue
		}
		nextDM[peer] = prevUnread
	}

	v.unreadByTop = nextTop
	v.unreadByDM = nextDM
}

func (v *topicsView) bumpTopic(name string, msg fmail.Message) {
	for i := range v.topics {
		if v.topics[i].Name != name {
			continue
		}
		v.topics[i].MessageCount++
		if msg.Time.After(v.topics[i].LastActivity) {
			v.topics[i].LastActivity = msg.Time.UTC()
		}
		if from := strings.TrimSpace(msg.From); from != "" && !containsStringFold(v.topics[i].Participants, from) {
			v.topics[i].Participants = append(v.topics[i].Participants, from)
		}
		return
	}

	info := data.TopicInfo{
		Name:         name,
		MessageCount: 1,
		LastActivity: msg.Time.UTC(),
	}
	if from := strings.TrimSpace(msg.From); from != "" {
		info.Participants = []string{from}
	}
	v.topics = append(v.topics, info)
}

func (v *topicsView) bumpDMConversation(peer string, msg fmail.Message) {
	for i := range v.dms {
		if v.dms[i].Agent != peer {
			continue
		}
		v.dms[i].MessageCount++
		if msg.Time.After(v.dms[i].LastActivity) {
			v.dms[i].LastActivity = msg.Time.UTC()
		}
		return
	}

	v.dms = append(v.dms, data.DMConversation{
		Agent:        peer,
		MessageCount: 1,
		LastActivity: msg.Time.UTC(),
	})
}

func containsStringFold(items []string, needle string) bool {
	needle = strings.TrimSpace(needle)
	if needle == "" {
		return false
	}
	for _, item := range items {
		if strings.EqualFold(strings.TrimSpace(item), needle) {
			return true
		}
	}
	return false
}

func unreadCountForTopic(provider data.MessageProvider, topic string, marker string, total int) (int, error) {
	marker = strings.TrimSpace(marker)
	if marker == "" {
		if total > 0 {
			return total, nil
		}
		msgs, err := provider.Messages(topic, data.MessageFilter{})
		if err != nil {
			return 0, err
		}
		return len(msgs), nil
	}
	msgs, err := provider.Messages(topic, data.MessageFilter{})
	if err != nil {
		return 0, err
	}
	count := 0
	for _, msg := range msgs {
		if msg.ID > marker {
			count++
		}
	}
	return count, nil
}

func unreadCountForDM(provider data.MessageProvider, self, peer, marker string) (int, error) {
	marker = strings.TrimSpace(marker)
	msgs, err := provider.DMs(peer, data.MessageFilter{To: "@" + self})
	if err != nil {
		return 0, err
	}
	count := 0
	for _, msg := range msgs {
		if marker != "" && msg.ID <= marker {
			continue
		}
		if self != "" && strings.EqualFold(strings.TrimSpace(msg.From), strings.TrimSpace(self)) {
			continue
		}
		count++
	}
	return count, nil
}

func topicMatchesFilter(item topicsItem, filter string) bool {
	blob := strings.ToLower(item.label + " " + item.target + " " + strings.Join(item.participants, " "))
	return strings.Contains(blob, filter)
}

func readMarkerForTarget(markers map[string]string, target string) string {
	target = strings.TrimSpace(target)
	if target == "" {
		return ""
	}
	if marker := strings.TrimSpace(markers[target]); marker != "" {
		return marker
	}
	if strings.HasPrefix(target, "@") {
		if marker := strings.TrimSpace(markers[strings.TrimPrefix(target, "@")]); marker != "" {
			return marker
		}
	}
	return ""
}

func cloneStringMap(src map[string]string) map[string]string {
	if len(src) == 0 {
		return map[string]string{}
	}
	dst := make(map[string]string, len(src))
	for key, value := range src {
		dst[key] = value
	}
	return dst
}

func stringMapEqual(left map[string]string, right map[string]string) bool {
	if len(left) != len(right) {
		return false
	}
	for key, value := range left {
		if right[key] != value {
			return false
		}
	}
	return true
}

func changedMarkerKeys(prev map[string]string, next map[string]string) []string {
	seen := make(map[string]bool, len(prev)+len(next))
	out := make([]string, 0, minInt(8, len(next)))

	for key, value := range next {
		key = strings.TrimSpace(key)
		if key == "" {
			continue
		}
		seen[key] = true
		if strings.TrimSpace(prev[key]) != strings.TrimSpace(value) {
			out = append(out, key)
		}
	}
	for key := range prev {
		key = strings.TrimSpace(key)
		if key == "" || seen[key] {
			continue
		}
		// Marker removed.
		out = append(out, key)
	}
	return out
}

func normalizeMarkerTargets(targets []string) []string {
	if len(targets) == 0 {
		return nil
	}
	seen := make(map[string]bool, len(targets))
	out := make([]string, 0, len(targets))
	for _, target := range targets {
		target = strings.TrimSpace(target)
		if target == "" || seen[target] {
			continue
		}
		seen[target] = true
		out = append(out, target)
	}
	return out
}

func sortedStarred(starred map[string]bool) []string {
	if len(starred) == 0 {
		return nil
	}
	out := make([]string, 0, len(starred))
	for name, on := range starred {
		if !on {
			continue
		}
		out = append(out, name)
	}
	sort.Strings(out)
	return out
}

func nextTopicSortKey(sortKey topicSortKey) topicSortKey {
	switch sortKey {
	case topicSortName:
		return topicSortCount
	case topicSortCount:
		return topicSortActivity
	case topicSortActivity:
		return topicSortParticipants
	default:
		return topicSortName
	}
}

func topicSortLabel(sortKey topicSortKey) string {
	switch sortKey {
	case topicSortName:
		return "name"
	case topicSortCount:
		return "count"
	case topicSortParticipants:
		return "participants"
	default:
		return "activity"
	}
}

func dmPeerForSelf(self string, msg fmail.Message) string {
	self = strings.TrimSpace(self)
	if self == "" {
		return ""
	}
	target := strings.TrimPrefix(strings.TrimSpace(msg.To), "@")
	if strings.EqualFold(strings.TrimSpace(msg.From), self) {
		return target
	}
	if strings.EqualFold(target, self) {
		return strings.TrimSpace(msg.From)
	}
	return ""
}

func (v *topicsView) sendCmd(target string, body string) tea.Cmd {
	root := strings.TrimSpace(v.root)
	self := strings.TrimSpace(v.self)
	target = strings.TrimSpace(target)
	body = strings.TrimSpace(body)
	if root == "" || self == "" || target == "" || body == "" {
		return func() tea.Msg {
			return topicsSentMsg{target: target, err: fmt.Errorf("missing to/from/body")}
		}
	}

	return func() tea.Msg {
		if sender, ok := v.provider.(providerSender); ok {
			msg, err := sender.Send(data.SendRequest{
				From:     self,
				To:       target,
				Body:     body,
				Priority: fmail.PriorityNormal,
			})
			return topicsSentMsg{target: target, msg: msg, err: err}
		}
		store, err := fmail.NewStore(root)
		if err != nil {
			return topicsSentMsg{target: target, err: err}
		}
		msg := &fmail.Message{
			From:     self,
			To:       target,
			Body:     body,
			Priority: fmail.PriorityNormal,
		}
		if _, err := store.SaveMessage(msg); err != nil {
			return topicsSentMsg{target: target, err: err}
		}
		return topicsSentMsg{target: target, msg: *msg}
	}
}
