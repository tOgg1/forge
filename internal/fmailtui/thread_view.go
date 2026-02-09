package fmailtui

import (
	"fmt"
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
	threadPageSize          = 100
	threadMaxVisibleDepth   = 6
	threadMaxVisibleLines   = 50
	threadRefreshInterval   = 2 * time.Second
	threadNewMessageTagline = "new messages"
)

type threadMode int

const (
	threadModeThreaded threadMode = iota
	threadModeFlat
)

type threadTickMsg struct{}

type threadLoadedMsg struct {
	now          time.Time
	target       string
	msgs         []fmail.Message
	total        int
	participants int
	lastErr      error
}

type threadIncomingMsg struct {
	msg fmail.Message
}

type threadBlock struct {
	id        string
	startLine int
	endLine   int
}

type threadItem struct {
	node         *threading.ThreadNode
	prefix       string
	depthClamped bool
	threadBreak  bool
}

type threadView struct {
	root     string
	provider data.MessageProvider

	now    time.Time
	target string // topic name or "@agent"

	mode threadMode

	limit int
	total int
	msgs  []fmail.Message

	participants int

	selected int
	scroll   int

	collapsed      map[string]bool
	expandedBodies map[string]bool
	readMarkers    map[string]string
	newMessages    int

	lastWidth  int
	lastHeight int
	blocks     []threadBlock
	lines      []string

	subCh     <-chan fmail.Message
	subCancel func()

	lastErr error
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
	return tea.Batch(v.loadCmd(), threadTickCmd(), v.waitForMessageCmd())
}

func (v *threadView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case threadTickMsg:
		return tea.Batch(v.loadCmd(), threadTickCmd())
	case threadLoadedMsg:
		v.applyLoaded(typed)
		return nil
	case threadIncomingMsg:
		if v.isRelevant(typed.msg) && v.scroll < v.maxScroll() {
			v.newMessages++
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

	if v.lastWidth != width || v.lastHeight != height {
		v.lastWidth = width
		v.lastHeight = height
		v.rebuildRender()
	}
	if len(v.lines) == 0 {
		v.rebuildRender()
	}

	palette := themePalette(theme)
	base := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Background(lipgloss.Color(palette.Base.Background))

	header := v.renderHeader(width, palette)
	bodyHeight := height - lipgloss.Height(header)
	if bodyHeight < 0 {
		bodyHeight = 0
	}
	body := v.renderBody(bodyHeight, palette)
	out := lipgloss.JoinVertical(lipgloss.Left, header, body)
	if v.lastErr != nil {
		errLine := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Render("data error: " + truncate(v.lastErr.Error(), maxInt(0, width-2)))
		out = lipgloss.JoinVertical(lipgloss.Left, out, errLine)
	}
	return base.Render(out)
}

func (v *threadView) applyLoaded(loaded threadLoadedMsg) {
	v.now = loaded.now
	v.lastErr = loaded.lastErr
	if loaded.lastErr != nil {
		return
	}

	oldTarget := v.target
	selectedID := v.selectedMessageID()

	v.target = loaded.target
	v.msgs = loaded.msgs
	v.total = loaded.total
	v.participants = loaded.participants

	if oldTarget != loaded.target {
		v.collapsed = make(map[string]bool)
		v.expandedBodies = make(map[string]bool)
		v.newMessages = 0
	}

	v.rebuildRender()
	if oldTarget != loaded.target {
		v.selected = maxInt(0, len(v.blocks)-1)
		v.scrollToSelection()
	} else {
		v.restoreSelection(selectedID)
	}
	v.updateReadMarker()
}

func (v *threadView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "f":
		selectedID := v.selectedMessageID()
		if v.mode == threadModeThreaded {
			v.mode = threadModeFlat
		} else {
			v.mode = threadModeThreaded
		}
		v.rebuildRender()
		v.restoreSelection(selectedID)
		v.updateReadMarker()
		return nil
	case "g", "home":
		v.selected = 0
		v.scrollToSelection()
		v.updateReadMarker()
		return v.maybeLoadOlder()
	case "G", "end":
		v.selected = maxInt(0, len(v.blocks)-1)
		v.scrollToSelection()
		v.newMessages = 0
		v.updateReadMarker()
		return nil
	case "j", "down":
		v.selected = minInt(maxInt(0, len(v.blocks)-1), v.selected+1)
		v.scrollToSelection()
		v.updateReadMarker()
		return nil
	case "k", "up":
		v.selected = maxInt(0, v.selected-1)
		v.scrollToSelection()
		v.updateReadMarker()
		return v.maybeLoadOlder()
	case "ctrl+d":
		v.pageBy(1)
		return nil
	case "ctrl+u":
		v.pageBy(-1)
		return v.maybeLoadOlder()
	case "enter":
		return v.toggleSelected()
	}
	return nil
}

func (v *threadView) pageBy(dir int) {
	if v.lastHeight <= 0 {
		return
	}
	delta := maxInt(1, v.lastHeight/2)
	v.scroll += dir * delta
	if v.scroll < 0 {
		v.scroll = 0
	}
	if v.scroll > v.maxScroll() {
		v.scroll = v.maxScroll()
	}

	for i := range v.blocks {
		if v.blocks[i].startLine >= v.scroll {
			v.selected = i
			break
		}
	}
	v.updateReadMarker()
}

func (v *threadView) maybeLoadOlder() tea.Cmd {
	if v.total <= len(v.msgs) || len(v.blocks) == 0 || v.selected > 0 {
		return nil
	}
	v.limit += threadPageSize
	if v.limit > v.total {
		v.limit = v.total
	}
	return v.loadCmd()
}

func (v *threadView) toggleSelected() tea.Cmd {
	id := v.selectedMessageID()
	if id == "" {
		return nil
	}

	if v.bodyIsLong(id) {
		v.expandedBodies[id] = !v.expandedBodies[id]
		v.rebuildRender()
		return nil
	}

	if v.mode == threadModeThreaded {
		v.collapsed[id] = !v.collapsed[id]
		selectedID := v.selectedMessageID()
		v.rebuildRender()
		v.restoreSelection(selectedID)
	}
	return nil
}

func (v *threadView) selectedMessageID() string {
	if v.selected < 0 || v.selected >= len(v.blocks) {
		return ""
	}
	return v.blocks[v.selected].id
}

func (v *threadView) restoreSelection(id string) {
	if id == "" {
		v.ensureSelectionInRange()
		return
	}
	for i := range v.blocks {
		if v.blocks[i].id == id {
			v.selected = i
			v.scrollToSelection()
			return
		}
	}
	v.ensureSelectionInRange()
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

func (v *threadView) renderHeader(width int, palette styles.Theme) string {
	title := strings.TrimSpace(v.target)
	if title == "" {
		title = "(no topic)"
	}
	metaCount := fmt.Sprintf("%d messages", v.total)
	if v.total > len(v.msgs) {
		metaCount = fmt.Sprintf("%d/%d messages", len(v.msgs), v.total)
	}
	meta := fmt.Sprintf("%s  %d participants", metaCount, v.participants)
	if v.mode == threadModeFlat {
		meta += "  flat"
	}
	left := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render(title)
	right := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(meta)
	gap := width - lipgloss.Width(left) - lipgloss.Width(right)
	if gap < 1 {
		gap = 1
	}
	return truncateVis(left+strings.Repeat(" ", gap)+right, width)
}

func (v *threadView) renderBody(height int, palette styles.Theme) string {
	if height <= 0 {
		return ""
	}
	if len(v.lines) == 0 {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("no messages")
	}
	if v.scroll > v.maxScroll() {
		v.scroll = v.maxScroll()
	}
	if v.scroll < 0 {
		v.scroll = 0
	}

	start := v.scroll
	end := minInt(len(v.lines), start+height)
	out := append([]string(nil), v.lines[start:end]...)

	if v.newMessages > 0 && v.scroll < v.maxScroll() && len(out) > 0 {
		tag := fmt.Sprintf("%s: %d (G jump)", threadNewMessageTagline, v.newMessages)
		out[len(out)-1] = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Render(truncateVis(tag, maxInt(0, v.lastWidth-1)))
	}
	return strings.Join(out, "\n")
}

func (v *threadView) loadCmd() tea.Cmd {
	if v.provider == nil {
		return func() tea.Msg {
			return threadLoadedMsg{now: time.Now().UTC(), lastErr: fmt.Errorf("missing provider")}
		}
	}

	currentTarget := strings.TrimSpace(v.target)
	limit := v.limit
	return func() tea.Msg {
		now := time.Now().UTC()
		target := currentTarget
		if target == "" {
			topics, err := v.provider.Topics()
			if err != nil {
				return threadLoadedMsg{now: now, lastErr: err}
			}
			target = mostRecentTopic(topics)
		}

		if target == "" {
			return threadLoadedMsg{now: now, target: "", msgs: nil, total: 0, participants: 0}
		}

		var all []fmail.Message
		var err error
		if strings.HasPrefix(target, "@") {
			all, err = v.provider.DMs(strings.TrimPrefix(target, "@"), data.MessageFilter{})
		} else {
			all, err = v.provider.Messages(target, data.MessageFilter{})
		}
		if err != nil {
			return threadLoadedMsg{now: now, target: target, lastErr: err}
		}

		total := len(all)
		msgs := all
		if total > 1000 {
			if limit <= 0 {
				limit = threadPageSize
			}
			if limit > total {
				limit = total
			}
			msgs = all[total-limit:]
		}

		return threadLoadedMsg{
			now:          now,
			target:       target,
			msgs:         msgs,
			total:        total,
			participants: len(uniqueParticipants(all)),
		}
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
	if strings.EqualFold(strings.TrimSpace(msg.To), target) {
		return true
	}
	if strings.HasPrefix(target, "@") {
		peer := strings.TrimPrefix(target, "@")
		return strings.EqualFold(msg.From, peer) && strings.HasPrefix(msg.To, "@")
	}
	return false
}

func (v *threadView) rebuildRender() {
	width := v.lastWidth
	if width <= 0 {
		width = 100
	}

	palette := themePalette(ThemeDefault)
	mapper := styles.NewAgentColorMapper()

	selectedID := v.selectedMessageID()
	readMarker := strings.TrimSpace(v.readMarkers[v.target])

	items := v.buildItems()
	blocks := make([]threadBlock, 0, len(items))
	lines := make([]string, 0, len(items)*4)

	for _, item := range items {
		if item.threadBreak {
			lines = append(lines, "")
			continue
		}
		if item.node == nil || item.node.Message == nil {
			continue
		}
		id := item.node.Message.ID
		selected := id == selectedID
		unread := readMarker == "" || id > readMarker
		expanded := v.expandedBodies[id]

		start := len(lines)
		rendered := renderThreadMessage(item, width, palette, mapper, selected, unread, expanded)
		lines = append(lines, rendered...)
		end := len(lines)
		blocks = append(blocks, threadBlock{id: id, startLine: start, endLine: end})
	}

	v.blocks = blocks
	v.lines = lines
	v.ensureSelectionInRange()
	v.scrollToSelection()
}

func (v *threadView) buildItems() []threadItem {
	if len(v.msgs) == 0 {
		return nil
	}
	if v.mode == threadModeFlat {
		msgs := append([]fmail.Message(nil), v.msgs...)
		sort.SliceStable(msgs, func(i, j int) bool { return msgs[i].ID < msgs[j].ID })
		out := make([]threadItem, 0, len(msgs))
		for i := range msgs {
			msg := msgs[i]
			out = append(out, threadItem{node: &threading.ThreadNode{Message: &msg}})
		}
		return out
	}

	threads := threading.BuildThreads(v.msgs)
	out := make([]threadItem, 0, len(v.msgs)+len(threads))
	for i, th := range threads {
		nodes := threading.FlattenThread(th)
		for _, node := range nodes {
			if node == nil || node.Message == nil {
				continue
			}
			if v.isCollapsedByAncestor(node) {
				continue
			}
			prefix, clamped := prefixForNode(node, threadMaxVisibleDepth)
			out = append(out, threadItem{node: node, prefix: prefix, depthClamped: clamped})
		}
		if i < len(threads)-1 {
			out = append(out, threadItem{threadBreak: true})
		}
	}
	return out
}

func (v *threadView) isCollapsedByAncestor(node *threading.ThreadNode) bool {
	for cur := node.Parent; cur != nil; cur = cur.Parent {
		if cur.Message != nil && v.collapsed[cur.Message.ID] {
			return true
		}
	}
	return false
}

func renderThreadMessage(item threadItem, width int, palette styles.Theme, mapper *styles.AgentColorMapper, selected bool, unread bool, expanded bool) []string {
	node := item.node
	if node == nil || node.Message == nil {
		return nil
	}
	msg := node.Message

	left := mapper.Foreground(msg.From).Render("▌")
	if selected {
		left = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Render("▌")
	}
	if unread {
		left = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Render("●")
	}

	prefix := item.prefix
	if item.depthClamped {
		prefix = "... " + prefix
	}
	ts := msg.Time.UTC().Format("15:04")
	if selected {
		ts = msg.Time.UTC().Format(time.RFC3339)
	}
	header := fmt.Sprintf("%s%s%s (%s)", left, prefix, mapper.Foreground(msg.From).Render(msg.From), ts)
	if selected {
		host := strings.TrimSpace(msg.Host)
		if host == "" {
			host = "-"
		}
		header += "  id:" + msg.ID + " host:" + host
	}

	innerW := maxInt(16, width-2-lipgloss.Width(prefix))
	bodyLines := renderMessageBody(fmt.Sprint(msg.Body), innerW, palette)
	truncated := false
	if len(bodyLines) > threadMaxVisibleLines && !expanded {
		bodyLines = bodyLines[:threadMaxVisibleLines]
		truncated = true
	}

	out := make([]string, 0, 2+len(bodyLines)+2)
	out = append(out, truncateVis(header, width))
	for _, line := range bodyLines {
		out = append(out, truncateVis("  "+prefix+line, width))
	}

	footerParts := make([]string, 0, 4)
	if prio := strings.TrimSpace(msg.Priority); prio != "" && !strings.EqualFold(prio, fmail.PriorityNormal) {
		footerParts = append(footerParts, "["+strings.ToUpper(prio)+"]")
	}
	if len(msg.Tags) > 0 {
		tags := make([]string, 0, len(msg.Tags))
		for _, tag := range msg.Tags {
			tag = strings.TrimSpace(tag)
			if tag == "" {
				continue
			}
			tags = append(tags, "["+strings.ToLower(tag)+"]")
		}
		if len(tags) > 0 {
			footerParts = append(footerParts, strings.Join(tags, " "))
		}
	}
	if node.Parent != nil && node.Parent.Message != nil {
		footerParts = append(footerParts, "reply-to:"+shortID(node.Parent.Message.ID))
	}
	if len(footerParts) > 0 {
		footer := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(strings.Join(footerParts, "  "))
		out = append(out, truncateVis("  "+prefix+footer, width))
	}

	if truncated {
		label := "... [show more]"
		if expanded {
			label = "[show less]"
		}
		out = append(out, truncateVis("  "+prefix+label, width))
	}

	return out
}

func renderMessageBody(body string, width int, palette styles.Theme) []string {
	body = strings.ReplaceAll(body, "\r\n", "\n")
	body = strings.TrimRight(body, "\n")
	if body == "" {
		return []string{""}
	}

	codeBlock := lipgloss.NewStyle().Background(lipgloss.Color("236")).Foreground(lipgloss.Color("252"))
	inlineCode := lipgloss.NewStyle().Background(lipgloss.Color("238")).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb))

	lines := strings.Split(body, "\n")
	out := make([]string, 0, len(lines))
	inFence := false
	for _, raw := range lines {
		trimmed := strings.TrimSpace(raw)
		if strings.HasPrefix(trimmed, "```") {
			inFence = !inFence
			out = append(out, codeBlock.Render(trimmed))
			continue
		}
		if inFence {
			out = append(out, codeBlock.Render(raw))
			continue
		}

		wrapped := wordwrap.String(raw, maxInt(1, width))
		parts := strings.Split(wrapped, "\n")
		for _, part := range parts {
			out = append(out, highlightInlineCode(part, inlineCode))
		}
	}
	return out
}

func highlightInlineCode(line string, style lipgloss.Style) string {
	if !strings.Contains(line, "`") {
		return line
	}
	parts := strings.Split(line, "`")
	if len(parts) < 3 {
		return line
	}
	for i := 1; i < len(parts); i += 2 {
		parts[i] = style.Render("`" + parts[i] + "`")
	}
	return strings.Join(parts, "")
}

func prefixForNode(node *threading.ThreadNode, maxDepth int) (string, bool) {
	if node == nil || node.Parent == nil {
		return "", false
	}

	path := make([]*threading.ThreadNode, 0, 8)
	for cur := node; cur != nil; cur = cur.Parent {
		path = append(path, cur)
	}
	for i, j := 0, len(path)-1; i < j; i, j = i+1, j-1 {
		path[i], path[j] = path[j], path[i]
	}

	depth := len(path) - 1
	clamped := depth > maxDepth
	visibleDepth := depth
	if visibleDepth > maxDepth {
		visibleDepth = maxDepth
	}

	start := 0
	if depth > visibleDepth {
		start = depth - visibleDepth
	}

	segments := make([]string, 0, visibleDepth)
	for i := 0; i < visibleDepth; i++ {
		parent := path[start+i]
		child := path[start+i+1]
		if i == visibleDepth-1 {
			if hasNextSibling(parent, child) {
				segments = append(segments, "├─ ")
			} else {
				segments = append(segments, "└─ ")
			}
			continue
		}
		if hasNextSibling(parent, child) {
			segments = append(segments, "│  ")
		} else {
			segments = append(segments, "   ")
		}
	}
	return strings.Join(segments, ""), clamped
}

func hasNextSibling(parent, child *threading.ThreadNode) bool {
	if parent == nil || child == nil || len(parent.Children) == 0 {
		return false
	}
	children := append([]*threading.ThreadNode(nil), parent.Children...)
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
	for i := range children {
		if children[i] == child {
			return i < len(children)-1
		}
	}
	return false
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

func (v *threadView) bodyIsLong(id string) bool {
	if id == "" {
		return false
	}
	for i := range v.msgs {
		if v.msgs[i].ID != id {
			continue
		}
		raw := fmt.Sprint(v.msgs[i].Body)
		return len(strings.Split(strings.ReplaceAll(raw, "\r\n", "\n"), "\n")) > threadMaxVisibleLines
	}
	return false
}
