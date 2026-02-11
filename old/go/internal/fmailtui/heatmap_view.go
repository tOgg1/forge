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

type heatmapWindow struct {
	label  string
	window time.Duration
	bucket time.Duration
}

type heatmapLoadedMsg struct {
	now      time.Time
	start    time.Time
	end      time.Time
	bucket   time.Duration
	messages []fmail.Message
	err      error
}

type heatmapIncomingMsg struct {
	msg fmail.Message
}

type heatmapView struct {
	root     string
	self     string
	provider data.MessageProvider

	windows   []heatmapWindow
	windowIdx int
	windowEnd time.Time

	mode heatmapMode
	sort heatmapSort

	loading bool
	lastErr error
	now     time.Time

	start  time.Time
	end    time.Time
	bucket time.Duration

	all  []fmail.Message
	seen map[string]struct{}

	matrix heatmapMatrix

	selectedRow int
	selectedCol int
	top         int
	gridH       int

	subCh     <-chan fmail.Message
	subCancel func()
}

func newHeatmapView(root, self string, provider data.MessageProvider) *heatmapView {
	self = strings.TrimSpace(self)
	if self == "" {
		self = defaultSelfAgent
	}
	return &heatmapView{
		root:     root,
		self:     self,
		provider: provider,
		windows: []heatmapWindow{
			{label: "4h", window: 4 * time.Hour, bucket: 10 * time.Minute},
			{label: "12h", window: 12 * time.Hour, bucket: 30 * time.Minute},
			{label: "24h", window: 24 * time.Hour, bucket: 1 * time.Hour},
			{label: "7d", window: 7 * 24 * time.Hour, bucket: 4 * time.Hour},
			{label: "30d", window: 30 * 24 * time.Hour, bucket: 24 * time.Hour},
		},
		windowIdx: 2,
		mode:      heatmapModeAgents,
		sort:      heatmapSortTotal,
		seen:      make(map[string]struct{}, 2048),
	}
}

func (v *heatmapView) Init() tea.Cmd {
	v.startSubscription()
	v.loading = true
	return tea.Batch(v.loadCmd(), v.waitForMessageCmd())
}

func (v *heatmapView) Close() {
	if v.subCancel != nil {
		v.subCancel()
		v.subCancel = nil
	}
	v.subCh = nil
}

func (v *heatmapView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.subCancel = cancel
}

func (v *heatmapView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return heatmapIncomingMsg{msg: msg}
	}
}

func (v *heatmapView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case heatmapLoadedMsg:
		v.applyLoaded(typed)
		return nil
	case heatmapIncomingMsg:
		v.applyIncoming(typed.msg)
		return v.waitForMessageCmd()
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *heatmapView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "t":
		if v.mode == heatmapModeAgents {
			v.mode = heatmapModeTopics
		} else {
			v.mode = heatmapModeAgents
		}
		v.rebuildMatrix()
		return nil
	case "s":
		v.sort = (v.sort + 1) % 4
		v.matrix.sortRows(v.sort)
		v.restoreSelection()
		return nil
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
	case "h":
		v.windowEnd = v.windowEnd.Add(-v.panStep())
		v.loading = true
		return v.loadCmd()
	case "l":
		v.windowEnd = v.windowEnd.Add(v.panStep())
		v.loading = true
		return v.loadCmd()
	case "up", "k":
		v.moveSelection(-1, 0)
		return nil
	case "down", "j":
		v.moveSelection(1, 0)
		return nil
	case "enter":
		return v.openTimelineForSelectionCmd()
	}
	if msg.String() == "ctrl+u" || msg.String() == "pgup" {
		v.moveSelection(-maxInt(1, v.visibleRowCount()), 0)
		return nil
	}
	if msg.String() == "ctrl+d" || msg.String() == "pgdown" {
		v.moveSelection(maxInt(1, v.visibleRowCount()), 0)
		return nil
	}
	switch msg.String() {
	case "left":
		if v.selectedCol <= 0 {
			v.windowEnd = v.windowEnd.Add(-v.panStep())
			v.loading = true
			return v.loadCmd()
		}
		v.moveSelection(0, -1)
		return nil
	case "right":
		if v.selectedCol >= maxInt(0, v.matrix.Cols-1) {
			v.windowEnd = v.windowEnd.Add(v.panStep())
			v.loading = true
			return v.loadCmd()
		}
		v.moveSelection(0, 1)
		return nil
	}
	return nil
}

func (v *heatmapView) visibleRowCount() int {
	return maxInt(1, v.visibleRowsForHeight(v.gridH)-1)
}

func (v *heatmapView) moveSelection(rowDelta, colDelta int) {
	if len(v.matrix.Rows) == 0 || v.matrix.Cols <= 0 {
		return
	}
	v.selectedRow = clampInt(v.selectedRow+rowDelta, 0, len(v.matrix.Rows)-1)
	v.selectedCol = clampInt(v.selectedCol+colDelta, 0, v.matrix.Cols-1)
	v.ensureRowVisible()
}

func (v *heatmapView) ensureRowVisible() {
	if v.selectedRow < v.top {
		v.top = v.selectedRow
		return
	}
	visible := v.visibleRowsForHeight(v.gridH)
	if visible <= 0 {
		return
	}
	if v.selectedRow >= v.top+visible {
		v.top = v.selectedRow - visible + 1
		if v.top < 0 {
			v.top = 0
		}
	}
}

func (v *heatmapView) panStep() time.Duration {
	if len(v.windows) == 0 {
		return time.Hour
	}
	w := v.windows[clampInt(v.windowIdx, 0, len(v.windows)-1)].window
	step := w / 6
	if step < time.Hour {
		step = time.Hour
	}
	return step
}

func (v *heatmapView) windowBounds(now time.Time) (time.Time, time.Time, time.Duration) {
	if len(v.windows) == 0 {
		return time.Time{}, time.Time{}, 0
	}
	cfg := v.windows[clampInt(v.windowIdx, 0, len(v.windows)-1)]
	end := v.windowEnd
	if end.IsZero() {
		end = now
	}
	start := end.Add(-cfg.window)
	return start, end, cfg.bucket
}

func (v *heatmapView) loadCmd() tea.Cmd {
	provider := v.provider
	self := v.self
	windowIdx := v.windowIdx
	windowEnd := v.windowEnd
	windows := append([]heatmapWindow(nil), v.windows...)

	return func() tea.Msg {
		now := time.Now().UTC()
		if provider == nil || len(windows) == 0 {
			return heatmapLoadedMsg{now: now}
		}
		cfg := windows[clampInt(windowIdx, 0, len(windows)-1)]
		end := windowEnd
		if end.IsZero() {
			end = now
		}
		start := end.Add(-cfg.window)
		filter := data.MessageFilter{Since: start, Until: end}

		merged := make([]fmail.Message, 0, 1024)
		seen := make(map[string]struct{}, 1024)

		topics, err := provider.Topics()
		if err != nil {
			return heatmapLoadedMsg{now: now, err: err}
		}
		for i := range topics {
			topic := strings.TrimSpace(topics[i].Name)
			if topic == "" {
				continue
			}
			msgs, err := provider.Messages(topic, filter)
			if err != nil {
				return heatmapLoadedMsg{now: now, err: err}
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
					return heatmapLoadedMsg{now: now, err: dmErr}
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
		return heatmapLoadedMsg{
			now:      now,
			start:    start,
			end:      end,
			bucket:   cfg.bucket,
			messages: merged,
		}
	}
}

func (v *heatmapView) applyLoaded(msg heatmapLoadedMsg) {
	v.loading = false
	v.now = msg.now
	v.lastErr = msg.err
	if msg.err != nil {
		return
	}
	v.start = msg.start
	v.end = msg.end
	v.bucket = msg.bucket
	v.all = append(v.all[:0], msg.messages...)
	v.seen = make(map[string]struct{}, len(v.all))
	for i := range v.all {
		v.seen[statsDedupKey(v.all[i])] = struct{}{}
	}
	v.rebuildMatrix()
}

func (v *heatmapView) rebuildMatrix() {
	v.matrix = buildHeatmapMatrix(v.all, v.start, v.end, v.bucket, v.mode)
	v.matrix.sortRows(v.sort)
	v.restoreSelection()
}

func (v *heatmapView) restoreSelection() {
	if len(v.matrix.Rows) == 0 {
		v.selectedRow = 0
		v.selectedCol = 0
		v.top = 0
		return
	}
	v.selectedRow = clampInt(v.selectedRow, 0, len(v.matrix.Rows)-1)
	v.selectedCol = clampInt(v.selectedCol, 0, maxInt(0, v.matrix.Cols-1))
	if v.top > v.selectedRow {
		v.top = v.selectedRow
	}
}

func (v *heatmapView) applyIncoming(msg fmail.Message) {
	now := time.Now().UTC()
	v.now = now
	key := statsDedupKey(msg)
	if _, ok := v.seen[key]; ok {
		return
	}
	v.seen[key] = struct{}{}

	// Only update live when following the tail.
	if v.windowEnd.IsZero() || now.Sub(v.windowEnd) <= 2*time.Second {
		v.all = append(v.all, msg)
		sortMessages(v.all)
		// Keep window anchored to now.
		v.windowEnd = now
		v.start, v.end, v.bucket = v.windowBounds(now)
		v.rebuildMatrix()
	}
}

func (v *heatmapView) openTimelineForSelectionCmd() tea.Cmd {
	if len(v.matrix.Rows) == 0 || v.matrix.Cols <= 0 {
		return nil
	}
	row := v.matrix.Rows[clampInt(v.selectedRow, 0, len(v.matrix.Rows)-1)]
	col := clampInt(v.selectedCol, 0, v.matrix.Cols-1)
	cellStart := v.start.Add(time.Duration(col) * v.bucket)
	cellEnd := cellStart.Add(v.bucket)
	filter := ""
	if v.mode == heatmapModeTopics {
		filter = "to:" + strings.TrimSpace(row.Label)
	} else {
		filter = "from:" + strings.TrimSpace(row.Label)
	}
	return tea.Batch(pushViewCmd(ViewTimeline), applyTimelineCmd(filter, cellEnd, v.bucket))
}

func (v *heatmapView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	palette := themePalette(theme)
	panel := styles.PanelStyle(palette, true)
	innerW := maxInt(0, width-(styles.LayoutInnerPadding*2)-2)
	innerH := maxInt(1, height-(styles.LayoutInnerPadding*2)-2)

	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb))
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))

	header := titleStyle.Render(truncateVis(fmt.Sprintf("ACTIVITY HEATMAP  last %s  mode:%s  sort:%s",
		v.windows[clampInt(v.windowIdx, 0, len(v.windows)-1)].label,
		heatmapModeLabel(v.mode),
		heatmapSortLabel(v.sort),
	), innerW))

	if v.lastErr != nil {
		content := lipgloss.JoinVertical(lipgloss.Left, header, "", muted.Render("error: "+truncate(v.lastErr.Error(), innerW)))
		return panel.Width(width).Height(height).Render(content)
	}
	if v.loading {
		content := lipgloss.JoinVertical(lipgloss.Left, header, "", muted.Render("Loading..."))
		return panel.Width(width).Height(height).Render(content)
	}

	gridH := maxInt(0, innerH-8)
	if gridH < 4 {
		gridH = innerH
	}
	v.gridH = gridH

	grid := v.renderGrid(innerW, gridH, palette)
	tooltip := v.renderTooltip(innerW, palette)
	legend := muted.Render("Legend: ░  low   ▒  mid   ▓  high   █  max")
	summary := v.renderSummary(innerW, palette)
	footer := muted.Render(truncateVis("[/]: range  t: toggle  s: sort  Enter: timeline  Esc: back  (H: heatmap)", innerW))

	lines := []string{header, "", grid, tooltip, legend, summary, "", footer}
	content := lipgloss.JoinVertical(lipgloss.Left, lines...)
	return panel.Width(width).Height(height).Render(content)
}

func (v *heatmapView) renderTooltip(width int, palette styles.Theme) string {
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	if len(v.matrix.Rows) == 0 || v.matrix.Cols <= 0 || v.bucket <= 0 || v.start.IsZero() {
		return muted.Render("")
	}
	r := v.matrix.Rows[clampInt(v.selectedRow, 0, len(v.matrix.Rows)-1)]
	c := clampInt(v.selectedCol, 0, v.matrix.Cols-1)
	cellStart := v.start.Add(time.Duration(c) * v.bucket)
	cellEnd := cellStart.Add(v.bucket)
	count := 0
	if c < len(r.Counts) {
		count = r.Counts[c]
	}
	detail := v.cellBreakdown(r.Label, cellStart, cellEnd)
	line := fmt.Sprintf("%s, %s: %d msgs", strings.TrimSpace(r.Label), timeRangeLabel(cellStart.UTC(), cellEnd.UTC()), count)
	if strings.TrimSpace(detail) != "" {
		line += " (" + detail + ")"
	}
	return muted.Render(truncateVis(line, width))
}

func timeRangeLabel(start, end time.Time) string {
	if start.IsZero() || end.IsZero() || !end.After(start) {
		return "-"
	}
	// Keep it short.
	if end.Sub(start) >= 24*time.Hour {
		return start.Format("2006-01-02")
	}
	return start.Format("15:04") + "-" + end.Format("15:04")
}

func (v *heatmapView) cellBreakdown(rowLabel string, start, end time.Time) string {
	rowLabel = strings.TrimSpace(rowLabel)
	if rowLabel == "" || start.IsZero() || end.IsZero() || !end.After(start) {
		return ""
	}
	type kv struct {
		k string
		v int
	}

	dmCount := 0
	counts := make(map[string]int, 8)
	for i := range v.all {
		msg := v.all[i]
		if msg.Time.IsZero() || msg.Time.Before(start) || !msg.Time.Before(end) {
			continue
		}
		switch v.mode {
		case heatmapModeTopics:
			if strings.TrimSpace(msg.To) != rowLabel {
				continue
			}
			from := strings.TrimSpace(msg.From)
			if from != "" {
				counts[from]++
			}
		default:
			if strings.TrimSpace(msg.From) != rowLabel {
				continue
			}
			to := strings.TrimSpace(msg.To)
			if strings.HasPrefix(to, "@") {
				dmCount++
				continue
			}
			if to != "" {
				counts[to]++
			}
		}
	}

	top := make([]kv, 0, len(counts))
	for k, v := range counts {
		top = append(top, kv{k: k, v: v})
	}
	sort.SliceStable(top, func(i, j int) bool {
		if top[i].v != top[j].v {
			return top[i].v > top[j].v
		}
		return top[i].k < top[j].k
	})

	parts := make([]string, 0, 4)
	limit := 3
	if v.mode == heatmapModeAgents {
		limit = 2
	}
	for i := 0; i < len(top) && i < limit; i++ {
		parts = append(parts, fmt.Sprintf("%s: %d", top[i].k, top[i].v))
	}
	if dmCount > 0 && v.mode == heatmapModeAgents {
		parts = append(parts, fmt.Sprintf("DMs: %d", dmCount))
	}
	return strings.Join(parts, ", ")
}

func heatmapModeLabel(mode heatmapMode) string {
	if mode == heatmapModeTopics {
		return "topics"
	}
	return "agents"
}

func heatmapSortLabel(mode heatmapSort) string {
	switch mode {
	case heatmapSortName:
		return "name"
	case heatmapSortPeak:
		return "peak"
	case heatmapSortRecency:
		return "recent"
	default:
		return "total"
	}
}

func (v *heatmapView) visibleRowsForHeight(height int) int {
	if height <= 0 {
		return 0
	}
	// header row + axis row.
	usable := height - 2
	if usable < 1 {
		return 1
	}
	return usable
}

func (v *heatmapView) renderGrid(width, height int, palette styles.Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	nameW := 8
	cellW := 3
	cols := v.matrix.Cols
	if cols <= 0 {
		return ""
	}

	gridW := cols * cellW
	if nameW+1+gridW > width {
		// Too narrow; show as much as possible.
		maxCols := maxInt(1, (width-(nameW+1))/cellW)
		if maxCols < cols {
			cols = maxCols
			gridW = cols * cellW
		}
	}

	axis := v.renderAxis(nameW, cols, cellW, palette)
	rows := v.renderRows(nameW, cols, cellW, height-1, palette)
	return lipgloss.JoinVertical(lipgloss.Left, axis, rows)
}

func (v *heatmapView) renderAxis(nameW, cols, cellW int, palette styles.Theme) string {
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	var b strings.Builder
	b.WriteString(strings.Repeat(" ", nameW))
	b.WriteString(" ")
	for c := 0; c < cols; c++ {
		ts := v.start.Add(time.Duration(c) * v.bucket).UTC()
		label := ""
		if v.bucket >= 24*time.Hour {
			label = ts.Format("02")
		} else {
			label = ts.Format("15")
		}
		if c%2 == 0 {
			b.WriteString(muted.Render(padCenter(label, cellW)))
		} else {
			b.WriteString(strings.Repeat(" ", cellW))
		}
	}
	return b.String()
}

func (v *heatmapView) renderRows(nameW, cols, cellW, height int, palette styles.Theme) string {
	if height <= 0 {
		height = 1
	}
	rows := v.matrix.Rows
	if len(rows) == 0 {
		return ""
	}
	startRow := clampInt(v.top, 0, maxInt(0, len(rows)-1))
	visible := minInt(height, len(rows)-startRow)
	if visible < 1 {
		visible = 1
	}
	rows = rows[startRow : startRow+visible]

	lines := make([]string, 0, visible)
	for i, row := range rows {
		rowIdx := startRow + i
		name := truncate(row.Label, nameW)
		nameStyle := lipgloss.NewStyle()
		if rowIdx == v.selectedRow {
			nameStyle = nameStyle.Bold(true).Foreground(lipgloss.Color(palette.Chrome.SelectedItem))
		} else {
			nameStyle = nameStyle.Foreground(lipgloss.Color(palette.Base.Muted))
		}

		var b strings.Builder
		b.WriteString(padRight(nameStyle.Render(name), nameW))
		b.WriteString(" ")
		for c := 0; c < cols; c++ {
			count := 0
			if c < len(row.Counts) {
				count = row.Counts[c]
			}
			cell := heatmapCell(count, v.matrix.Threshold, palette)
			if rowIdx == v.selectedRow && c == v.selectedCol {
				cell = lipgloss.NewStyle().Reverse(true).Render(cell)
			}
			b.WriteString(cell)
		}
		lines = append(lines, b.String())
	}
	return strings.Join(lines, "\n")
}

func heatmapCell(count int, th [3]int, palette styles.Theme) string {
	if count <= 0 {
		return "   "
	}
	char := "░"
	color := palette.Status.Recent
	switch {
	case count <= th[0]:
		char = "░"
		color = palette.Base.Accent
	case count <= th[1]:
		char = "▒"
		color = palette.Status.Recent
	case count <= th[2]:
		char = "▓"
		color = palette.Priority.Normal
	default:
		char = "█"
		color = palette.Priority.High
	}
	style := lipgloss.NewStyle().Foreground(lipgloss.Color(color))
	return style.Render(strings.Repeat(char, 3))
}

func (v *heatmapView) renderSummary(width int, palette styles.Theme) string {
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	if len(v.all) == 0 || v.start.IsZero() || v.end.IsZero() {
		return muted.Render("Summary: no data")
	}

	total := 0
	agents := make(map[string]int, 16)
	topics := make(map[string]int, 16)
	byID := make(map[string]fmail.Message, len(v.all))
	replyFirst := make(map[string]time.Duration, 64)

	for i := range v.all {
		msg := v.all[i]
		if msg.Time.IsZero() || msg.Time.Before(v.start) || !msg.Time.Before(v.end) {
			continue
		}
		total++
		if from := strings.TrimSpace(msg.From); from != "" {
			agents[from]++
		}
		if to := strings.TrimSpace(msg.To); to != "" {
			topics[to]++
		}
		if id := strings.TrimSpace(msg.ID); id != "" {
			byID[id] = msg
		}
	}

	for i := range v.all {
		msg := v.all[i]
		parentID := strings.TrimSpace(msg.ReplyTo)
		if parentID == "" {
			continue
		}
		parent, ok := byID[parentID]
		if !ok || parent.Time.IsZero() || msg.Time.IsZero() {
			continue
		}
		delta := msg.Time.Sub(parent.Time)
		if delta < 0 {
			continue
		}
		prev, ok := replyFirst[parentID]
		if !ok || delta < prev {
			replyFirst[parentID] = delta
		}
	}
	var avgFirst time.Duration
	if len(replyFirst) > 0 {
		var sum time.Duration
		for _, d := range replyFirst {
			sum += d
		}
		avgFirst = sum / time.Duration(len(replyFirst))
	}

	peakStart, peakCount := v.peakBucket()
	mostAgent, mostAgentCount := maxCount(agents)
	mostTopic, mostTopicCount := maxCount(topics)

	parts := []string{
		fmt.Sprintf("Summary: total %d msgs, %d active agents", total, len(agents)),
		fmt.Sprintf("Peak: %s (%d msgs)", hourRangeLabel(peakStart), peakCount),
		fmt.Sprintf("Most active: %s (%d)", mostAgent, mostAgentCount),
		fmt.Sprintf("Busiest topic: %s (%d)", mostTopic, mostTopicCount),
	}
	if avgFirst > 0 {
		parts = append(parts, fmt.Sprintf("Avg response: %s", formatDurationCompact(avgFirst)))
	}
	return muted.Render(truncateVis(strings.Join(parts, "  |  "), width))
}

func (v *heatmapView) peakBucket() (time.Time, int) {
	if v.matrix.Cols <= 0 || v.bucket <= 0 {
		return time.Time{}, 0
	}
	counts := make([]int, v.matrix.Cols)
	for _, row := range v.matrix.Rows {
		for i := 0; i < len(row.Counts) && i < len(counts); i++ {
			counts[i] += row.Counts[i]
		}
	}
	bestIdx := 0
	best := 0
	for i, c := range counts {
		if c > best {
			best = c
			bestIdx = i
		}
	}
	return v.start.Add(time.Duration(bestIdx) * v.bucket).UTC(), best
}

func maxCount(m map[string]int) (string, int) {
	bestK := ""
	bestV := 0
	for k, v := range m {
		if v > bestV || (v == bestV && k < bestK) {
			bestK = k
			bestV = v
		}
	}
	return bestK, bestV
}

func padRight(s string, width int) string {
	if width <= 0 {
		return s
	}
	plain := lipgloss.Width(s)
	if plain >= width {
		return s
	}
	return s + strings.Repeat(" ", width-plain)
}

func padCenter(s string, width int) string {
	if width <= 0 {
		return s
	}
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	left := (width - w) / 2
	right := width - w - left
	return strings.Repeat(" ", left) + s + strings.Repeat(" ", right)
}
