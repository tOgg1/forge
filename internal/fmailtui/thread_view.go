package fmailtui

import (
	"fmt"
	"regexp"
	"sort"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/reflow/wordwrap"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
	"github.com/tOgg1/forge/internal/fmailtui/threading"
)

const (
	threadPageSize        = 100
	threadMaxDepth        = 6
	threadMaxBodyLines    = 50
	threadRefreshInterval = 2 * time.Second
)

var inlineCodePattern = regexp.MustCompile("`[^`]+`")

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
	err    error
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

	now     time.Time
	lastErr error

	topics []data.TopicInfo
	topic  string

	mode threadMode

	allMsgs      []fmail.Message
	windowStart  int
	rows         []threadRow
	rowIndexByID map[string]int

	collapsed      map[string]bool
	expandedBodies map[string]bool
	readMarkers    map[string]string

	selected int
	top      int

	lastWidth    int
	lastHeight   int
	viewportRows int
	pendingNew   int
	newestID     string

	initialized bool
}

func newThreadView(root string, provider data.MessageProvider) *threadView {
	return &threadView{
		root:           root,
		provider:       provider,
		mode:           threadModeThreaded,
		collapsed:      make(map[string]bool),
		expandedBodies: make(map[string]bool),
		readMarkers:    make(map[string]string),
		rowIndexByID:   make(map[string]int),
	}
}

func (v *threadView) Init() tea.Cmd {
	return tea.Batch(v.loadCmd(), threadTickCmd())
}

func (v *threadView) SetTarget(target string) tea.Cmd {
	next := strings.TrimSpace(target)
	if next == "" {
		return nil
	}
	if next == v.topic {
		return v.loadCmd()
	}
	v.topic = next
	v.windowStart = 0
	v.pendingNew = 0
	v.selected = 0
	v.top = 0
	return v.loadCmd()
}

func (v *threadView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case threadTickMsg:
		return tea.Batch(v.loadCmd(), threadTickCmd())
	case threadLoadedMsg:
		v.applyLoaded(typed)
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

	bodyHeight := height - lipgloss.Height(header) - lipgloss.Height(meta)
	if bodyHeight < 1 {
		bodyHeight = 1
	}
	v.viewportRows = maxInt(1, bodyHeight/4)

	body := v.renderRows(width, bodyHeight, palette)
	content := lipgloss.JoinVertical(lipgloss.Left, header, meta, body)
	if v.lastErr != nil {
		errLine := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Render("data error: " + truncate(v.lastErr.Error(), maxInt(0, width-2)))
		content = lipgloss.JoinVertical(lipgloss.Left, content, errLine)
	}

	base := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Background(lipgloss.Color(palette.Base.Background))
	return base.Render(content)
}

func (v *threadView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "j", "down":
		v.moveSelection(1)
		return nil
	case "k", "up":
		if v.tryLoadOlderOnUp() {
			return nil
		}
		v.moveSelection(-1)
		return nil
	case "ctrl+d":
		v.moveSelection(maxInt(1, v.pageStep()))
		return nil
	case "ctrl+u":
		v.moveSelection(-maxInt(1, v.pageStep()))
		return nil
	case "g":
		v.selected = 0
		v.top = 0
		v.advanceReadMarker()
		return nil
	case "G", "end":
		v.jumpBottom()
		return nil
	case "f":
		if v.mode == threadModeThreaded {
			v.mode = threadModeFlat
		} else {
			v.mode = threadModeThreaded
		}
		anchor := v.selectedID()
		v.rebuildRows(anchor, false)
		v.ensureVisible()
		return nil
	case "[":
		return v.switchTopic(-1)
	case "]":
		return v.switchTopic(1)
	case "enter":
		return v.handleEnter()
	}
	return nil
}

func (v *threadView) switchTopic(delta int) tea.Cmd {
	if len(v.topics) == 0 {
		return nil
	}
	idx := v.topicIndex(v.topic)
	if idx < 0 {
		idx = 0
	}
	idx = (idx + delta + len(v.topics)) % len(v.topics)
	v.topic = v.topics[idx].Name
	v.windowStart = 0
	v.pendingNew = 0
	v.selected = 0
	v.top = 0
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

func (v *threadView) tryLoadOlderOnUp() bool {
	if v.selected > 0 || v.windowStart <= 0 || len(v.allMsgs) == 0 {
		return false
	}
	anchorID := v.selectedID()
	prevStart := v.windowStart
	v.windowStart = maxInt(0, v.windowStart-threadPageSize)
	if v.windowStart == prevStart {
		return false
	}
	v.rebuildRows(anchorID, false)
	idx := v.indexForID(anchorID)
	if idx > 0 {
		v.selected = idx - 1
	}
	v.ensureVisible()
	v.advanceReadMarker()
	return true
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

func (v *threadView) applyLoaded(msg threadLoadedMsg) {
	v.now = msg.now
	v.lastErr = msg.err
	if msg.err != nil {
		return
	}

	prevTopic := v.topic
	prevAnchor := v.selectedID()
	wasAtBottom := v.isAtBottom()
	prevNewest := v.newestID

	v.topics = sortTopicsByActivity(msg.topics)
	if strings.TrimSpace(msg.topic) != "" {
		v.topic = msg.topic
	} else if v.topic == "" && len(v.topics) > 0 {
		v.topic = v.topics[0].Name
	}

	v.allMsgs = append([]fmail.Message(nil), msg.msgs...)
	if !v.initialized || v.topic != prevTopic {
		v.windowStart = initialWindowStart(len(v.allMsgs))
		v.initialized = true
		prevAnchor = ""
	}
	v.windowStart = clampInt(v.windowStart, 0, maxInt(0, len(v.allMsgs)-1))

	if len(v.allMsgs) > 0 {
		v.newestID = v.allMsgs[len(v.allMsgs)-1].ID
	} else {
		v.newestID = ""
	}

	if prevNewest != "" && v.newestID != "" && v.newestID > prevNewest && !wasAtBottom {
		v.pendingNew += countNewerMessages(v.allMsgs, prevNewest)
	}

	preferBottom := wasAtBottom && prevTopic == v.topic
	v.rebuildRows(prevAnchor, preferBottom)
	if preferBottom {
		v.pendingNew = 0
	}
	v.ensureVisible()
	v.advanceReadMarker()
}

func (v *threadView) renderHeader(width int, palette styles.Theme) string {
	topic := strings.TrimSpace(v.topic)
	if topic == "" {
		topic = "(no topic)"
	}
	participants := v.participantCount(topic)
	left := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render(topic)
	right := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(fmt.Sprintf("%d messages  %d participants", len(v.allMsgs), participants))

	gap := maxInt(1, width-lipgloss.Width(left)-lipgloss.Width(right))
	return truncateVis(left+strings.Repeat(" ", gap)+right, width)
}

func (v *threadView) renderMeta(width int, palette styles.Theme) string {
	mode := "threaded"
	if v.mode == threadModeFlat {
		mode = "flat"
	}
	marker := strings.TrimSpace(v.readMarkers[v.topic])
	meta := fmt.Sprintf("mode:%s  j/k move  ctrl+d/u page  g/G top/bot  Enter expand/collapse  f toggle  [ ] topic", mode)
	if marker != "" {
		meta = meta + "  read:" + shortID(marker)
	}
	return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(truncateVis(meta, width))
}

func (v *threadView) renderRows(width, height int, palette styles.Theme) string {
	if height <= 0 {
		return ""
	}
	if len(v.rows) == 0 {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("No messages")
	}

	v.ensureVisible()
	start := clampInt(v.top, 0, maxInt(0, len(v.rows)-1))
	remaining := height
	out := make([]string, 0, height)
	mapper := styles.NewAgentColorMapper()
	msgStyles := styles.NewMessageStyles(palette, mapper)

	for i := start; i < len(v.rows) && remaining > 0; i++ {
		row := v.rows[i]
		if row.groupGap && len(out) > 0 && remaining > 0 {
			out = append(out, "")
			remaining--
			if remaining <= 0 {
				break
			}
		}

		selected := i == v.selected
		unread := v.isUnread(row.msg.ID)
		lines := v.renderRowCard(row, width, selected, unread, palette, mapper, msgStyles)
		if len(lines) > remaining {
			lines = lines[:remaining]
		}
		out = append(out, lines...)
		remaining -= len(lines)
	}

	if v.pendingNew > 0 && !v.isAtBottom() && len(out) > 0 {
		indicator := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Render(fmt.Sprintf("New messages (%d) - press G", v.pendingNew))
		out[len(out)-1] = truncateVis(indicator, width)
	}

	return strings.Join(out, "\n")
}

func (v *threadView) renderRowCard(row threadRow, width int, selected bool, unread bool, palette styles.Theme, mapper *styles.AgentColorMapper, msgStyles styles.MessageStyles) []string {
	agentColor := mapper.ColorCode(row.msg.From)
	borderColor := agentColor
	if selected {
		borderColor = palette.Chrome.SelectedItem
	}

	timeLabel := relativeTime(row.msg.Time, v.now)
	if selected {
		timeLabel = row.msg.Time.UTC().Format(time.RFC3339)
	}

	agentName := mapper.Foreground(row.msg.From).Render(strings.TrimSpace(row.msg.From))
	indent := row.connector
	if row.overflow {
		indent = indent + "... "
	}
	unreadDot := ""
	if unread {
		unreadDot = msgStyles.RenderUnreadIndicator(true) + " "
	}

	header := fmt.Sprintf("%s%s (%s)", indent+unreadDot+agentName, "", timeLabel)
	content := []string{header}

	bodyWidth := maxInt(10, width-8-lipgloss.Width(indent))
	bodyLines := renderBodyLines(messageBodyString(row.msg.Body), bodyWidth, palette)
	if row.truncated {
		limit := minInt(threadMaxBodyLines, len(bodyLines))
		bodyLines = bodyLines[:limit]
	}

	bodyPrefix := strings.Repeat(" ", lipgloss.Width(indent))
	for _, line := range bodyLines {
		content = append(content, bodyPrefix+line)
	}
	if row.truncated {
		content = append(content, bodyPrefix+fmt.Sprintf("... [show more] (%d lines)", row.hiddenLines))
	}

	footerParts := make([]string, 0, 4)
	if badge := msgStyles.RenderPriorityBadge(row.msg.Priority); badge != "" {
		footerParts = append(footerParts, badge)
	}
	if tags := msgStyles.RenderTagPills(row.msg.Tags); tags != "" {
		footerParts = append(footerParts, tags)
	}
	if row.replyTo != "" {
		reply := "↩ " + shortID(row.replyTo)
		if row.crossTarget != "" {
			reply = reply + " from " + row.crossTarget
		}
		footerParts = append(footerParts, reply)
	}
	if len(footerParts) > 0 {
		content = append(content, bodyPrefix+strings.Join(footerParts, "  "))
	}
	if selected {
		details := fmt.Sprintf("id:%s", row.msg.ID)
		if host := strings.TrimSpace(row.msg.Host); host != "" {
			details += "  host:" + host
		}
		content = append(content, bodyPrefix+details)
	}

	card := strings.Join(content, "\n")
	cardStyle := lipgloss.NewStyle().BorderLeft(true).BorderStyle(lipgloss.NormalBorder()).BorderForeground(lipgloss.Color(borderColor)).PaddingLeft(1)
	if selected {
		cardStyle = cardStyle.Bold(true)
	}
	return strings.Split(cardStyle.Width(maxInt(0, width)).Render(card), "\n")
}

func (v *threadView) loadCmd() tea.Cmd {
	if v.provider == nil {
		return func() tea.Msg {
			return threadLoadedMsg{now: time.Now().UTC(), err: fmt.Errorf("missing provider")}
		}
	}
	currentTopic := strings.TrimSpace(v.topic)
	return func() tea.Msg {
		now := time.Now().UTC()
		topics, err := v.provider.Topics()
		if err != nil {
			return threadLoadedMsg{now: now, err: err}
		}
		sortedTopics := sortTopicsByActivity(topics)

		topic := currentTopic
		if topic == "" && len(sortedTopics) > 0 {
			topic = sortedTopics[0].Name
		}
		if topic != "" && !topicExists(sortedTopics, topic) && len(sortedTopics) > 0 {
			topic = sortedTopics[0].Name
		}

		msgs := []fmail.Message{}
		if topic != "" {
			msgs, err = v.provider.Messages(topic, data.MessageFilter{})
			if err != nil {
				return threadLoadedMsg{now: now, topics: sortedTopics, topic: topic, err: err}
			}
		}

		return threadLoadedMsg{now: now, topics: sortedTopics, topic: topic, msgs: msgs}
	}
}

func threadTickCmd() tea.Cmd {
	return tea.Tick(threadRefreshInterval, func(time.Time) tea.Msg {
		return threadTickMsg{}
	})
}

func (v *threadView) rebuildRows(anchorID string, preferBottom bool) {
	msgs := v.windowMessages()
	rows := make([]threadRow, 0, len(msgs))

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
				overflow := node.Depth > threadMaxDepth
				connector := v.nodeConnector(node)
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

func (v *threadView) windowMessages() []fmail.Message {
	if len(v.allMsgs) == 0 {
		return nil
	}
	start := clampInt(v.windowStart, 0, maxInt(0, len(v.allMsgs)-1))
	if start >= len(v.allMsgs) {
		return nil
	}
	out := make([]fmail.Message, len(v.allMsgs[start:]))
	copy(out, v.allMsgs[start:])
	return out
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

func (v *threadView) nodeConnector(node *threading.ThreadNode) string {
	if node == nil || node.Message == nil || node.Parent == nil {
		return ""
	}
	ancestors := ancestorChain(node)
	parts := make([]string, 0, len(ancestors))
	for _, anc := range ancestors {
		if anc.Parent == nil {
			continue
		}
		if v.isLastChild(anc) {
			parts = append(parts, "   ")
		} else {
			parts = append(parts, "│  ")
		}
	}
	if v.isLastChild(node) {
		parts = append(parts, "└─ ")
	} else {
		parts = append(parts, "├─ ")
	}
	return strings.Join(parts, "")
}

func ancestorChain(node *threading.ThreadNode) []*threading.ThreadNode {
	out := make([]*threading.ThreadNode, 0, 8)
	for cur := node.Parent; cur != nil; cur = cur.Parent {
		out = append(out, cur)
	}
	for i, j := 0, len(out)-1; i < j; i, j = i+1, j-1 {
		out[i], out[j] = out[j], out[i]
	}
	return out
}

func (v *threadView) isLastChild(node *threading.ThreadNode) bool {
	if node == nil || node.Parent == nil {
		return true
	}
	siblings := sortedChildren(node.Parent.Children)
	if len(siblings) == 0 {
		return true
	}
	last := siblings[len(siblings)-1]
	if last == nil || last.Message == nil || node.Message == nil {
		return true
	}
	return last.Message.ID == node.Message.ID
}

func sortedChildren(children []*threading.ThreadNode) []*threading.ThreadNode {
	cloned := append([]*threading.ThreadNode(nil), children...)
	sort.SliceStable(cloned, func(i, j int) bool {
		if cloned[i] == nil || cloned[i].Message == nil {
			return false
		}
		if cloned[j] == nil || cloned[j].Message == nil {
			return true
		}
		left := *cloned[i].Message
		right := *cloned[j].Message
		if !left.Time.Equal(right.Time) {
			return left.Time.Before(right.Time)
		}
		return left.ID < right.ID
	})
	return cloned
}

func (v *threadView) bodyTruncation(msg fmail.Message) (bool, int) {
	id := strings.TrimSpace(msg.ID)
	if id != "" && v.expandedBodies[id] {
		return false, 0
	}
	raw := strings.ReplaceAll(messageBodyString(msg.Body), "\r\n", "\n")
	count := len(strings.Split(raw, "\n"))
	if count > threadMaxBodyLines {
		return true, count - threadMaxBodyLines
	}
	return false, 0
}

func (v *threadView) selectedID() string {
	if v.selected < 0 || v.selected >= len(v.rows) {
		return ""
	}
	return strings.TrimSpace(v.rows[v.selected].msg.ID)
}

func (v *threadView) selectedRow() *threadRow {
	if v.selected < 0 || v.selected >= len(v.rows) {
		return nil
	}
	return &v.rows[v.selected]
}

func (v *threadView) indexForID(id string) int {
	if strings.TrimSpace(id) == "" {
		return -1
	}
	if idx, ok := v.rowIndexByID[id]; ok {
		return idx
	}
	return -1
}

func (v *threadView) isAtBottom() bool {
	if len(v.rows) == 0 {
		return true
	}
	return v.selected >= len(v.rows)-1
}

func (v *threadView) isUnread(id string) bool {
	marker := strings.TrimSpace(v.readMarkers[v.topic])
	if marker == "" {
		return false
	}
	return id > marker
}

func (v *threadView) advanceReadMarker() {
	if strings.TrimSpace(v.topic) == "" {
		return
	}
	id := v.selectedID()
	if id == "" {
		return
	}
	if prev := strings.TrimSpace(v.readMarkers[v.topic]); prev == "" || id > prev {
		v.readMarkers[v.topic] = id
	}
}

func (v *threadView) topicIndex(topic string) int {
	for i := range v.topics {
		if v.topics[i].Name == topic {
			return i
		}
	}
	return -1
}

func (v *threadView) participantCount(topic string) int {
	for i := range v.topics {
		if v.topics[i].Name == topic {
			return len(v.topics[i].Participants)
		}
	}
	return 0
}

func sortTopicsByActivity(topics []data.TopicInfo) []data.TopicInfo {
	out := append([]data.TopicInfo(nil), topics...)
	sort.SliceStable(out, func(i, j int) bool {
		if !out[i].LastActivity.Equal(out[j].LastActivity) {
			return out[i].LastActivity.After(out[j].LastActivity)
		}
		return out[i].Name < out[j].Name
	})
	return out
}

func topicExists(topics []data.TopicInfo, topic string) bool {
	for i := range topics {
		if topics[i].Name == topic {
			return true
		}
	}
	return false
}

func sortMessages(msgs []fmail.Message) {
	sort.SliceStable(msgs, func(i, j int) bool {
		if msgs[i].ID != msgs[j].ID {
			return msgs[i].ID < msgs[j].ID
		}
		if !msgs[i].Time.Equal(msgs[j].Time) {
			return msgs[i].Time.Before(msgs[j].Time)
		}
		return msgs[i].From < msgs[j].From
	})
}

func initialWindowStart(total int) int {
	if total > 1000 {
		return maxInt(0, total-threadPageSize)
	}
	return 0
}

func countNewerMessages(messages []fmail.Message, marker string) int {
	if marker == "" {
		return 0
	}
	count := 0
	for i := range messages {
		if messages[i].ID > marker {
			count++
		}
	}
	return count
}

func relativeTime(ts time.Time, now time.Time) string {
	if ts.IsZero() {
		return "unknown"
	}
	if now.IsZero() {
		now = time.Now().UTC()
	}
	delta := now.Sub(ts)
	if delta < 0 {
		delta = -delta
	}
	switch {
	case delta < time.Minute:
		return fmt.Sprintf("%ds ago", int(delta.Seconds()))
	case delta < time.Hour:
		return fmt.Sprintf("%dm ago", int(delta.Minutes()))
	case delta < 24*time.Hour:
		return fmt.Sprintf("%dh ago", int(delta.Hours()))
	default:
		return fmt.Sprintf("%dd ago", int(delta.Hours()/24))
	}
}

func renderBodyLines(body string, width int, palette styles.Theme) []string {
	if width <= 0 {
		width = 1
	}
	body = strings.ReplaceAll(body, "\r\n", "\n")
	if strings.TrimSpace(body) == "" {
		return []string{""}
	}

	codeStyle := lipgloss.NewStyle().Background(lipgloss.Color(palette.Borders.Divider)).Foreground(lipgloss.Color(palette.Base.Foreground))
	inlineCodeStyle := lipgloss.NewStyle().Background(lipgloss.Color(palette.Base.Border)).Foreground(lipgloss.Color(palette.Base.Foreground)).Bold(true)

	inCode := false
	lines := strings.Split(body, "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "```") {
			inCode = !inCode
			continue
		}

		if inCode {
			wrapped := wrapLines(line, width)
			for _, codeLine := range wrapped {
				out = append(out, codeStyle.Render(codeLine))
			}
			continue
		}

		wrapped := wrapLines(line, width)
		for _, textLine := range wrapped {
			out = append(out, highlightInlineCode(textLine, inlineCodeStyle))
		}
	}
	if len(out) == 0 {
		return []string{""}
	}
	return out
}

func wrapLines(line string, width int) []string {
	if width <= 0 {
		return []string{line}
	}
	wrapped := wordwrap.String(line, width)
	parts := strings.Split(wrapped, "\n")
	if len(parts) == 0 {
		return []string{""}
	}
	return parts
}

func highlightInlineCode(line string, style lipgloss.Style) string {
	matches := inlineCodePattern.FindAllStringIndex(line, -1)
	if len(matches) == 0 {
		return line
	}
	var b strings.Builder
	cursor := 0
	for _, m := range matches {
		if m[0] > cursor {
			b.WriteString(line[cursor:m[0]])
		}
		b.WriteString(style.Render(line[m[0]:m[1]]))
		cursor = m[1]
	}
	if cursor < len(line) {
		b.WriteString(line[cursor:])
	}
	return b.String()
}

func messageBodyString(body any) string {
	if body == nil {
		return ""
	}
	if s, ok := body.(string); ok {
		return s
	}
	return fmt.Sprint(body)
}

func shortID(id string) string {
	id = strings.TrimSpace(id)
	if len(id) <= 8 {
		return id
	}
	return id[:8]
}
