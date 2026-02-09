package fmailtui

import (
	"fmt"
	"sort"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
	"github.com/tOgg1/forge/internal/fmailtui/threading"
)

const (
	threadPageSize          = 100
	threadMaxVisibleDepth   = 6
	threadMaxVisibleLines   = 50
	threadRefreshInterval   = 2 * time.Second
	threadNewMessageTagline = "New messages (press G)"
)

type threadMode int

const (
	threadModeThreaded threadMode = iota
	threadModeFlat
)

type threadTickMsg struct{}

type threadLoadedMsg struct {
	now     time.Time
	target  string
	agents  []string
	msgs    []fmail.Message
	lastErr error
}

type threadIncomingMsg struct {
	msg fmail.Message
}

type threadView struct {
	root     string
	provider data.MessageProvider

	now    time.Time
	target string // topic name or "@agent"

	mode threadMode

	limit int
	msgs  []fmail.Message

	// UI state
	selected int
	scroll   int // line-based scroll offset in rendered output

	// Thread display state
	collapsed       map[string]bool // message ID -> collapsed replies
	expandedBodies  map[string]bool // message ID -> show full body (>50 lines)
	readMarkers     map[string]string
	newMessagesSeen int

	// Render cache (rebuilt when data or layout changes)
	lastWidth  int
	lastHeight int
	blocks     []threadBlock
	lines      []string

	subCh     <-chan fmail.Message
	subCancel func()

	lastErr error
}

type threadBlock struct {
	id        string
	startLine int
	endLine   int
}

func newThreadView(root string, provider data.MessageProvider) *threadView {
	return &threadView{
		root:           root,
		provider:       provider,
		mode:           threadModeThreaded,
		limit:          threadPageSize,
		collapsed:      make(map[string]bool),
		expandedBodies: make(map[string]bool),
		readMarkers:    make(map[string]string),
	}
}

func (v *threadView) Close() {
	if v == nil {
		return
	}
	if v.subCancel != nil {
		v.subCancel()
		v.subCancel = nil
	}
}

func (v *threadView) Init() tea.Cmd {
	v.startSubscription()
	return tea.Batch(
		v.loadCmd(),
		threadTickCmd(),
		v.waitForMessageCmd(),
	)
}

func (v *threadView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case threadTickMsg:
		return tea.Batch(v.loadCmd(), threadTickCmd())
	case threadLoadedMsg:
		v.now = typed.now
		v.lastErr = typed.lastErr
		if typed.lastErr == nil {
			v.target = typed.target
			v.msgs = typed.msgs
			v.ensureSelectionInRange()
			v.updateReadMarker()
			v.rebuildRender()
		}
		return nil
	case threadIncomingMsg:
		if v.isRelevant(typed.msg) {
			v.msgs = append(v.msgs, typed.msg)
			if len(v.msgs) > 0 && (v.scroll < v.maxScroll()) {
				v.newMessagesSeen++
			}
			v.rebuildRender()
		}
		return tea.Batch(v.waitForMessageCmd(), v.loadCmd())
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
	if len(v.lines) == 0 || v.renderDirty() {
		v.rebuildRender()
	}

	palette := themePalette(theme)
	base := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Background(lipgloss.Color(palette.Base.Background))

	header := v.renderHeader(width, palette)
	bodyHeight := height - lipgloss.Height(header)
	if bodyHeight < 0 {
		bodyHeight = 0
	}
	body := v.renderBody(width, bodyHeight, palette)

	out := lipgloss.JoinVertical(lipgloss.Left, header, body)
	if v.lastErr != nil {
		out = lipgloss.JoinVertical(lipgloss.Left, out, lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Render("data error: "+truncate(v.lastErr.Error(), maxInt(0, width-2))))
	}
	return base.Render(out)
}

func (v *threadView) renderHeader(width int, palette styles.Theme) string {
	title := strings.TrimSpace(v.target)
	if title == "" {
		title = "(no topic)"
	}

	participants := uniqueParticipants(v.msgs)
	meta := fmt.Sprintf("%d messages  %d participants", len(v.msgs), len(participants))
	if v.mode == threadModeFlat {
		meta = meta + "  flat"
	}

	left := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render(title)
	right := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(meta)

	line := left
	if width > 0 {
		gap := width - lipgloss.Width(left) - lipgloss.Width(right)
		if gap < 1 {
			gap = 1
		}
		line = left + strings.Repeat(" ", gap) + right
	}
	return truncateVis(line, width)
}

func (v *threadView) renderBody(width, height int, palette styles.Theme) string {
	if height <= 0 {
		return ""
	}
	if len(v.lines) == 0 {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("no messages")
	}

	maxScroll := v.maxScroll()
	if v.scroll > maxScroll {
		v.scroll = maxScroll
	}
	if v.scroll < 0 {
		v.scroll = 0
	}

	start := v.scroll
	end := minInt(len(v.lines), start+height)
	out := v.lines[start:end]

	// New messages indicator when scrolled up.
	if v.newMessagesSeen > 0 && v.scroll < maxScroll && height > 1 {
		out = append(out[:maxInt(0, len(out)-1)], lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Render(threadNewMessageTagline))
	}
	return strings.Join(out, "\n")
}

func (v *threadView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "f":
		if v.mode == threadModeThreaded {
			v.mode = threadModeFlat
		} else {
			v.mode = threadModeThreaded
		}
		v.rebuildRender()
		return nil
	case "g":
		v.selected = 0
		v.scrollToSelection()
		v.updateReadMarker()
		v.rebuildRender()
		return nil
	case "G", "end":
		v.selected = maxInt(0, len(v.blocks)-1)
		v.scrollToSelection()
		v.newMessagesSeen = 0
		v.updateReadMarker()
		v.rebuildRender()
		return nil
	case "j", "down":
		v.selected = minInt(maxInt(0, len(v.blocks)-1), v.selected+1)
		v.scrollToSelection()
		v.updateReadMarker()
		v.rebuildRender()
		return nil
	case "k", "up":
		v.selected = maxInt(0, v.selected-1)
		v.scrollToSelection()
		v.updateReadMarker()
		v.rebuildRender()
		return nil
	case "ctrl+d":
		return v.pageBy(1)
	case "ctrl+u":
		return v.pageBy(-1)
	case "enter":
		return v.toggleSelected()
	}
	return nil
}

func (v *threadView) pageBy(dir int) tea.Cmd {
	if v.lastHeight <= 0 {
		return nil
	}
	delta := maxInt(1, v.lastHeight/2)
	v.scroll += dir * delta
	if v.scroll < 0 {
		v.scroll = 0
	}
	if v.scroll > v.maxScroll() {
		v.scroll = v.maxScroll()
	}

	// Move selection to the first block visible at/after scroll.
	for i := range v.blocks {
		if v.blocks[i].startLine >= v.scroll {
			v.selected = i
			break
		}
	}
	v.updateReadMarker()
	v.rebuildRender()
	return nil
}

func (v *threadView) toggleSelected() tea.Cmd {
	id := v.selectedMessageID()
	if id == "" {
		return nil
	}

	// If body is truncated, Enter expands first.
	if v.bodyIsTruncated(id) {
		v.expandedBodies[id] = !v.expandedBodies[id]
		v.rebuildRender()
		return nil
	}

	if v.mode == threadModeThreaded {
		v.collapsed[id] = !v.collapsed[id]
		v.rebuildRender()
	}
	return nil
}

func (v *threadView) selectedMessageID() string {
	if v.selected < 0 || v.selected >= len(v.blocks) {
		return ""
	}
	return v.blocks[v.selected].id
}

func (v *threadView) updateReadMarker() {
	if strings.TrimSpace(v.target) == "" {
		return
	}
	id := v.selectedMessageID()
	if id == "" {
		return
	}
	if prev := strings.TrimSpace(v.readMarkers[v.target]); prev == "" || id > prev {
		v.readMarkers[v.target] = id
	}
}

func (v *threadView) ensureSelectionInRange() {
	if v.selected < 0 {
		v.selected = 0
	}
	if v.selected >= len(v.blocks) {
		v.selected = maxInt(0, len(v.blocks)-1)
	}
}

func (v *threadView) scrollToSelection() {
	if v.selected < 0 || v.selected >= len(v.blocks) {
		return
	}
	block := v.blocks[v.selected]
	if v.scroll > block.startLine {
		v.scroll = block.startLine
		return
	}
	if v.lastHeight > 0 && block.endLine > v.scroll+v.lastHeight {
		v.scroll = maxInt(0, block.endLine-v.lastHeight)
	}
}

func (v *threadView) maxScroll() int {
	if v.lastHeight <= 0 {
		return maxInt(0, len(v.lines)-1)
	}
	return maxInt(0, len(v.lines)-v.lastHeight)
}

func (v *threadView) renderDirty() bool {
	return v.lastWidth != 0 && v.lastWidth != v.lastWidth // noop, kept for future
}

func (v *threadView) loadCmd() tea.Cmd {
	if v.provider == nil {
		return func() tea.Msg {
			return threadLoadedMsg{now: time.Now().UTC(), lastErr: fmt.Errorf("missing provider")}
		}
	}
	return func() tea.Msg {
		now := time.Now().UTC()
		target := strings.TrimSpace(v.target)
		if target == "" {
			topics, err := v.provider.Topics()
			if err != nil {
				return threadLoadedMsg{now: now, lastErr: err}
			}
			target = mostRecentTopic(topics)
		}

		var msgs []fmail.Message
		var err error
		if strings.HasPrefix(target, "@") {
			msgs, err = v.provider.DMs(strings.TrimPrefix(target, "@"), data.MessageFilter{Limit: v.limit})
		} else if target != "" {
			msgs, err = v.provider.Messages(target, data.MessageFilter{Limit: v.limit})
		}
		if err != nil {
			return threadLoadedMsg{now: now, target: target, lastErr: err}
		}
		return threadLoadedMsg{now: now, target: target, msgs: msgs}
	}
}

func threadTickCmd() tea.Cmd {
	return tea.Tick(threadRefreshInterval, func(time.Time) tea.Msg {
		return threadTickMsg{}
	})
}

func (v *threadView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.subCancel = cancel
}

func (v *threadView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return threadIncomingMsg{msg: msg}
	}
}

func (v *threadView) isRelevant(msg fmail.Message) bool {
	target := strings.TrimSpace(v.target)
	if target == "" {
		return false
	}
	if strings.TrimSpace(msg.To) == target {
		return true
	}
	// DM conversation: accept either direction.
	if strings.HasPrefix(target, "@") {
		peer := strings.TrimPrefix(target, "@")
		if msg.To == target {
			return true
		}
		if msg.From == peer && strings.HasPrefix(msg.To, "@") {
			return true
		}
	}
	return false
}

func (v *threadView) rebuildRender() {
	width := v.lastWidth
	height := v.lastHeight
	if width <= 0 {
		width = 100
	}
	if height <= 0 {
		height = 30
	}

	palette := themePalette(ThemeDefault)
	mapper := styles.NewAgentColorMapper()

	blocks := make([]threadBlock, 0, len(v.msgs))
	lines := make([]string, 0, len(v.msgs)*4)

	selectedID := v.selectedMessageID()
	readMarker := strings.TrimSpace(v.readMarkers[v.target])

	items := v.buildItems()
	for idx, item := range items {
		if item == nil || item.Message == nil {
			continue
		}
		id := item.Message.ID
		isSelected := id == selectedID
		unread := readMarker != "" && id > readMarker

		blockStart := len(lines)
		rendered := renderThreadMessage(item, width, palette, mapper, isSelected, unread, v.mode == threadModeThreaded)
		lines = append(lines, rendered...)
		blockEnd := len(lines)
		blocks = append(blocks, threadBlock{id: id, startLine: blockStart, endLine: blockEnd})

		// Blank line between thread roots for visual separation in threaded mode.
		if v.mode == threadModeThreaded && idx+1 < len(items) {
			next := items[idx+1]
			if next != nil && next.Parent == nil {
				lines = append(lines, "")
			}
		}
	}

	v.blocks = blocks
	v.lines = lines
	v.ensureSelectionInRange()
	v.scrollToSelection()
}

func (v *threadView) buildItems() []*threading.ThreadNode {
	if len(v.msgs) == 0 {
		return nil
	}

	if v.mode == threadModeFlat {
		msgs := append([]fmail.Message(nil), v.msgs...)
		sort.SliceStable(msgs, func(i, j int) bool { return msgs[i].ID < msgs[j].ID })
		out := make([]*threading.ThreadNode, 0, len(msgs))
		for i := range msgs {
			msg := msgs[i]
			out = append(out, &threading.ThreadNode{Message: &msg, Depth: 0})
		}
		return out
	}

	threads := threading.BuildThreads(v.msgs)
	out := make([]*threading.ThreadNode, 0, len(v.msgs))
	for _, th := range threads {
		if th == nil || len(th.Messages) == 0 {
			continue
		}
		root := findRootNode(th)
		if root == nil {
			continue
		}
		out = append(out, flattenWithCollapse(root, v.collapsed, threadMaxVisibleDepth)...)
	}
	return out
}

func findRootNode(thread *threading.Thread) *threading.ThreadNode {
	if thread == nil || thread.Root == nil {
		return nil
	}
	for _, node := range thread.Messages {
		if node == nil || node.Message == nil {
			continue
		}
		if node.Parent == nil && node.Message.ID == thread.Root.ID {
			return node
		}
	}
	for _, node := range thread.Messages {
		if node != nil && node.Parent == nil {
			return node
		}
	}
	return nil
}

func flattenWithCollapse(root *threading.ThreadNode, collapsed map[string]bool, maxDepth int) []*threading.ThreadNode {
	type frame struct {
		node          *threading.ThreadNode
		ancestorLast  []bool
		isLastSibling bool
	}

	out := make([]*threading.ThreadNode, 0, 64)
	stack := []frame{{node: root, ancestorLast: nil, isLastSibling: true}}

	for len(stack) > 0 {
		f := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		n := f.node
		if n == nil || n.Message == nil {
			continue
		}

		// Clamp depth for display.
		if maxDepth > 0 && n.Depth > maxDepth {
			n.Depth = maxDepth
		}

		out = append(out, n)
		if collapsed != nil && collapsed[n.Message.ID] {
			continue
		}
		if n.Depth >= maxDepth {
			continue
		}

		children := append([]*threading.ThreadNode(nil), n.Children...)
		sort.SliceStable(children, func(i, j int) bool {
			if children[i] == nil || children[i].Message == nil {
				return false
			}
			if children[j] == nil || children[j].Message == nil {
				return true
			}
			if !children[i].Message.Time.Equal(children[j].Message.Time) {
				return children[i].Message.Time.Before(children[j].Message.Time)
			}
			return children[i].Message.ID < children[j].Message.ID
		})

		// Push reverse for DFS in chronological order.
		for i := len(children) - 1; i >= 0; i-- {
			child := children[i]
			if child == nil {
				continue
			}
			// Track ancestor "last" flags for connector rendering.
			ancestorLast := append([]bool(nil), f.ancestorLast...)
			if n.Parent != nil || n.Depth > 0 {
				ancestorLast = append(ancestorLast, f.isLastSibling)
			}
			stack = append(stack, frame{
				node:          child,
				ancestorLast:  ancestorLast,
				isLastSibling: i == len(children)-1,
			})
		}
	}

	return out
}

func renderThreadMessage(node *threading.ThreadNode, width int, palette styles.Theme, mapper *styles.AgentColorMapper, selected bool, unread bool, showTree bool) []string {
	if node == nil || node.Message == nil {
		return nil
	}
	msg := node.Message

	leftBorder := mapper.Foreground(msg.From).Render("▌")
	if selected {
		leftBorder = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Render("▌")
	}
	if unread {
		leftBorder = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Render("●")
	}

	indent := ""
	if showTree {
		indent = strings.Repeat("  ", maxInt(0, minInt(node.Depth, threadMaxVisibleDepth)))
	}

	headerTS := msg.Time.UTC().Format("15:04")
	if selected {
		headerTS = msg.Time.UTC().Format(time.RFC3339)
	}
	header := fmt.Sprintf("%s%s %s (%s)", leftBorder, indent, msg.From, headerTS)
	if selected && strings.TrimSpace(msg.Host) != "" {
		header = header + "  " + strings.TrimSpace(msg.Host)
	}
	if selected {
		header = header + "  " + msg.ID
	}

	innerW := maxInt(10, width-2)
	bodyLines := renderMessageBody(fmt.Sprint(msg.Body), innerW)
	truncated := false
	if len(bodyLines) > threadMaxVisibleLines && !selected {
		bodyLines = bodyLines[:threadMaxVisibleLines]
		truncated = true
	}

	out := make([]string, 0, 2+len(bodyLines)+2)
	out = append(out, truncateVis(header, width))

	for _, line := range bodyLines {
		out = append(out, truncateVis("  "+indent+line, width))
	}
	if truncated {
		out = append(out, truncateVis("  "+indent+"... [show more]", width))
	}

	footerParts := make([]string, 0, 4)
	if strings.TrimSpace(msg.Priority) != "" {
		footerParts = append(footerParts, msg.Priority)
	}
	if len(msg.Tags) > 0 {
		footerParts = append(footerParts, strings.Join(msg.Tags, ","))
	}
	if strings.TrimSpace(msg.ReplyTo) != "" {
		footerParts = append(footerParts, "reply_to:"+shortID(msg.ReplyTo))
	}
	if len(footerParts) > 0 {
		footer := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(strings.Join(footerParts, "  "))
		out = append(out, truncateVis("  "+indent+footer, width))
	}

	return out
}

func renderMessageBody(body string, width int) []string {
	body = strings.ReplaceAll(body, "\r\n", "\n")
	body = strings.TrimRight(body, "\n")
	if body == "" {
		return []string{""}
	}
	lines := strings.Split(body, "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		wrapped := lipgloss.NewStyle().Width(width).Render(line)
		parts := strings.Split(wrapped, "\n")
		out = append(out, parts...)
	}
	return out
}

func shortID(id string) string {
	id = strings.TrimSpace(id)
	if len(id) <= 8 {
		return id
	}
	return id[:8]
}

func uniqueParticipants(messages []fmail.Message) []string {
	set := make(map[string]struct{}, 16)
	for i := range messages {
		if from := strings.TrimSpace(messages[i].From); from != "" {
			set[from] = struct{}{}
		}
		if to := strings.TrimSpace(messages[i].To); strings.HasPrefix(to, "@") {
			set[strings.TrimPrefix(to, "@")] = struct{}{}
		}
	}
	out := make([]string, 0, len(set))
	for name := range set {
		out = append(out, name)
	}
	sort.Strings(out)
	return out
}

func mostRecentTopic(topics []data.TopicInfo) string {
	if len(topics) == 0 {
		return ""
	}
	best := topics[0]
	for i := 1; i < len(topics); i++ {
		if topics[i].LastActivity.After(best.LastActivity) {
			best = topics[i]
		}
	}
	return strings.TrimSpace(best.Name)
}

func (v *threadView) bodyIsTruncated(id string) bool {
	if id == "" {
		return false
	}
	// Only allow expand in threaded mode selection; cheap approximation:
	// count raw lines and compare with threshold.
	for i := range v.msgs {
		if v.msgs[i].ID == id {
			raw := fmt.Sprint(v.msgs[i].Body)
			return len(strings.Split(strings.ReplaceAll(raw, "\r\n", "\n"), "\n")) > threadMaxVisibleLines
		}
	}
	return false
}

