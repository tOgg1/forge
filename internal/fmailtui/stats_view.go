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
)

const statsRefreshInterval = 30 * time.Second

type statsTickMsg struct{}

func statsTickCmd() tea.Cmd {
	return tea.Tick(statsRefreshInterval, func(time.Time) tea.Msg { return statsTickMsg{} })
}

type statsLoadedMsg struct {
	now     time.Time
	start   time.Time
	end     time.Time
	allTime bool
	msgs    []fmail.Message
	err     error
}

type statsIncomingMsg struct {
	msg fmail.Message
}

type statsView struct {
	root     string
	self     string
	provider data.MessageProvider

	now       time.Time
	windowEnd time.Time
	windowIdx int
	windows   []time.Duration

	loading bool
	lastErr error

	loadedStart time.Time
	loadedEnd   time.Time
	all         []fmail.Message
	seen        map[string]struct{}

	subCh     <-chan fmail.Message
	subCancel func()

	snap statsSnapshot
}

func newStatsView(root, self string, provider data.MessageProvider) *statsView {
	return &statsView{
		root:     root,
		self:     strings.TrimSpace(self),
		provider: provider,
		windows: []time.Duration{
			4 * time.Hour,
			12 * time.Hour,
			24 * time.Hour,
			7 * 24 * time.Hour,
			30 * 24 * time.Hour,
			0, // all-time
		},
		windowIdx: 2, // 24h
		seen:      make(map[string]struct{}, 1024),
	}
}

func (v *statsView) Init() tea.Cmd {
	v.startSubscription()
	v.loading = true
	return tea.Batch(v.loadCmd(), statsTickCmd(), v.waitForMessageCmd())
}

func (v *statsView) Close() {
	if v.subCancel != nil {
		v.subCancel()
		v.subCancel = nil
	}
	v.subCh = nil
}

func (v *statsView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.subCancel = cancel
}

func (v *statsView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return statsIncomingMsg{msg: msg}
	}
}

func (v *statsView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case statsTickMsg:
		now := time.Now().UTC()
		v.now = now
		if v.followingTail(now) {
			v.loading = true
			return tea.Batch(v.loadCmd(), statsTickCmd())
		}
		return statsTickCmd()
	case statsLoadedMsg:
		v.applyLoaded(typed)
		return nil
	case statsIncomingMsg:
		v.applyIncoming(typed.msg)
		return v.waitForMessageCmd()
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *statsView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "r":
		v.loading = true
		return v.loadCmd()
	case "[":
		if v.windowIdx > 0 {
			v.windowIdx--
			v.windowEnd = time.Time{}
			v.loading = true
			return v.loadCmd()
		}
	case "]":
		if v.windowIdx < len(v.windows)-1 {
			v.windowIdx++
			v.windowEnd = time.Time{}
			v.loading = true
			return v.loadCmd()
		}
	case "left", "h":
		if v.windows[v.windowIdx] > 0 {
			v.windowEnd = v.windowEnd.Add(-v.panStep())
			v.loading = true
			return v.loadCmd()
		}
	case "right", "l":
		if v.windows[v.windowIdx] > 0 {
			v.windowEnd = v.windowEnd.Add(v.panStep())
			v.loading = true
			return v.loadCmd()
		}
	}
	return nil
}

func (v *statsView) followingTail(now time.Time) bool {
	if v.windows[v.windowIdx] == 0 {
		return false
	}
	if v.windowEnd.IsZero() {
		return true
	}
	diff := now.Sub(v.windowEnd)
	if diff < 0 {
		return true
	}
	return diff <= 2*time.Second
}

func (v *statsView) panStep() time.Duration {
	d := v.windows[v.windowIdx]
	if d <= 0 {
		return 0
	}
	step := d / 6
	if step < 15*time.Minute {
		step = 15 * time.Minute
	}
	return step
}

func (v *statsView) windowBounds(now time.Time) (time.Time, time.Time, bool) {
	d := v.windows[v.windowIdx]
	if d == 0 {
		return time.Time{}, time.Time{}, true
	}
	end := v.windowEnd
	if end.IsZero() {
		end = now
	}
	start := end.Add(-d)
	return start, end, false
}

func (v *statsView) loadCmd() tea.Cmd {
	provider := v.provider
	self := v.self
	windowIdx := v.windowIdx
	windowEnd := v.windowEnd
	windows := append([]time.Duration(nil), v.windows...)

	return func() tea.Msg {
		now := time.Now().UTC()
		if provider == nil {
			return statsLoadedMsg{now: now}
		}

		d := time.Duration(0)
		if windowIdx >= 0 && windowIdx < len(windows) {
			d = windows[windowIdx]
		}
		allTime := d == 0

		end := windowEnd
		if end.IsZero() {
			end = now
		}
		start := end.Add(-d)

		filter := data.MessageFilter{}
		if !allTime {
			filter.Since = start
			filter.Until = end
		}

		merged := make([]fmail.Message, 0, 1024)
		seen := make(map[string]struct{}, 1024)

		topics, err := provider.Topics()
		if err != nil {
			return statsLoadedMsg{now: now, err: err}
		}
		for i := range topics {
			topic := strings.TrimSpace(topics[i].Name)
			if topic == "" {
				continue
			}
			msgs, err := provider.Messages(topic, filter)
			if err != nil {
				return statsLoadedMsg{now: now, err: err}
			}
			for _, msg := range msgs {
				key := statsDedupKey(msg)
				if _, ok := seen[key]; ok {
					continue
				}
				seen[key] = struct{}{}
				merged = append(merged, msg)
			}
		}

		convs, err := provider.DMConversations(self)
		if err == nil {
			for i := range convs {
				agent := strings.TrimSpace(convs[i].Agent)
				if agent == "" {
					continue
				}
				msgs, dmErr := provider.DMs(agent, filter)
				if dmErr != nil {
					return statsLoadedMsg{now: now, err: dmErr}
				}
				for _, msg := range msgs {
					key := statsDedupKey(msg)
					if _, ok := seen[key]; ok {
						continue
					}
					seen[key] = struct{}{}
					merged = append(merged, msg)
				}
			}
		}

		sortMessages(merged)
		return statsLoadedMsg{
			now:     now,
			start:   start,
			end:     end,
			allTime: allTime,
			msgs:    merged,
		}
	}
}

func (v *statsView) applyLoaded(msg statsLoadedMsg) {
	v.loading = false
	v.now = msg.now
	v.lastErr = msg.err
	if msg.err != nil {
		return
	}

	v.all = append(v.all[:0], msg.msgs...)
	v.seen = make(map[string]struct{}, len(v.all))
	for i := range v.all {
		v.seen[statsDedupKey(v.all[i])] = struct{}{}
	}

	start := msg.start
	end := msg.end
	if msg.allTime {
		minT, maxT := statsMinMaxTime(v.all)
		start = minT
		end = maxT
		if !end.IsZero() {
			// Compute uses an exclusive end bound; keep latest message included.
			end = end.Add(1 * time.Second)
		} else {
			end = msg.now
		}
	}

	v.loadedStart = start
	v.loadedEnd = end
	v.snap = computeStats(v.all, v.loadedStart, v.loadedEnd)
	if v.followingTail(msg.now) && !msg.allTime {
		v.windowEnd = msg.now
	}
}

func (v *statsView) applyIncoming(msg fmail.Message) {
	now := time.Now().UTC()
	v.now = now
	key := statsDedupKey(msg)
	if _, ok := v.seen[key]; ok {
		return
	}
	v.seen[key] = struct{}{}

	// Respect pan: only include new messages when following the tail.
	if v.windows[v.windowIdx] > 0 && !v.followingTail(now) {
		return
	}

	v.all = append(v.all, msg)
	sortMessages(v.all)

	// Extend tail window.
	if v.windows[v.windowIdx] > 0 {
		if v.loadedEnd.IsZero() || msg.Time.After(v.loadedEnd) {
			v.loadedEnd = now
		}
		if v.windowEnd.IsZero() {
			v.windowEnd = now
		}
	}
	if v.loadedStart.IsZero() || v.loadedEnd.IsZero() {
		start, end := statsMinMaxTime(v.all)
		v.loadedStart = start
		if !end.IsZero() {
			v.loadedEnd = end.Add(1 * time.Second)
		} else {
			v.loadedEnd = end
		}
	}

	v.snap = computeStats(v.all, v.loadedStart, v.loadedEnd)
}

func statsDedupKey(msg fmail.Message) string {
	return strings.TrimSpace(msg.ID) + "|" + strings.TrimSpace(msg.From) + "|" + strings.TrimSpace(msg.To)
}

func statsMinMaxTime(messages []fmail.Message) (time.Time, time.Time) {
	var minT time.Time
	var maxT time.Time
	for i := range messages {
		ts := messages[i].Time
		if ts.IsZero() {
			continue
		}
		if minT.IsZero() || ts.Before(minT) {
			minT = ts
		}
		if maxT.IsZero() || ts.After(maxT) {
			maxT = ts
		}
	}
	return minT, maxT
}

func (v *statsView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}

	palette := themePalette(theme)
	panel := styles.PanelStyle(palette, true)
	innerW := maxInt(0, width-(styles.LayoutInnerPadding*2)-2)
	innerH := maxInt(1, height-(styles.LayoutInnerPadding*2)-2)

	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb))
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	mapper := styles.NewAgentColorMapperWithPalette(palette.AgentPalette)

	rangeLabel := v.rangeLabel()
	head := titleStyle.Render(truncateVis("STATS  "+rangeLabel, innerW))

	if v.lastErr != nil {
		body := muted.Render("error: " + v.lastErr.Error())
		content := lipgloss.JoinVertical(lipgloss.Left, head, "", truncateVis(body, innerW))
		return panel.Width(width).Height(height).Render(content)
	}
	if v.loading {
		body := muted.Render("Loading...")
		content := lipgloss.JoinVertical(lipgloss.Left, head, "", truncateVis(body, innerW))
		return panel.Width(width).Height(height).Render(content)
	}

	leftW := innerW/2 - 1
	if leftW < 24 {
		leftW = innerW
	}
	rightW := innerW - leftW - 1
	if rightW < 0 {
		rightW = 0
	}
	divider := styles.DividerStyle(palette).Render("│")

	left := v.renderLeft(leftW, innerH, palette, mapper)
	if rightW == 0 {
		content := lipgloss.JoinVertical(lipgloss.Left, head, "", left)
		return panel.Width(width).Height(height).Render(content)
	}
	right := v.renderRight(rightW, innerH, palette, mapper)

	cols := lipgloss.JoinHorizontal(lipgloss.Top,
		lipgloss.NewStyle().Width(leftW).Height(innerH).Render(left),
		divider,
		lipgloss.NewStyle().Width(rightW).Height(innerH).Render(right),
	)

	footer := muted.Render(truncateVis("[/]: range  \u2190/\u2192: pan  r: refresh  Esc: back  (p: stats)", innerW))
	content := lipgloss.JoinVertical(lipgloss.Left, head, "", cols, "", footer)
	return panel.Width(width).Height(height).Render(content)
}

func (v *statsView) rangeLabel() string {
	if len(v.windows) == 0 || v.windowIdx < 0 || v.windowIdx >= len(v.windows) {
		return "range: ?"
	}
	d := v.windows[v.windowIdx]
	if d == 0 {
		return "all-time"
	}
	if d < 24*time.Hour {
		return fmt.Sprintf("last %s", formatDurationCompact(d))
	}
	days := int(d / (24 * time.Hour))
	if days%7 == 0 && days >= 7 {
		weeks := days / 7
		if weeks == 1 {
			return "last 7d"
		}
		return fmt.Sprintf("last %dw", weeks)
	}
	return fmt.Sprintf("last %dd", days)
}

func (v *statsView) renderLeft(width, height int, palette styles.Theme, mapper *styles.AgentColorMapper) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	bold := lipgloss.NewStyle().Bold(true)

	s := v.snap
	lines := make([]string, 0, height)

	lines = append(lines, bold.Render("OVERVIEW"))
	lines = append(lines, fmt.Sprintf("Total messages: %d", s.TotalMessages))
	lines = append(lines, fmt.Sprintf("Active agents:  %d", s.ActiveAgents))
	lines = append(lines, fmt.Sprintf("Active topics:  %d", s.ActiveTopics))
	if s.ReplySamples > 0 {
		lines = append(lines, fmt.Sprintf("Avg reply time: %s", formatDurationCompact(s.AvgReply)))
		lines = append(lines, fmt.Sprintf("Median reply:   %s", formatDurationCompact(s.MedianReply)))
	} else {
		lines = append(lines, "Avg reply time: -")
		lines = append(lines, "Median reply:   -")
	}
	if s.LongestThreadMessages > 0 {
		lines = append(lines, fmt.Sprintf("Longest thread: %d msgs", s.LongestThreadMessages))
	} else {
		lines = append(lines, "Longest thread: -")
	}

	if s.MostRepliedCount > 0 && strings.TrimSpace(s.MostRepliedID) != "" {
		parent := statsFindByID(v.all, s.MostRepliedID)
		parentLine := ""
		if parent != nil {
			parentLine = firstNonEmptyLine(messageBodyString(parent.Body))
		}
		if parentLine == "" {
			parentLine = s.MostRepliedID
		}
		lines = append(lines, truncateVis(fmt.Sprintf("Most replied:  %d (%s)", s.MostRepliedCount, parentLine), width))
	} else {
		lines = append(lines, "Most replied:  -")
	}

	lines = append(lines, "")

	lines = append(lines, bold.Render("TOP AGENTS (by msgs sent)"))
	if len(s.TopAgents) == 0 {
		lines = append(lines, muted.Render("No data"))
	} else {
		maxC := 0
		for _, a := range s.TopAgents {
			if a.Count > maxC {
				maxC = a.Count
			}
		}
		barW := maxInt(0, width-18)
		if barW > 24 {
			barW = 24
		}
		for i, a := range s.TopAgents {
			label := mapper.Foreground(a.Label).Render(mapper.Plain(a.Label))
			bar := renderBar(a.Count, maxC, barW, "█")
			line := fmt.Sprintf("%2d. %-10s %4d %s", i+1, label, a.Count, bar)
			lines = append(lines, truncateVis(line, width))
			if len(lines) >= height {
				break
			}
		}
	}

	lines = append(lines, "")
	lines = append(lines, bold.Render("BUSIEST / QUIETEST HOUR"))
	if !v.loadedStart.IsZero() && !v.loadedEnd.IsZero() {
		if !s.BusiestHourStart.IsZero() {
			lines = append(lines, fmt.Sprintf("Busiest:  %s (%d msgs)", hourRangeLabel(s.BusiestHourStart), s.BusiestHourCount))
		}
		if !s.QuietestHourStart.IsZero() {
			lines = append(lines, fmt.Sprintf("Quietest: %s (%d msgs)", hourRangeLabel(s.QuietestHourStart), s.QuietestHourCount))
		}
	} else {
		lines = append(lines, muted.Render("No data"))
	}

	lines = append(lines, "")
	lines = append(lines, bold.Render("THREAD DEPTH"))
	if s.ThreadAvgMessages > 0 {
		lines = append(lines, fmt.Sprintf("Avg msgs/thread: %.1f", s.ThreadAvgMessages))
		lines = append(lines, fmt.Sprintf("Standalone: %d", s.ThreadDist.Standalone))
		lines = append(lines, fmt.Sprintf("2-3 msgs:   %d", s.ThreadDist.Small))
		lines = append(lines, fmt.Sprintf("4-10 msgs:  %d", s.ThreadDist.Medium))
		lines = append(lines, fmt.Sprintf("10+ msgs:   %d", s.ThreadDist.Large))
	} else {
		lines = append(lines, muted.Render("No threads"))
	}

	if len(lines) > height {
		lines = lines[:height]
	}
	for i := range lines {
		lines[i] = truncateVis(lines[i], width)
	}
	return strings.Join(lines, "\n")
}

func (v *statsView) renderRight(width, height int, palette styles.Theme, mapper *styles.AgentColorMapper) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	bold := lipgloss.NewStyle().Bold(true)

	s := v.snap
	lines := make([]string, 0, height)

	lines = append(lines, bold.Render("MESSAGES OVER TIME"))
	if len(s.OverTimeCounts) == 0 {
		lines = append(lines, muted.Render("No data"))
	} else {
		spark := renderSpark(s.OverTimeCounts)
		lines = append(lines, truncateVis(spark, width))
		if !s.OverTimeStart.IsZero() && s.OverTimeInterval > 0 && v.windows[v.windowIdx] > 0 {
			start := v.loadedStart.UTC().Format("15:04")
			end := v.loadedEnd.UTC().Format("15:04")
			lines = append(lines, muted.Render(truncateVis(start+"  ...  "+end, width)))
		}
	}

	lines = append(lines, "")
	lines = append(lines, bold.Render("TOPIC VOLUME"))
	if len(s.TopicVolumes) == 0 {
		lines = append(lines, muted.Render("No data"))
	} else {
		maxC := 0
		for _, t := range s.TopicVolumes {
			if t.Count > maxC {
				maxC = t.Count
			}
		}
		barW := maxInt(0, width-18)
		if barW > 24 {
			barW = 24
		}
		for _, t := range s.TopicVolumes {
			label := mapper.Foreground(t.Label).Render(truncate(mapper.Plain(t.Label), 10))
			bar := renderBar(t.Count, maxC, barW, "█")
			line := fmt.Sprintf("%-10s %4d %s", label, t.Count, bar)
			lines = append(lines, truncateVis(line, width))
			if len(lines) >= height {
				break
			}
		}
	}

	lines = append(lines, "")
	lines = append(lines, bold.Render("RESPONSE LATENCY"))
	if len(s.ResponseLatency) == 0 || s.ReplySamples == 0 {
		lines = append(lines, muted.Render("No replies"))
	} else {
		maxC := 0
		for _, b := range s.ResponseLatency {
			if b.Count > maxC {
				maxC = b.Count
			}
		}
		barW := maxInt(0, width-18)
		if barW > 24 {
			barW = 24
		}
		for _, b := range s.ResponseLatency {
			style := latencyStyle(palette, b.Label)
			bar := style.Render(renderBar(b.Count, maxC, barW, "█"))
			line := fmt.Sprintf("%-7s %s %4.0f%%", b.Label, bar, b.Pct)
			lines = append(lines, truncateVis(line, width))
			if len(lines) >= height {
				break
			}
		}
	}

	if len(lines) > height {
		lines = lines[:height]
	}
	for i := range lines {
		lines[i] = truncateVis(lines[i], width)
	}
	return strings.Join(lines, "\n")
}

func latencyStyle(palette styles.Theme, label string) lipgloss.Style {
	switch label {
	case "<30s":
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Status.Online))
	case "30s-5m":
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.Normal))
	case "5m-30m":
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.Low))
	default:
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High))
	}
}

func renderBar(value, maxValue, width int, fill string) string {
	if width <= 0 {
		return ""
	}
	if maxValue <= 0 || value <= 0 {
		return strings.Repeat(" ", width)
	}
	n := int(float64(value) / float64(maxValue) * float64(width))
	if n < 0 {
		n = 0
	}
	if n > width {
		n = width
	}
	return strings.Repeat(fill, n) + strings.Repeat(" ", width-n)
}

func formatDurationCompact(d time.Duration) string {
	if d <= 0 {
		return "0s"
	}
	if d < time.Minute {
		return fmt.Sprintf("%ds", int(d.Seconds()))
	}
	if d < time.Hour {
		m := int(d / time.Minute)
		s := int((d % time.Minute) / time.Second)
		if s == 0 {
			return fmt.Sprintf("%dm", m)
		}
		return fmt.Sprintf("%dm%ds", m, s)
	}
	h := int(d / time.Hour)
	m := int((d % time.Hour) / time.Minute)
	if m == 0 {
		return fmt.Sprintf("%dh", h)
	}
	return fmt.Sprintf("%dh%dm", h, m)
}

func hourRangeLabel(start time.Time) string {
	if start.IsZero() {
		return "-"
	}
	end := start.Add(time.Hour)
	return start.Format("15:04") + "-" + end.Format("15:04")
}

func statsFindByID(messages []fmail.Message, id string) *fmail.Message {
	id = strings.TrimSpace(id)
	if id == "" {
		return nil
	}
	// Prefer binary search by ID (sortable).
	msgs := append([]fmail.Message(nil), messages...)
	sortMessages(msgs)
	i := sort.Search(len(msgs), func(i int) bool { return strings.TrimSpace(msgs[i].ID) >= id })
	if i >= 0 && i < len(msgs) && strings.TrimSpace(msgs[i].ID) == id {
		m := msgs[i]
		return &m
	}
	for i := range messages {
		if strings.TrimSpace(messages[i].ID) == id {
			m := messages[i]
			return &m
		}
	}
	return nil
}
