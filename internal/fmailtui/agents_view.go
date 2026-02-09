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

const agentsRefreshInterval = 5 * time.Second

type agentSortKey int

const (
	agentSortLastSeen agentSortKey = iota
	agentSortName
	agentSortMsgCount
	agentSortHost
)

type agentsMode int

const (
	agentsModeRoster agentsMode = iota
	agentsModeHistory
)

type agentsTickMsg struct{}

type agentsLoadedMsg struct {
	now    time.Time
	agents []fmail.AgentRecord
	err    error
}

type agentsIncomingMsg struct {
	msg fmail.Message
}

type agentsCountsLoadedMsg struct {
	counts map[string]int
	err    error
}

type agentsDetailLoadedMsg struct {
	agent string
	now   time.Time
	msgs  []data.SearchResult
	err   error
}

type agentRow struct {
	rec      fmail.AgentRecord
	msgCount int
}

type agentRecent struct {
	ts     time.Time
	target string
	body   string
	id     string
}

type agentDetail struct {
	name        string
	rec         fmail.AgentRecord
	msgCount24h int
	topTargets  []targetCount
	recent      []agentRecent
	spark       []int
	uptime      []bool // 48x 30m buckets
}

type targetCount struct {
	target string
	count  int
}

type agentsView struct {
	root     string
	provider data.MessageProvider

	now     time.Time
	lastErr error

	mode agentsMode

	sortKey agentSortKey
	filter  string
	editing bool

	records  []fmail.AgentRecord
	rows     []agentRow
	selected int

	counts    map[string]int
	countsTTL time.Time

	windowIdx int
	windows   []time.Duration

	detail        agentDetail
	detailAgent   string
	detailCached  map[string][]data.SearchResult
	detailUpdated map[string]time.Time

	historySelected int

	subCh     <-chan fmail.Message
	subCancel func()
}

func newAgentsView(root string, provider data.MessageProvider) *agentsView {
	return &agentsView{
		root:          root,
		provider:      provider,
		mode:          agentsModeRoster,
		sortKey:       agentSortLastSeen,
		counts:        make(map[string]int),
		windows:       []time.Duration{1 * time.Hour, 2 * time.Hour, 4 * time.Hour, 8 * time.Hour, 12 * time.Hour},
		windowIdx:     2, // 4h
		detailCached:  make(map[string][]data.SearchResult),
		detailUpdated: make(map[string]time.Time),
	}
}

func (v *agentsView) Close() {
	if v == nil {
		return
	}
	if v.subCancel != nil {
		v.subCancel()
		v.subCancel = nil
	}
	v.subCh = nil
}

func (v *agentsView) Init() tea.Cmd {
	v.startSubscription()
	return tea.Batch(v.loadCmd(), agentsTickCmd(), v.waitForMessageCmd())
}

func (v *agentsView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case agentsTickMsg:
		return tea.Batch(v.loadCmd(), agentsTickCmd())
	case agentsLoadedMsg:
		v.now = typed.now
		v.lastErr = typed.err
		if typed.err == nil {
			v.records = append([]fmail.AgentRecord(nil), typed.agents...)
			v.rebuildRows()
		}
		return v.ensureDetailCmd(false)
	case agentsCountsLoadedMsg:
		if typed.err != nil {
			v.lastErr = typed.err
			return nil
		}
		v.counts = typed.counts
		v.countsTTL = time.Now().UTC().Add(agentsRefreshInterval)
		v.rebuildRows()
		return nil
	case agentsDetailLoadedMsg:
		v.lastErr = typed.err
		if typed.err != nil {
			return nil
		}
		v.detailAgent = typed.agent
		v.detailUpdated[typed.agent] = typed.now
		v.detailCached[typed.agent] = append([]data.SearchResult(nil), typed.msgs...)
		v.recomputeDetail(typed.agent, typed.now, typed.msgs)
		return nil
	case agentsIncomingMsg:
		// Recompute caches lazily; just invalidate quick.
		if from := strings.TrimSpace(typed.msg.From); from != "" {
			delete(v.detailCached, from)
			delete(v.detailUpdated, from)
			v.countsTTL = time.Time{}
		}
		return tea.Batch(v.loadCmd(), v.waitForMessageCmd())
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *agentsView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	if v.now.IsZero() {
		v.now = time.Now().UTC()
	}
	palette := themePalette(theme)
	base := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Background(lipgloss.Color(palette.Base.Background))

	var body string
	if v.mode == agentsModeHistory {
		body = v.renderHistory(width, height, palette)
	} else {
		body = v.renderRoster(width, height, palette)
	}
	if v.lastErr != nil {
		errLine := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Render("data error: " + truncate(v.lastErr.Error(), maxInt(0, width-2)))
		body = lipgloss.JoinVertical(lipgloss.Left, body, errLine)
	}
	return base.Render(body)
}

func (v *agentsView) MinSize() (int, int) {
	return 48, 10
}

func (v *agentsView) handleKey(msg tea.KeyMsg) tea.Cmd {
	// History mode.
	if v.mode == agentsModeHistory {
		switch msg.String() {
		case "esc", "backspace":
			v.mode = agentsModeRoster
			v.historySelected = 0
			return nil
		case "j", "down":
			v.moveHistory(1)
			return nil
		case "k", "up":
			v.moveHistory(-1)
			return nil
		case "enter":
			target := v.historyTarget()
			if target == "" {
				return nil
			}
			return tea.Batch(openThreadCmd(target, ""), pushViewCmd(ViewThread))
		}
		return nil
	}

	// Filter editing.
	switch msg.Type {
	case tea.KeyEsc:
		if v.editing {
			v.editing = false
			return nil
		}
		return popViewCmd()
	case tea.KeyBackspace, tea.KeyDelete:
		if v.editing && len(v.filter) > 0 {
			runes := []rune(v.filter)
			v.filter = string(runes[:len(runes)-1])
			v.rebuildRows()
			return v.ensureDetailCmd(true)
		}
	case tea.KeyEnter:
		if v.editing {
			v.editing = false
			return nil
		}
	case tea.KeyRunes:
		if v.editing {
			v.filter += string(msg.Runes)
			v.rebuildRows()
			return v.ensureDetailCmd(true)
		}
	}

	switch msg.String() {
	case "/":
		v.editing = true
		return nil
	case "s":
		v.sortKey = nextAgentSortKey(v.sortKey)
		if v.sortKey == agentSortMsgCount {
			return v.ensureCountsCmd()
		}
		v.rebuildRows()
		return v.ensureDetailCmd(true)
	case "j", "down":
		v.moveSelection(1)
		return v.ensureDetailCmd(true)
	case "k", "up":
		v.moveSelection(-1)
		return v.ensureDetailCmd(true)
	case "[":
		v.windowIdx = maxInt(0, v.windowIdx-1)
		v.refreshDetailFromCache()
		return nil
	case "]":
		v.windowIdx = minInt(len(v.windows)-1, v.windowIdx+1)
		v.refreshDetailFromCache()
		return nil
	case "enter":
		if strings.TrimSpace(v.selectedAgent()) == "" {
			return nil
		}
		v.mode = agentsModeHistory
		v.historySelected = 0
		return nil
	}
	return nil
}

func (v *agentsView) loadCmd() tea.Cmd {
	if v.provider == nil {
		return func() tea.Msg {
			return agentsLoadedMsg{now: time.Now().UTC(), err: fmt.Errorf("missing provider")}
		}
	}
	return func() tea.Msg {
		now := time.Now().UTC()
		agents, err := v.provider.Agents()
		return agentsLoadedMsg{now: now, agents: agents, err: err}
	}
}

func agentsTickCmd() tea.Cmd {
	return tea.Tick(agentsRefreshInterval, func(time.Time) tea.Msg {
		return agentsTickMsg{}
	})
}

func (v *agentsView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.subCancel = cancel
}

func (v *agentsView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return agentsIncomingMsg{msg: msg}
	}
}

func (v *agentsView) ensureCountsCmd() tea.Cmd {
	if v.provider == nil {
		return nil
	}
	if !v.countsTTL.IsZero() && time.Now().UTC().Before(v.countsTTL) {
		return nil
	}
	return func() tea.Msg {
		now := time.Now().UTC()
		since := now.Add(-24 * time.Hour)
		results, err := v.provider.Search(data.SearchQuery{Since: since})
		if err != nil {
			return agentsCountsLoadedMsg{err: err}
		}
		counts := make(map[string]int)
		for _, res := range results {
			name := strings.TrimSpace(res.Message.From)
			if name == "" {
				continue
			}
			counts[name]++
		}
		return agentsCountsLoadedMsg{counts: counts}
	}
}

func (v *agentsView) ensureDetailCmd(force bool) tea.Cmd {
	if v.provider == nil {
		return nil
	}
	name := v.selectedAgent()
	if name == "" {
		return nil
	}
	if !force {
		if cached, ok := v.detailCached[name]; ok && len(cached) > 0 {
			v.recomputeDetail(name, v.now, cached)
			return nil
		}
	}

	return func() tea.Msg {
		now := time.Now().UTC()
		since := now.Add(-24 * time.Hour)
		msgs, err := v.provider.Search(data.SearchQuery{From: name, Since: since})
		return agentsDetailLoadedMsg{agent: name, now: now, msgs: msgs, err: err}
	}
}

func (v *agentsView) refreshDetailFromCache() {
	name := v.selectedAgent()
	if name == "" {
		return
	}
	cached, ok := v.detailCached[name]
	if !ok {
		return
	}
	v.recomputeDetail(name, v.now, cached)
}

func (v *agentsView) recomputeDetail(agent string, now time.Time, results []data.SearchResult) {
	rec := v.findRecord(agent)
	detail := agentDetail{
		name:   agent,
		rec:    rec,
		uptime: make([]bool, 48),
	}

	// Recent + per-target counts.
	targetCounts := make(map[string]int)
	recent := make([]agentRecent, 0, 16)
	for _, res := range results {
		msg := res.Message
		targetCounts[res.Topic]++
		recent = append(recent, agentRecent{
			ts:     msg.Time,
			target: res.Topic,
			body:   firstLine(msg.Body),
			id:     msg.ID,
		})

		if !msg.Time.IsZero() {
			diff := now.Sub(msg.Time)
			if diff < 0 {
				diff = -diff
			}
			if diff <= 24*time.Hour {
				idx := int(diff / (30 * time.Minute))
				if idx < 48 {
					detail.uptime[47-idx] = true
				}
			}
		}
	}

	sort.SliceStable(recent, func(i, j int) bool {
		if !recent[i].ts.Equal(recent[j].ts) {
			return recent[i].ts.After(recent[j].ts)
		}
		return recent[i].id > recent[j].id
	})
	if len(recent) > 10 {
		recent = recent[:10]
	}
	detail.recent = recent
	detail.msgCount24h = len(results)

	topTargets := make([]targetCount, 0, len(targetCounts))
	for target, count := range targetCounts {
		topTargets = append(topTargets, targetCount{target: target, count: count})
	}
	sort.SliceStable(topTargets, func(i, j int) bool {
		if topTargets[i].count != topTargets[j].count {
			return topTargets[i].count > topTargets[j].count
		}
		return topTargets[i].target < topTargets[j].target
	})
	if len(topTargets) > 8 {
		topTargets = topTargets[:8]
	}
	detail.topTargets = topTargets

	// Sparkline (last N hours, 15m buckets).
	window := v.windows[clampInt(v.windowIdx, 0, len(v.windows)-1)]
	buckets := int(window / (15 * time.Minute))
	if buckets < 1 {
		buckets = 1
	}
	spark := make([]int, buckets)
	for _, res := range results {
		ts := res.Message.Time
		if ts.IsZero() {
			continue
		}
		age := now.Sub(ts)
		if age < 0 || age > window {
			continue
		}
		idx := int((window - age) / (15 * time.Minute))
		if idx < 0 || idx >= len(spark) {
			continue
		}
		spark[idx]++
	}
	detail.spark = spark

	v.detail = detail
}

func (v *agentsView) findRecord(name string) fmail.AgentRecord {
	name = strings.TrimSpace(name)
	for _, rec := range v.records {
		if strings.EqualFold(strings.TrimSpace(rec.Name), name) {
			return rec
		}
	}
	return fmail.AgentRecord{Name: name}
}

func (v *agentsView) rebuildRows() {
	now := v.now
	if now.IsZero() {
		now = time.Now().UTC()
	}

	filter := strings.ToLower(strings.TrimSpace(v.filter))
	rows := make([]agentRow, 0, len(v.records))
	for _, rec := range v.records {
		name := strings.TrimSpace(rec.Name)
		if name == "" {
			continue
		}
		count := v.counts[name]
		row := agentRow{rec: rec, msgCount: count}
		if filter != "" && !agentMatchesFilter(row, filter) {
			continue
		}
		rows = append(rows, row)
	}

	sort.SliceStable(rows, func(i, j int) bool {
		a := rows[i]
		b := rows[j]
		switch v.sortKey {
		case agentSortName:
			la := strings.ToLower(strings.TrimSpace(a.rec.Name))
			lb := strings.ToLower(strings.TrimSpace(b.rec.Name))
			if la != lb {
				return la < lb
			}
		case agentSortMsgCount:
			if a.msgCount != b.msgCount {
				return a.msgCount > b.msgCount
			}
		case agentSortHost:
			ha := strings.ToLower(strings.TrimSpace(a.rec.Host))
			hb := strings.ToLower(strings.TrimSpace(b.rec.Host))
			if ha != hb {
				return ha < hb
			}
		default:
			if !a.rec.LastSeen.Equal(b.rec.LastSeen) {
				return a.rec.LastSeen.After(b.rec.LastSeen)
			}
		}
		return strings.ToLower(strings.TrimSpace(a.rec.Name)) < strings.ToLower(strings.TrimSpace(b.rec.Name))
	})

	v.rows = rows
	if len(v.rows) == 0 {
		v.selected = 0
		return
	}
	v.selected = clampInt(v.selected, 0, len(v.rows)-1)

	_ = now // for future presence calc
}

func agentMatchesFilter(row agentRow, filter string) bool {
	blob := strings.ToLower(strings.TrimSpace(row.rec.Name) + " " + strings.TrimSpace(row.rec.Host) + " " + strings.TrimSpace(row.rec.Status))
	return strings.Contains(blob, filter)
}

func (v *agentsView) moveSelection(delta int) {
	if len(v.rows) == 0 {
		v.selected = 0
		return
	}
	next := clampInt(v.selected+delta, 0, len(v.rows)-1)
	v.selected = next
}

func (v *agentsView) selectedAgent() string {
	if len(v.rows) == 0 || v.selected < 0 || v.selected >= len(v.rows) {
		return ""
	}
	return strings.TrimSpace(v.rows[v.selected].rec.Name)
}

func (v *agentsView) moveHistory(delta int) {
	results := v.detailCached[v.detailAgent]
	if len(results) == 0 {
		v.historySelected = 0
		return
	}
	v.historySelected = clampInt(v.historySelected+delta, 0, len(results)-1)
}

func (v *agentsView) historyTarget() string {
	results := v.detailCached[v.detailAgent]
	if len(results) == 0 || v.historySelected < 0 || v.historySelected >= len(results) {
		return ""
	}
	return strings.TrimSpace(results[v.historySelected].Topic)
}

func (v *agentsView) renderRoster(width, height int, palette styles.Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	panel := styles.PanelStyle(palette, true)
	innerW := maxInt(0, width-(styles.LayoutInnerPadding*2)-2)
	innerH := maxInt(1, height-(styles.LayoutInnerPadding*2)-2)

	title := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).
		Render(truncateVis(fmt.Sprintf("Agents  sort:%s  / filter  s sort  [ ] window  Enter history", agentSortLabel(v.sortKey)), innerW))
	filterSuffix := ""
	if v.editing {
		filterSuffix = "_"
	}
	filterLine := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).
		Render(truncateVis("Filter: "+v.filter+filterSuffix, innerW))

	header := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Base.Muted)).
		Render(truncateVis("AGENT           STATUS                 HOST              SEEN     MSGS", innerW))

	used := lipgloss.Height(title) + lipgloss.Height(filterLine) + lipgloss.Height(header) + 1
	listH := maxInt(5, innerH/2)
	detailH := maxInt(5, innerH-listH-used)

	list := v.renderAgentRows(innerW, listH, palette)
	detail := v.renderAgentDetail(innerW, detailH, palette)
	content := lipgloss.JoinVertical(lipgloss.Left, title, filterLine, header, list, styles.DividerStyle(palette).Render(strings.Repeat("─", innerW)), detail)
	return panel.Width(width).Height(height).Render(content)
}

func (v *agentsView) renderAgentRows(width, maxRows int, palette styles.Theme) string {
	if len(v.rows) == 0 {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("No agents")
	}
	v.selected = clampInt(v.selected, 0, len(v.rows)-1)
	start := maxInt(0, v.selected-maxRows/2)
	if start+maxRows > len(v.rows) {
		start = maxInt(0, len(v.rows)-maxRows)
	}

	mapper := styles.NewAgentColorMapperWithPalette(palette.AgentPalette)
	lines := make([]string, 0, maxRows)
	for idx := start; idx < len(v.rows) && len(lines) < maxRows; idx++ {
		row := v.rows[idx]
		rec := row.rec
		name := strings.TrimSpace(rec.Name)
		host := strings.TrimSpace(rec.Host)
		status := strings.TrimSpace(rec.Status)
		if status != "" {
			status = fmt.Sprintf("%q", status)
		}
		pres := agentPresenceIndicator(v.now, rec.LastSeen)
		seen := relativeTime(rec.LastSeen, v.now)
		if rec.LastSeen.IsZero() {
			seen = "-"
		}
		cursor := " "
		if idx == v.selected {
			cursor = "▸"
		}
		nameText := mapper.Foreground(name).Render(truncate(name, 14))
		line := fmt.Sprintf("%s%s %-14s %-22s %-17s %-8s %4d",
			cursor,
			pres,
			nameText,
			truncate(status, 22),
			truncate(host, 17),
			truncate(seen, 8),
			row.msgCount,
		)
		line = truncateVis(line, width)
		style := lipgloss.NewStyle()
		if idx == v.selected {
			style = style.Foreground(lipgloss.Color(palette.Chrome.SelectedItem)).Bold(true)
		}
		lines = append(lines, style.Render(line))
	}
	return strings.Join(lines, "\n")
}

func (v *agentsView) renderAgentDetail(width, height int, palette styles.Theme) string {
	if height <= 0 {
		return ""
	}
	name := strings.TrimSpace(v.selectedAgent())
	if name == "" {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Select an agent")
	}
	d := v.detail
	if !strings.EqualFold(strings.TrimSpace(d.name), name) {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Loading...")
	}

	lines := make([]string, 0, height)
	lines = append(lines, truncateVis(fmt.Sprintf("%s  host:%s  last:%s", name, d.rec.Host, relativeTime(d.rec.LastSeen, v.now)), width))
	if status := strings.TrimSpace(d.rec.Status); status != "" {
		lines = append(lines, truncateVis("status: "+status, width))
	}

	// Sparkline.
	lines = append(lines, "")
	lines = append(lines, truncateVis(fmt.Sprintf("Activity (%s): %s", v.windows[v.windowIdx].String(), renderSpark(d.spark)), width))

	// Uptime bar.
	lines = append(lines, truncateVis("Uptime (24h): "+renderUptime(d.uptime, palette), width))

	// Top targets.
	if len(d.topTargets) > 0 {
		parts := make([]string, 0, len(d.topTargets))
		for _, t := range d.topTargets {
			parts = append(parts, fmt.Sprintf("%s (%d)", t.target, t.count))
		}
		lines = append(lines, truncateVis("Active: "+strings.Join(parts, ", "), width))
	}

	// Recent messages.
	if len(d.recent) > 0 {
		lines = append(lines, "")
		lines = append(lines, "Recent:")
		for _, msg := range d.recent {
			line := fmt.Sprintf("%s -> %s: %s", msg.ts.UTC().Format("15:04"), msg.target, msg.body)
			lines = append(lines, truncateVis(line, width))
			if len(lines) >= height {
				break
			}
		}
	}
	if len(lines) > height {
		lines = lines[:height]
	}
	return strings.Join(lines, "\n")
}

func (v *agentsView) renderHistory(width, height int, palette styles.Theme) string {
	panel := styles.PanelStyle(palette, true)
	innerW := maxInt(0, width-(styles.LayoutInnerPadding*2)-2)
	innerH := maxInt(1, height-(styles.LayoutInnerPadding*2)-2)

	agent := v.selectedAgent()
	if v.detailAgent != agent {
		v.detailAgent = agent
	}
	results := v.detailCached[agent]
	title := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).
		Render(truncateVis("History: "+agent+"  (Enter open thread, Esc back)", innerW))

	if len(results) == 0 {
		content := lipgloss.JoinVertical(lipgloss.Left, title, lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("No messages"))
		return panel.Width(width).Height(height).Render(content)
	}

	sort.SliceStable(results, func(i, j int) bool {
		if results[i].Message.ID != results[j].Message.ID {
			return results[i].Message.ID > results[j].Message.ID
		}
		return results[i].Topic > results[j].Topic
	})

	v.historySelected = clampInt(v.historySelected, 0, len(results)-1)
	maxRows := innerH - lipgloss.Height(title) - 1
	if maxRows < 1 {
		maxRows = 1
	}
	start := maxInt(0, v.historySelected-maxRows/2)
	if start+maxRows > len(results) {
		start = maxInt(0, len(results)-maxRows)
	}

	lines := make([]string, 0, maxRows)
	for i := start; i < len(results) && len(lines) < maxRows; i++ {
		res := results[i]
		cursor := " "
		if i == v.historySelected {
			cursor = "▸"
		}
		line := fmt.Sprintf("%s %s -> %s: %s", cursor, res.Message.Time.UTC().Format("15:04"), res.Topic, firstLine(res.Message.Body))
		line = truncateVis(line, innerW)
		style := lipgloss.NewStyle()
		if i == v.historySelected {
			style = style.Foreground(lipgloss.Color(palette.Chrome.SelectedItem)).Bold(true)
		}
		lines = append(lines, style.Render(line))
	}

	content := lipgloss.JoinVertical(lipgloss.Left, title, strings.Join(lines, "\n"))
	return panel.Width(width).Height(height).Render(content)
}

func nextAgentSortKey(key agentSortKey) agentSortKey {
	switch key {
	case agentSortName:
		return agentSortMsgCount
	case agentSortMsgCount:
		return agentSortHost
	case agentSortHost:
		return agentSortLastSeen
	default:
		return agentSortName
	}
}

func agentSortLabel(key agentSortKey) string {
	switch key {
	case agentSortName:
		return "name"
	case agentSortMsgCount:
		return "count(24h)"
	case agentSortHost:
		return "host"
	default:
		return "seen"
	}
}

func agentPresenceIndicator(now, lastSeen time.Time) string {
	if lastSeen.IsZero() {
		return "✕"
	}
	diff := now.Sub(lastSeen)
	switch {
	case diff <= time.Minute:
		return "●"
	case diff <= 10*time.Minute:
		return "○"
	case diff <= time.Hour:
		return "◌"
	default:
		return "✕"
	}
}

func renderSpark(values []int) string {
	if len(values) == 0 {
		return ""
	}
	maxV := 0
	for _, v := range values {
		if v > maxV {
			maxV = v
		}
	}
	levels := []rune("▁▂▃▄▅▆▇█")
	out := make([]rune, 0, len(values))
	for _, v := range values {
		if maxV <= 0 || v <= 0 {
			out = append(out, levels[0])
			continue
		}
		idx := int(float64(v) / float64(maxV) * float64(len(levels)-1))
		if idx < 0 {
			idx = 0
		}
		if idx >= len(levels) {
			idx = len(levels) - 1
		}
		out = append(out, levels[idx])
	}
	return string(out)
}

func renderUptime(buckets []bool, palette styles.Theme) string {
	if len(buckets) == 0 {
		return ""
	}
	active := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Status.Online)).Render("█")
	idle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Status.Stale)).Render("░")
	var b strings.Builder
	for _, on := range buckets {
		if on {
			b.WriteString(active)
		} else {
			b.WriteString(idle)
		}
	}
	return b.String()
}
