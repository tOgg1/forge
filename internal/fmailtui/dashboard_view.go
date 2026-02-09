package fmailtui

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

const (
	dashboardRefreshInterval = 2 * time.Second
	dashboardMetadataRefresh = 10 * time.Second
	dashboardHotWindow       = 5 * time.Minute
	dashboardFeedLimit       = 500
)

type dashboardFocus int

const (
	focusAgents dashboardFocus = iota
	focusTopics
	focusFeed
)

func (f dashboardFocus) label() string {
	switch f {
	case focusAgents:
		return "agents"
	case focusTopics:
		return "topics"
	case focusFeed:
		return "feed"
	default:
		return ""
	}
}

type dashboardTickMsg struct {
	now time.Time
}

type dashboardRefreshMsg struct {
	now    time.Time
	agents []fmail.AgentRecord
	topics []data.TopicInfo
	err    error
}

type dashboardIncomingMsg struct {
	msg fmail.Message
}

type dashboardHotEvent struct {
	topic string
	at    time.Time
	count int
}

type dashboardView struct {
	root          string
	projectID     string
	provider      data.MessageProvider
	notifications *notificationCenter
	now           time.Time
	lastErr       error
	agents        []fmail.AgentRecord
	topics        []data.TopicInfo
	hotCounts     map[string]int
	focus         dashboardFocus
	agentIdx      int
	topicIdx      int
	feed          []fmail.Message
	feedOffset    int // 0 = follow tail; >0 = paused, lines from tail
	hotEvents     []dashboardHotEvent
	topicCounts   map[string]int
	lastRefresh   time.Time

	subCh     <-chan fmail.Message
	subCancel func()
}

func newDashboardView(root, projectID string, provider data.MessageProvider, notifications *notificationCenter) *dashboardView {
	return &dashboardView{
		root:          root,
		projectID:     projectID,
		provider:      provider,
		notifications: notifications,
		hotCounts:     make(map[string]int),
		topicCounts:   make(map[string]int),
		focus:         focusAgents,
	}
}

func (v *dashboardView) Close() {
	if v == nil {
		return
	}
	if v.subCancel != nil {
		v.subCancel()
		v.subCancel = nil
	}
}

func (v *dashboardView) Init() tea.Cmd {
	v.startSubscription()
	return tea.Batch(
		v.refreshCmd(),
		dashboardTickCmd(),
		v.waitForMessageCmd(),
	)
}

func (v *dashboardView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case dashboardTickMsg:
		cmds := []tea.Cmd{dashboardTickCmd()}
		if v.shouldRefresh(typed.now) {
			cmds = append(cmds, v.refreshCmd())
		}
		return tea.Batch(cmds...)
	case dashboardRefreshMsg:
		v.now = typed.now
		v.lastErr = typed.err
		if typed.err == nil {
			v.agents = typed.agents
			v.applyTopicsSnapshot(typed.topics, typed.now)
			v.clampSelection()
			v.lastRefresh = typed.now
		}
		return nil
	case dashboardIncomingMsg:
		v.appendFeed(typed.msg)
		v.applyIncoming(typed.msg)
		return v.waitForMessageCmd()
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *dashboardView) FocusLabel() string {
	if v == nil {
		return ""
	}
	return v.focus.label()
}

func (v *dashboardView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}

	palette := themePalette(theme)
	base := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Background(lipgloss.Color(palette.Base.Background))

	content := v.renderPanels(width, height, palette)
	if v.lastErr != nil {
		errLine := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Render("data error: " + truncate(v.lastErr.Error(), maxInt(0, width-2)))
		content = lipgloss.JoinVertical(lipgloss.Left, content, errLine)
	}

	return base.Render(content)
}

func (v *dashboardView) MinSize() (int, int) {
	return 60, 12
}

func (v *dashboardView) renderPanels(width, height int, palette styles.Theme) string {
	if width < 80 {
		return v.renderFeedPanel(width, height, palette, true)
	}

	leftW := clampInt(width/3, 30, 50)
	rightW := width - leftW - styles.LayoutGap
	if rightW < 20 {
		return v.renderFeedPanel(width, height, palette, true)
	}

	left := v.renderLeftPanel(leftW, height, palette)
	right := v.renderFeedPanel(rightW, height, palette, false)
	return lipgloss.JoinHorizontal(lipgloss.Top, left, strings.Repeat(" ", styles.LayoutGap), right)
}

func (v *dashboardView) renderLeftPanel(width, height int, palette styles.Theme) string {
	panel := styles.PanelStyle(palette, v.focus != focusFeed)
	innerW := maxInt(0, width-(styles.LayoutInnerPadding*2)-2)

	agentsTitle, agentsBody := v.renderAgents(innerW, palette)
	topicsTitle, topicsBody := v.renderTopics(innerW, palette)

	// Split space roughly: agents gets ~40%, topics rest.
	usableH := maxInt(0, height-(styles.LayoutInnerPadding*2)-2)
	agentsMax := maxInt(4, usableH/2)
	agentsBody = clampLines(agentsBody, agentsMax)
	topicsBody = clampLines(topicsBody, maxInt(0, usableH-lipgloss.Height(agentsTitle)-lipgloss.Height(agentsBody)-1))

	divider := styles.DividerStyle(palette).Render(strings.Repeat("─", innerW))
	content := lipgloss.JoinVertical(lipgloss.Left,
		agentsTitle,
		agentsBody,
		divider,
		topicsTitle,
		topicsBody,
	)
	return panel.Width(width).Height(height).Render(content)
}

func (v *dashboardView) renderAgents(width int, palette styles.Theme) (string, string) {
	now := v.now
	if now.IsZero() {
		now = time.Now().UTC()
	}

	records := append([]fmail.AgentRecord(nil), v.agents...)
	sort.SliceStable(records, func(i, j int) bool {
		if !records[i].LastSeen.Equal(records[j].LastSeen) {
			return records[i].LastSeen.After(records[j].LastSeen)
		}
		return records[i].Name < records[j].Name
	})

	onlineCount := 0
	lines := make([]string, 0, minInt(len(records), 8))
	mapper := styles.NewAgentColorMapperWithPalette(palette.AgentPalette)
	for idx, rec := range records {
		presence := presenceIndicator(now, rec.LastSeen)
		if presence == "●" {
			onlineCount++
		}
		name := mapper.Foreground(rec.Name).Render(rec.Name)
		status := strings.TrimSpace(rec.Status)
		if status != "" {
			status = fmt.Sprintf("%q", status)
		}

		prefix := "  "
		if v.focus == focusAgents && idx == v.agentIdx {
			prefix = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Chrome.SelectedItem)).Bold(true).Render("▸ ")
		}
		line := fmt.Sprintf("%s%s %s", prefix, presence, name)
		if status != "" {
			line = fmt.Sprintf("%s %s", line, mutedStyle(palette).Render(truncate(status, maxInt(0, width-lipgloss.Width(line)-1))))
		}
		lines = append(lines, truncateVis(line, width))
	}

	title := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render(fmt.Sprintf("AGENTS (%d online)  Enter:open", onlineCount))
	return title, strings.Join(lines, "\n")
}

func (v *dashboardView) renderTopics(width int, palette styles.Theme) (string, string) {
	topics := append([]data.TopicInfo(nil), v.topics...)
	sort.SliceStable(topics, func(i, j int) bool {
		if !topics[i].LastActivity.Equal(topics[j].LastActivity) {
			return topics[i].LastActivity.After(topics[j].LastActivity)
		}
		return topics[i].Name < topics[j].Name
	})

	lines := make([]string, 0, minInt(len(topics), 8))
	maxHot := 0
	for _, info := range topics {
		if c := v.hotCounts[info.Name]; c > maxHot {
			maxHot = c
		}
	}

	for idx, info := range topics {
		bar := topicHeatBar(v.hotCounts[info.Name], maxHot)
		prefix := "  "
		if v.focus == focusTopics && idx == v.topicIdx {
			prefix = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Chrome.SelectedItem)).Bold(true).Render("▸ ")
		}
		label := fmt.Sprintf("%s%s %s", prefix, bar, info.Name)
		count := v.hotCounts[info.Name]
		meta := fmt.Sprintf("%d msgs/5m", count)
		line := label
		remaining := width - lipgloss.Width(label) - 1
		if remaining > 0 {
			line = fmt.Sprintf("%s %s", label, mutedStyle(palette).Render(truncate(meta, remaining)))
		}
		lines = append(lines, truncateVis(line, width))
		if len(lines) >= 6 {
			break
		}
	}

	title := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render("TOPICS (hot)  Enter:thread")
	return title, strings.Join(lines, "\n")
}

func (v *dashboardView) renderFeedPanel(width, height int, palette styles.Theme, fullWidth bool) string {
	panel := styles.PanelStyle(palette, v.focus == focusFeed || fullWidth)
	innerW := maxInt(0, width-(styles.LayoutInnerPadding*2)-2)
	innerH := maxInt(0, height-(styles.LayoutInnerPadding*2)-2)

	feedState := "follow"
	if v.feedOffset > 0 {
		feedState = fmt.Sprintf("paused:%d", v.feedOffset)
	}
	titleText := "LIVE FEED [" + feedState + "]  Enter:thread  Ctrl+N"
	if v.notifications != nil {
		if unread := v.notifications.UnreadCount(); unread > 0 {
			titleText = fmt.Sprintf("LIVE FEED  Ctrl+N [%d]", unread)
			titleText = fmt.Sprintf("%s [%s]  Enter:thread", titleText, feedState)
		}
	}
	title := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render(titleText)

	lines := v.renderFeedLines(innerW, innerH-lipgloss.Height(title)-1, palette)
	content := lipgloss.JoinVertical(lipgloss.Left, title, lines)
	return panel.Width(width).Height(height).Render(content)
}

func (v *dashboardView) renderFeedLines(width, maxLines int, palette styles.Theme) string {
	if maxLines <= 0 {
		return ""
	}
	mapper := styles.NewAgentColorMapperWithPalette(palette.AgentPalette)

	start := 0
	if v.feedOffset > 0 {
		start = maxInt(0, len(v.feed)-maxLines-v.feedOffset)
	} else {
		start = maxInt(0, len(v.feed)-maxLines)
	}
	end := minInt(len(v.feed), start+maxLines)

	out := make([]string, 0, end-start)
	for i := start; i < end; i++ {
		msg := v.feed[i]
		ts := msg.Time.UTC()
		tsStr := ts.Format("15:04:05")
		from := mapper.Foreground(msg.From).Render(msg.From)
		target := strings.TrimSpace(msg.To)
		if target == "" {
			target = "(unknown)"
		}
		body := truncate(firstLine(msg.Body), maxInt(0, width-2))
		if body == "" {
			body = "(empty)"
		}

		priority := ""
		switch strings.ToLower(strings.TrimSpace(msg.Priority)) {
		case strings.ToLower(fmail.PriorityHigh):
			priority = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Bold(true).Render("[HIGH] ")
		case strings.ToLower(fmail.PriorityLow):
			priority = mutedStyle(palette).Render("[low] ")
		}

		line := fmt.Sprintf("%s %s \u2192 %s  %s%s", mutedStyle(palette).Render(tsStr), from, mutedStyle(palette).Render(target), priority, body)
		out = append(out, truncateVis(line, width))
	}

	if v.feedOffset > 0 {
		out = append(out, mutedStyle(palette).Render(fmt.Sprintf("PAUSED (%d)  j/k scroll  G resume", v.feedOffset)))
	}
	return strings.Join(out, "\n")
}

func mutedStyle(palette styles.Theme) lipgloss.Style {
	return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
}

func (v *dashboardView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "tab":
		v.focus = (v.focus + 1) % 3
		return nil
	case "enter":
		switch v.focus {
		case focusAgents:
			return pushViewCmd(ViewAgents)
		case focusTopics:
			if v.topicIdx >= 0 && v.topicIdx < len(v.topics) {
				return tea.Batch(
					openThreadCmd(v.topics[v.topicIdx].Name, ""),
					pushViewCmd(ViewThread),
				)
			}
			return pushViewCmd(ViewThread)
		case focusFeed:
			if len(v.feed) > 0 {
				idx := len(v.feed) - 1 - v.feedOffset
				if idx < 0 {
					idx = 0
				}
				if idx >= len(v.feed) {
					idx = len(v.feed) - 1
				}
				target := strings.TrimSpace(v.feed[idx].To)
				if target != "" {
					return tea.Batch(openThreadCmd(target, ""), pushViewCmd(ViewThread))
				}
			}
			return pushViewCmd(ViewThread)
		default:
			return pushViewCmd(ViewTopics)
		}
	case "up", "k":
		switch v.focus {
		case focusAgents:
			v.agentIdx = maxInt(0, v.agentIdx-1)
		case focusTopics:
			v.topicIdx = maxInt(0, v.topicIdx-1)
		case focusFeed:
			v.feedOffset++
		}
		return nil
	case "down", "j":
		switch v.focus {
		case focusAgents:
			v.agentIdx = minInt(maxInt(0, len(v.agents)-1), v.agentIdx+1)
		case focusTopics:
			v.topicIdx = minInt(maxInt(0, len(v.topics)-1), v.topicIdx+1)
		case focusFeed:
			v.feedOffset = maxInt(0, v.feedOffset-1)
		}
		return nil
	case "end", "G":
		if v.focus == focusFeed {
			v.feedOffset = 0
		}
		return nil
	}

	// Quick routes.
	switch msg.String() {
	case "t":
		return pushViewCmd(ViewTopics)
	case "a":
		return pushViewCmd(ViewAgents)
	case "/":
		return pushViewCmd(ViewSearch)
	case "l":
		return pushViewCmd(ViewLiveTail)
	case "ctrl+n":
		return pushViewCmd(ViewNotify)
	case "?":
		// Global help handled by Model.
		return nil
	}
	return nil
}

func (v *dashboardView) refreshCmd() tea.Cmd {
	if v.provider == nil {
		return func() tea.Msg {
			return dashboardRefreshMsg{now: time.Now().UTC(), err: fmt.Errorf("missing provider")}
		}
	}
	return func() tea.Msg {
		now := time.Now().UTC()
		agents, err := v.provider.Agents()
		if err != nil {
			return dashboardRefreshMsg{now: now, err: err}
		}
		topics, err := v.provider.Topics()
		if err != nil {
			return dashboardRefreshMsg{now: now, err: err}
		}
		return dashboardRefreshMsg{now: now, agents: agents, topics: topics}
	}
}

func dashboardTickCmd() tea.Cmd {
	return tea.Tick(dashboardRefreshInterval, func(ts time.Time) tea.Msg {
		return dashboardTickMsg{now: ts.UTC()}
	})
}

func (v *dashboardView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.subCancel = cancel
}

func (v *dashboardView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return dashboardIncomingMsg{msg: msg}
	}
}

func (v *dashboardView) appendFeed(msg fmail.Message) {
	v.feed = append(v.feed, msg)
	if len(v.feed) > dashboardFeedLimit {
		v.feed = v.feed[len(v.feed)-dashboardFeedLimit:]
	}
}

func (v *dashboardView) shouldRefresh(now time.Time) bool {
	if now.IsZero() {
		now = time.Now().UTC()
	}
	if v.lastRefresh.IsZero() {
		return true
	}
	return now.Sub(v.lastRefresh) >= dashboardMetadataRefresh
}

func (v *dashboardView) applyIncoming(msg fmail.Message) {
	target := strings.TrimSpace(msg.To)
	if target == "" || strings.HasPrefix(target, "@") {
		return
	}
	ts := msg.Time.UTC()
	if ts.IsZero() {
		ts = time.Now().UTC()
	}
	v.addHotEvent(target, ts, 1)
	v.bumpTopic(target, ts)
}

func (v *dashboardView) applyTopicsSnapshot(topics []data.TopicInfo, now time.Time) {
	if now.IsZero() {
		now = time.Now().UTC()
	}
	nextCounts := make(map[string]int, len(topics))
	for _, topic := range topics {
		name := strings.TrimSpace(topic.Name)
		if name == "" {
			continue
		}
		nextCounts[name] = maxInt(0, topic.MessageCount)
		if prev, ok := v.topicCounts[name]; ok && topic.MessageCount > prev {
			v.addHotEvent(name, topic.LastActivity, topic.MessageCount-prev)
		} else if !ok && topic.MessageCount > 0 && topic.LastActivity.After(now.Add(-dashboardHotWindow)) {
			v.addHotEvent(name, topic.LastActivity, 1)
		}
	}
	v.topicCounts = nextCounts
	v.topics = append([]data.TopicInfo(nil), topics...)
	v.pruneHotEvents(now)
}

func (v *dashboardView) addHotEvent(topic string, ts time.Time, count int) {
	if count <= 0 {
		return
	}
	if ts.IsZero() {
		ts = time.Now().UTC()
	}
	v.hotEvents = append(v.hotEvents, dashboardHotEvent{
		topic: topic,
		at:    ts.UTC(),
		count: count,
	})
	v.pruneHotEvents(ts)
}

func (v *dashboardView) pruneHotEvents(now time.Time) {
	if now.IsZero() {
		now = time.Now().UTC()
	}
	if len(v.hotEvents) == 0 {
		v.hotCounts = map[string]int{}
		return
	}
	cutoff := now.Add(-dashboardHotWindow)
	kept := make([]dashboardHotEvent, 0, len(v.hotEvents))
	next := make(map[string]int, len(v.hotCounts))
	for _, ev := range v.hotEvents {
		if ev.at.Before(cutoff) {
			continue
		}
		kept = append(kept, ev)
		next[ev.topic] += ev.count
	}
	v.hotEvents = kept
	v.hotCounts = next
}

func (v *dashboardView) bumpTopic(name string, ts time.Time) {
	v.topicCounts[name] = v.topicCounts[name] + 1
	for i := range v.topics {
		if v.topics[i].Name != name {
			continue
		}
		v.topics[i].MessageCount++
		if ts.After(v.topics[i].LastActivity) {
			v.topics[i].LastActivity = ts
		}
		return
	}
	v.topics = append(v.topics, data.TopicInfo{
		Name:         name,
		MessageCount: 1,
		LastActivity: ts,
	})
}

func (v *dashboardView) clampSelection() {
	if v.agentIdx < 0 {
		v.agentIdx = 0
	}
	if v.agentIdx >= len(v.agents) {
		v.agentIdx = maxInt(0, len(v.agents)-1)
	}
	if v.topicIdx < 0 {
		v.topicIdx = 0
	}
	if v.topicIdx >= len(v.topics) {
		v.topicIdx = maxInt(0, len(v.topics)-1)
	}
}

func presenceIndicator(now, lastSeen time.Time) string {
	if lastSeen.IsZero() {
		return "◌"
	}
	diff := now.Sub(lastSeen)
	switch {
	case diff <= time.Minute:
		return "●"
	case diff <= 10*time.Minute:
		return "○"
	default:
		return "◌"
	}
}

func topicHeatBar(count, max int) string {
	if max <= 0 || count <= 0 {
		return "░░"
	}
	ratio := float64(count) / float64(max)
	switch {
	case ratio >= 0.75:
		return "██"
	case ratio >= 0.5:
		return "▓▓"
	case ratio >= 0.25:
		return "▒▒"
	default:
		return "░░"
	}
}

func truncate(s string, max int) string {
	if max <= 0 {
		return ""
	}
	if len(s) <= max {
		return s
	}
	if max <= 3 {
		return s[:max]
	}
	return s[:max-3] + "..."
}

func truncateVis(s string, max int) string {
	if max <= 0 {
		return ""
	}
	if lipgloss.Width(s) <= max {
		return s
	}
	runes := []rune(s)
	if len(runes) <= max {
		return string(runes[:max])
	}
	return string(runes[:max])
}

func clampLines(s string, maxLines int) string {
	if maxLines <= 0 {
		return ""
	}
	lines := strings.Split(s, "\n")
	if len(lines) <= maxLines {
		return s
	}
	return strings.Join(lines[:maxLines], "\n")
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

func clampInt(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

func themePalette(theme Theme) styles.Theme {
	if palette, ok := styles.Themes[string(theme)]; ok {
		return palette
	}
	return styles.DefaultTheme
}

func firstLine(body any) string {
	if body == nil {
		return ""
	}
	s, ok := body.(string)
	if !ok {
		s = fmt.Sprint(body)
	}
	s = strings.TrimSpace(s)
	if idx := strings.IndexByte(s, '\n'); idx >= 0 {
		s = s[:idx]
	}
	return strings.TrimSpace(s)
}

func (v *dashboardView) forgedSocketPresent() bool {
	if strings.TrimSpace(v.root) == "" {
		return false
	}
	path := filepath.Join(v.root, ".fmail", "forged.sock")
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return !info.IsDir()
}
