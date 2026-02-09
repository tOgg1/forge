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
	dashboardFeedLimit       = 500
)

type dashboardFocus int

const (
	focusAgents dashboardFocus = iota
	focusTopics
	focusFeed
)

type dashboardView struct {
	provider    data.MessageProvider
	projectID   string
	projectRoot string

	theme     styles.Theme
	colors    *styles.AgentColorMapper
	msgStyles styles.MessageStyles

	focus dashboardFocus

	agents       []fmail.AgentRecord
	topics       []data.TopicInfo
	recentCounts map[string]int

	selAgent int
	selTopic int

	feed       []fmail.Message
	feedPaused bool
	feedScroll int // lines from bottom
	selFeed    int

	subCh     <-chan fmail.Message
	subCancel func()
}

type dashboardDataMsg struct {
	agents []fmail.AgentRecord
	topics []data.TopicInfo
	recent map[string]int
	err    error
}

type dashboardTickMsg struct{}

type dashboardLiveMsg struct {
	msg fmail.Message
	ok  bool
}

func newDashboardView(provider data.MessageProvider, projectID string, projectRoot string) *dashboardView {
	return &dashboardView{
		provider:     provider,
		projectID:    projectID,
		projectRoot:  projectRoot,
		colors:       styles.NewAgentColorMapper(),
		recentCounts: make(map[string]int),
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
	if v.provider == nil {
		return tea.Tick(dashboardRefreshInterval, func(time.Time) tea.Msg { return dashboardTickMsg{} })
	}

	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{
		Topic:     "*",
		IncludeDM: true,
	})
	v.subCh = ch
	v.subCancel = cancel

	return tea.Batch(
		v.fetchCmd(),
		tea.Tick(dashboardRefreshInterval, func(time.Time) tea.Msg { return dashboardTickMsg{} }),
		v.listenCmd(),
	)
}

func (v *dashboardView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case dashboardTickMsg:
		return v.fetchCmd()
	case dashboardDataMsg:
		if typed.err != nil {
			return nil
		}
		v.agents = typed.agents
		v.topics = typed.topics
		v.recentCounts = typed.recent
		v.clampSelections()
		return nil
	case dashboardLiveMsg:
		if !typed.ok {
			return nil
		}
		v.appendFeed(typed.msg)
		return v.listenCmd()
	case tea.KeyMsg:
		return v.handleKey(typed)
	default:
		return nil
	}
}

func (v *dashboardView) View(width, height int, themeName Theme) string {
	v.applyTheme(themeName)
	if width <= 0 {
		return "loading..."
	}

	header := v.renderHeader(width)
	footer := v.renderFooter(width)

	bodyHeight := height - lipgloss.Height(header) - lipgloss.Height(footer)
	if bodyHeight < 1 {
		bodyHeight = 1
	}

	if width < 80 {
		feed := v.renderFeed(width, bodyHeight)
		return header + "\n" + feed + "\n" + footer
	}

	leftWidth := clampInt(width*35/100, 30, 50)
	rightWidth := width - leftWidth - 1
	if rightWidth < 20 {
		rightWidth = 20
		leftWidth = width - rightWidth - 1
	}

	left := v.renderLeft(leftWidth, bodyHeight)
	right := v.renderFeed(rightWidth, bodyHeight)
	body := lipgloss.JoinHorizontal(lipgloss.Top, left, right)
	return header + "\n" + body + "\n" + footer
}

func (v *dashboardView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "tab":
		v.focus = (v.focus + 1) % 3
		return nil
	case "shift+tab":
		v.focus = (v.focus + 2) % 3
		return nil
	case "up", "k":
		v.moveUp()
		return nil
	case "down", "j":
		v.moveDown()
		return nil
	case "g", "end":
		if v.focus == focusFeed {
			v.feedPaused = false
			v.feedScroll = 0
			return nil
		}
	case "enter":
		switch v.focus {
		case focusAgents:
			return pushViewCmd(ViewAgents)
		case focusTopics:
			return pushViewCmd(ViewTopics)
		case focusFeed:
			return pushViewCmd(ViewThread)
		}
	}
	return nil
}

func (v *dashboardView) moveUp() {
	switch v.focus {
	case focusAgents:
		if v.selAgent > 0 {
			v.selAgent--
		}
	case focusTopics:
		if v.selTopic > 0 {
			v.selTopic--
		}
	case focusFeed:
		v.feedPaused = true
		v.feedScroll++
	}
}

func (v *dashboardView) moveDown() {
	switch v.focus {
	case focusAgents:
		if v.selAgent < len(v.agents)-1 {
			v.selAgent++
		}
	case focusTopics:
		if v.selTopic < len(v.topics)-1 {
			v.selTopic++
		}
	case focusFeed:
		if v.feedScroll > 0 {
			v.feedScroll--
		}
		if v.feedScroll == 0 {
			v.feedPaused = false
		}
	}
}

func (v *dashboardView) clampSelections() {
	if v.selAgent >= len(v.agents) {
		v.selAgent = maxInt(0, len(v.agents)-1)
	}
	if v.selTopic >= len(v.topics) {
		v.selTopic = maxInt(0, len(v.topics)-1)
	}
}

func (v *dashboardView) appendFeed(msg fmail.Message) {
	v.feed = append(v.feed, msg)
	if len(v.feed) > dashboardFeedLimit {
		trim := len(v.feed) - dashboardFeedLimit
		v.feed = v.feed[trim:]
	}
	if !v.feedPaused {
		v.feedScroll = 0
	}
}

func (v *dashboardView) applyTheme(themeName Theme) {
	if v.theme.Name != "" && v.theme.Name == string(themeName) {
		return
	}
	theme, ok := styles.Themes[string(themeName)]
	if !ok {
		theme = styles.DefaultTheme
	}
	v.theme = theme
	v.msgStyles = styles.NewMessageStyles(theme, v.colors)
}

func (v *dashboardView) renderHeader(width int) string {
	status := "polling"
	if forgedSocketExists(v.projectRoot) {
		status = "connected"
	}

	left := lipgloss.NewStyle().Bold(true).Render("fmail TUI")
	mid := lipgloss.NewStyle().Foreground(lipgloss.Color(v.theme.Base.Muted)).Render(fmt.Sprintf("project=%s", v.projectID))
	right := lipgloss.NewStyle().Foreground(lipgloss.Color(v.theme.Base.Accent)).Render(status)

	line := left + "  " + mid
	spaces := width - lipgloss.Width(line) - lipgloss.Width(right)
	if spaces < 1 {
		spaces = 1
	}
	return line + strings.Repeat(" ", spaces) + right
}

func (v *dashboardView) renderFooter(width int) string {
	hint := "[Tab] focus  [Enter] open  [↑↓] navigate  [G] bottom  [?] help  q quit"
	style := lipgloss.NewStyle().Foreground(lipgloss.Color(v.theme.Base.Muted))
	text := style.Render(hint)
	if lipgloss.Width(text) > width {
		return style.Render("Tab focus  Enter open  q quit")
	}
	return text
}

func (v *dashboardView) renderLeft(width, height int) string {
	agentsTitle := v.sectionTitle("AGENTS", v.focus == focusAgents)
	topicsTitle := v.sectionTitle("TOPICS", v.focus == focusTopics)

	agentLines := v.renderAgentLines(width, maxInt(3, height/2-2))
	topicLines := v.renderTopicLines(width, maxInt(3, height-lipgloss.Height(agentsTitle)-len(agentLines)-4))

	var b strings.Builder
	b.WriteString(agentsTitle)
	b.WriteString("\n")
	b.WriteString(strings.Join(agentLines, "\n"))
	b.WriteString("\n\n")
	b.WriteString(topicsTitle)
	b.WriteString("\n")
	b.WriteString(strings.Join(topicLines, "\n"))

	panel := styles.PanelStyle(v.theme, v.focus == focusAgents || v.focus == focusTopics).Width(width).Height(height)
	return panel.Render(b.String())
}

func (v *dashboardView) renderFeed(width, height int) string {
	title := v.sectionTitle("LIVE FEED", v.focus == focusFeed)

	lines := v.renderFeedLines(width, maxInt(1, height-2))
	content := title + "\n" + strings.Join(lines, "\n")
	panel := styles.PanelStyle(v.theme, v.focus == focusFeed).Width(width).Height(height)
	return panel.Render(content)
}

func (v *dashboardView) sectionTitle(label string, focused bool) string {
	color := v.theme.Chrome.Header
	if focused {
		color = v.theme.Chrome.SelectedItem
	}
	return lipgloss.NewStyle().Foreground(lipgloss.Color(color)).Bold(true).Render(label)
}

func (v *dashboardView) renderAgentLines(width, limit int) []string {
	if limit < 1 {
		limit = 1
	}
	now := time.Now().UTC()
	agents := append([]fmail.AgentRecord(nil), v.agents...)
	sort.SliceStable(agents, func(i, j int) bool {
		return agents[i].LastSeen.After(agents[j].LastSeen)
	})

	lines := make([]string, 0, minInt(limit, len(agents)))
	for i := 0; i < len(agents) && len(lines) < limit; i++ {
		rec := agents[i]
		prefix := "  "
		if v.focus == focusAgents && i == v.selAgent {
			prefix = "▸ "
		}
		pres := presenceDot(now, rec.LastSeen)
		name := v.colors.Foreground(rec.Name).Render(rec.Name)
		status := strings.TrimSpace(rec.Status)
		if status == "" {
			status = "—"
		}
		status = truncate(status, maxInt(0, width-18))
		lines = append(lines, fmt.Sprintf("%s%s %s %s", prefix, pres, name, status))
	}
	if len(lines) == 0 {
		lines = []string{"(no agents)"}
	}
	return lines
}

func (v *dashboardView) renderTopicLines(width, limit int) []string {
	if limit < 1 {
		limit = 1
	}
	now := time.Now().UTC()
	topics := append([]data.TopicInfo(nil), v.topics...)
	sort.SliceStable(topics, func(i, j int) bool {
		return topics[i].LastActivity.After(topics[j].LastActivity)
	})

	lines := make([]string, 0, minInt(limit, len(topics)))
	for i := 0; i < len(topics) && len(lines) < limit; i++ {
		info := topics[i]
		prefix := "  "
		if v.focus == focusTopics && i == v.selTopic {
			prefix = "▸ "
		}
		bar := activityBar(v.recentCounts[info.Name])
		name := truncate(info.Name, 12)
		age := relTime(now, info.LastActivity)
		lines = append(lines, fmt.Sprintf("%s%s %-12s %3d %s", prefix, bar, name, info.MessageCount, age))
	}
	if len(lines) == 0 {
		lines = []string{"(no topics)"}
	}
	return lines
}

func (v *dashboardView) renderFeedLines(width, height int) []string {
	if height < 1 {
		height = 1
	}
	if len(v.feed) == 0 {
		return []string{"(no messages yet)"}
	}

	now := time.Now().UTC()

	start := 0
	if !v.feedPaused {
		start = maxInt(0, len(v.feed)-height)
	} else {
		// Scroll from bottom.
		bottom := maxInt(0, len(v.feed)-(height))
		start = bottom - v.feedScroll
		if start < 0 {
			start = 0
		}
	}

	end := minInt(len(v.feed), start+height)
	lines := make([]string, 0, end-start)
	for _, msg := range v.feed[start:end] {
		ts := msg.Time
		if ts.IsZero() {
			ts = now
		}
		head := fmt.Sprintf("%s %s -> %s", ts.Format("15:04:05"), msg.From, msg.To)
		head = v.msgStyles.HeaderBase.Render(head)
		body := firstLine(msg.Body)
		body = truncate(body, maxInt(0, width-2))
		lines = append(lines, fmt.Sprintf("%s  %s", head, body))
	}
	return lines
}

func (v *dashboardView) fetchCmd() tea.Cmd {
	return func() tea.Msg {
		if v.provider == nil {
			return dashboardDataMsg{err: fmt.Errorf("provider unavailable")}
		}
		topics, err := v.provider.Topics()
		if err != nil {
			return dashboardDataMsg{err: err}
		}
		agents, err := v.provider.Agents()
		if err != nil {
			return dashboardDataMsg{err: err}
		}

		now := time.Now().UTC()
		recent := make(map[string]int, 16)
		sort.SliceStable(topics, func(i, j int) bool { return topics[i].LastActivity.After(topics[j].LastActivity) })
		for i := 0; i < len(topics) && i < 8; i++ {
			name := topics[i].Name
			msgs, err := v.provider.Messages(name, data.MessageFilter{Since: now.Add(-5 * time.Minute)})
			if err != nil {
				continue
			}
			recent[name] = len(msgs)
		}

		sort.SliceStable(agents, func(i, j int) bool { return agents[i].LastSeen.After(agents[j].LastSeen) })
		return dashboardDataMsg{agents: agents, topics: topics, recent: recent}
	}
}

func (v *dashboardView) listenCmd() tea.Cmd {
	return func() tea.Msg {
		if v.subCh == nil {
			return dashboardLiveMsg{ok: false}
		}
		msg, ok := <-v.subCh
		return dashboardLiveMsg{msg: msg, ok: ok}
	}
}

func forgedSocketExists(root string) bool {
	if strings.TrimSpace(root) == "" {
		return false
	}
	socketPath := filepath.Join(root, ".fmail", "forged.sock")
	info, err := os.Stat(socketPath)
	if err != nil {
		return false
	}
	return !info.IsDir()
}

func presenceDot(now time.Time, lastSeen time.Time) string {
	if lastSeen.IsZero() {
		return "◌"
	}
	age := now.Sub(lastSeen)
	switch {
	case age < time.Minute:
		return "●"
	case age < 10*time.Minute:
		return "○"
	default:
		return "◌"
	}
}

func relTime(now time.Time, ts time.Time) string {
	if ts.IsZero() {
		return "—"
	}
	d := now.Sub(ts)
	if d < time.Minute {
		return "now"
	}
	if d < time.Hour {
		return fmt.Sprintf("%dm", int(d.Minutes()))
	}
	if d < 24*time.Hour {
		return fmt.Sprintf("%dh", int(d.Hours()))
	}
	return fmt.Sprintf("%dd", int(d.Hours()/24))
}

func activityBar(count int) string {
	switch {
	case count >= 12:
		return "██"
	case count >= 6:
		return "▓▓"
	case count >= 1:
		return "░░"
	default:
		return "  "
	}
}

func firstLine(body any) string {
	switch v := body.(type) {
	case nil:
		return ""
	case string:
		return strings.SplitN(v, "\n", 2)[0]
	default:
		s := fmt.Sprintf("%v", v)
		return strings.SplitN(s, "\n", 2)[0]
	}
}

func truncate(s string, max int) string {
	if max <= 0 {
		return ""
	}
	if lipgloss.Width(s) <= max {
		return s
	}
	if max <= 3 {
		return s[:max]
	}
	return s[:max-3] + "..."
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
