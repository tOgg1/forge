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

type graphWindow struct {
	label  string
	window time.Duration // 0 = all
}

type graphLoadedMsg struct {
	now      time.Time
	start    time.Time
	end      time.Time
	allTime  bool
	messages []fmail.Message
	err      error
}

type graphIncomingMsg struct {
	msg fmail.Message
}

type graphSelectionMode int

const (
	graphSelectNodes graphSelectionMode = iota
	graphSelectEdges
)

type graphView struct {
	root     string
	self     string
	provider data.MessageProvider

	windows   []graphWindow
	windowIdx int
	windowEnd time.Time

	loading bool
	lastErr error
	now     time.Time

	start time.Time
	end   time.Time

	all  []fmail.Message
	seen map[string]struct{}

	zoom       float64
	panX       int
	panY       int
	clustersOn bool

	topicOverlay bool

	graph graphData

	selectMode graphSelectionMode
	selected   int // node index or edge index
	detailOpen bool

	subCh     <-chan fmail.Message
	subCancel func()
}

func newGraphView(root, self string, provider data.MessageProvider) *graphView {
	self = strings.TrimSpace(self)
	if self == "" {
		self = defaultSelfAgent
	}
	return &graphView{
		root:     root,
		self:     self,
		provider: provider,
		windows: []graphWindow{
			{label: "1h", window: 1 * time.Hour},
			{label: "4h", window: 4 * time.Hour},
			{label: "12h", window: 12 * time.Hour},
			{label: "24h", window: 24 * time.Hour},
			{label: "7d", window: 7 * 24 * time.Hour},
			{label: "all", window: 0},
		},
		windowIdx: 1,
		zoom:      1.0,
		seen:      make(map[string]struct{}, 2048),
	}
}

func (v *graphView) Init() tea.Cmd {
	v.startSubscription()
	v.loading = true
	return tea.Batch(v.loadCmd(), v.waitForMessageCmd())
}

func (v *graphView) Close() {
	if v.subCancel != nil {
		v.subCancel()
		v.subCancel = nil
	}
	v.subCh = nil
}

func (v *graphView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.subCancel = cancel
}

func (v *graphView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return graphIncomingMsg{msg: msg}
	}
}

func (v *graphView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case graphLoadedMsg:
		v.applyLoaded(typed)
		return nil
	case graphIncomingMsg:
		v.applyIncoming(typed.msg)
		return v.waitForMessageCmd()
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *graphView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
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
	case "+", "=":
		v.zoom = minFloat(1.8, v.zoom+0.1)
		v.rebuildGraph()
		return nil
	case "-":
		v.zoom = maxFloat(0.6, v.zoom-0.1)
		v.rebuildGraph()
		return nil
	case "tab":
		if v.selectMode == graphSelectNodes {
			v.selectMode = graphSelectEdges
		} else {
			v.selectMode = graphSelectNodes
		}
		v.selected = 0
		v.detailOpen = false
		return nil
	case "c":
		v.clustersOn = !v.clustersOn
		return nil
	case "t":
		v.topicOverlay = !v.topicOverlay
		v.rebuildGraph()
		return nil
	case "enter":
		v.detailOpen = !v.detailOpen
		return nil
	case "up", "k":
		v.moveSelection(-1)
		return nil
	case "down", "j":
		v.moveSelection(1)
		return nil
	case "left", "h":
		v.panX--
		return nil
	case "right", "l":
		v.panX++
		return nil
	}
	return nil
}

func (v *graphView) moveSelection(delta int) {
	if delta == 0 {
		return
	}
	if v.selectMode == graphSelectEdges {
		if len(v.graph.Edges) == 0 {
			return
		}
		v.selected = clampInt(v.selected+delta, 0, len(v.graph.Edges)-1)
		return
	}
	if len(v.graph.Nodes) == 0 {
		return
	}
	v.selected = clampInt(v.selected+delta, 0, len(v.graph.Nodes)-1)
}

func (v *graphView) windowBounds(now time.Time) (time.Time, time.Time, bool) {
	cfg := v.windows[clampInt(v.windowIdx, 0, len(v.windows)-1)]
	if cfg.window == 0 {
		return time.Time{}, time.Time{}, true
	}
	end := v.windowEnd
	if end.IsZero() {
		end = now
	}
	start := end.Add(-cfg.window)
	return start, end, false
}

func (v *graphView) loadCmd() tea.Cmd {
	provider := v.provider
	self := v.self
	windowIdx := v.windowIdx
	windowEnd := v.windowEnd
	windows := append([]graphWindow(nil), v.windows...)

	return func() tea.Msg {
		now := time.Now().UTC()
		if provider == nil {
			return graphLoadedMsg{now: now}
		}
		cfg := windows[clampInt(windowIdx, 0, len(windows)-1)]
		allTime := cfg.window == 0
		end := windowEnd
		if end.IsZero() {
			end = now
		}
		start := end.Add(-cfg.window)

		filter := data.MessageFilter{}
		if !allTime {
			filter.Since = start
			filter.Until = end
		}

		merged := make([]fmail.Message, 0, 1024)
		seen := make(map[string]struct{}, 1024)

		topics, err := provider.Topics()
		if err != nil {
			return graphLoadedMsg{now: now, err: err}
		}
		for i := range topics {
			topic := strings.TrimSpace(topics[i].Name)
			if topic == "" {
				continue
			}
			msgs, err := provider.Messages(topic, filter)
			if err != nil {
				return graphLoadedMsg{now: now, err: err}
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
					return graphLoadedMsg{now: now, err: dmErr}
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
		return graphLoadedMsg{
			now:      now,
			start:    start,
			end:      end,
			allTime:  allTime,
			messages: merged,
		}
	}
}

func (v *graphView) applyLoaded(msg graphLoadedMsg) {
	v.loading = false
	v.now = msg.now
	v.lastErr = msg.err
	if msg.err != nil {
		return
	}
	v.all = append(v.all[:0], msg.messages...)
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
			end = end.Add(1 * time.Second)
		}
	}
	v.start = start
	v.end = end
	v.rebuildGraph()
}

func (v *graphView) applyIncoming(msg fmail.Message) {
	now := time.Now().UTC()
	v.now = now
	key := statsDedupKey(msg)
	if _, ok := v.seen[key]; ok {
		return
	}
	v.seen[key] = struct{}{}

	// Only follow tail when windowEnd is unset.
	if v.windowEnd.IsZero() {
		v.all = append(v.all, msg)
		sortMessages(v.all)
		v.rebuildGraph()
	}
}

func (v *graphView) rebuildGraph() {
	if !v.topicOverlay {
		v.graph = buildAgentGraph(v.all, v.start, v.end)
	} else {
		v.graph = buildTopicOverlay(v.all, v.start, v.end)
	}
	// Layout within a default canvas; final render clamps.
	layoutCircle(&v.graph, 80, 22, 12, v.zoom)
	v.selected = clampInt(v.selected, 0, maxInt(0, v.currentSelectionMax()))
}

func (v *graphView) currentSelectionMax() int {
	if v.selectMode == graphSelectEdges {
		return maxInt(0, len(v.graph.Edges)-1)
	}
	return maxInt(0, len(v.graph.Nodes)-1)
}

func buildTopicOverlay(messages []fmail.Message, start, end time.Time) graphData {
	type pair struct{ a, b string }
	nodes := make(map[string]*graphNode, 32)
	edges := make(map[pair]*graphEdge, 64)
	for i := range messages {
		msg := messages[i]
		if msg.Time.IsZero() || msg.Time.Before(start) || !msg.Time.Before(end) {
			continue
		}
		agent := strings.TrimSpace(msg.From)
		target := strings.TrimSpace(msg.To)
		if agent == "" || target == "" {
			continue
		}
		a := nodes[agent]
		if a == nil {
			a = &graphNode{ID: agent}
			nodes[agent] = a
		}
		a.Sent++
		a.Total++
		t := nodes[target]
		if t == nil {
			t = &graphNode{ID: target}
			nodes[target] = t
		}
		p := pair{a: agent, b: target}
		e := edges[p]
		if e == nil {
			e = &graphEdge{From: agent, To: target}
			edges[p] = e
		}
		e.Count++
	}
	outNodes := make([]*graphNode, 0, len(nodes))
	for _, n := range nodes {
		outNodes = append(outNodes, n)
	}
	outEdges := make([]*graphEdge, 0, len(edges))
	for _, e := range edges {
		outEdges = append(outEdges, e)
	}
	sort.SliceStable(outNodes, func(i, j int) bool {
		if outNodes[i].Total != outNodes[j].Total {
			return outNodes[i].Total > outNodes[j].Total
		}
		return outNodes[i].ID < outNodes[j].ID
	})
	sort.SliceStable(outEdges, func(i, j int) bool {
		if outEdges[i].Count != outEdges[j].Count {
			return outEdges[i].Count > outEdges[j].Count
		}
		if outEdges[i].From != outEdges[j].From {
			return outEdges[i].From < outEdges[j].From
		}
		return outEdges[i].To < outEdges[j].To
	})
	byID := make(map[string]*graphNode, len(outNodes))
	for _, n := range outNodes {
		byID[n.ID] = n
	}
	return graphData{Nodes: outNodes, Edges: outEdges, ByID: byID, Start: start, End: end}
}

func (v *graphView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	palette := themePalette(theme)
	panel := styles.PanelStyle(palette, true)
	innerW := maxInt(0, width-(styles.LayoutInnerPadding*2)-2)
	innerH := maxInt(1, height-(styles.LayoutInnerPadding*2)-2)

	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb))
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	accent := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true)

	rLabel := v.windows[clampInt(v.windowIdx, 0, len(v.windows)-1)].label
	mode := "agents"
	if v.topicOverlay {
		mode = "topics"
	}
	sel := "nodes"
	if v.selectMode == graphSelectEdges {
		sel = "edges"
	}
	head := fmt.Sprintf("GRAPH  last %s  mode:%s  select:%s  zoom:%.1fx  nodes:%d edges:%d",
		rLabel, mode, sel, v.zoom, len(v.graph.Nodes), len(v.graph.Edges))
	header := titleStyle.Render(truncateVis(head, innerW))

	if v.lastErr != nil {
		content := lipgloss.JoinVertical(lipgloss.Left, header, "", muted.Render("error: "+truncate(v.lastErr.Error(), innerW)))
		return panel.Width(width).Height(height).Render(content)
	}
	if v.loading {
		content := lipgloss.JoinVertical(lipgloss.Left, header, "", muted.Render("Loading..."))
		return panel.Width(width).Height(height).Render(content)
	}

	canvasH := maxInt(8, innerH-4)
	canvas := v.renderCanvas(innerW, canvasH, palette)
	footer := muted.Render(truncateVis("[/]: range  +/-: zoom  Tab: nodes/edges  t: overlay  c: clusters  Enter: details  Esc: back  (v: graph)", innerW))

	content := lipgloss.JoinVertical(lipgloss.Left, header, "", canvas, "", footer)
	if v.detailOpen {
		content = v.renderDetailOverlay(content, innerW, innerH, palette, accent, muted)
	}
	return panel.Width(width).Height(height).Render(content)
}

func (v *graphView) renderDetailOverlay(content string, width, height int, palette styles.Theme, accent, muted lipgloss.Style) string {
	lines := []string{accent.Render("DETAIL")}
	if v.selectMode == graphSelectEdges {
		if len(v.graph.Edges) == 0 {
			lines = append(lines, muted.Render("No edges"))
		} else {
			e := v.graph.Edges[clampInt(v.selected, 0, len(v.graph.Edges)-1)]
			lines = append(lines, fmt.Sprintf("%s -> %s", e.From, e.To))
			lines = append(lines, fmt.Sprintf("count: %d  reverse: %d", e.Count, e.Rev))
		}
	} else {
		if len(v.graph.Nodes) == 0 {
			lines = append(lines, muted.Render("No nodes"))
		} else {
			n := v.graph.Nodes[clampInt(v.selected, 0, len(v.graph.Nodes)-1)]
			lines = append(lines, fmt.Sprintf("%s", n.ID))
			lines = append(lines, fmt.Sprintf("sent: %d  received: %d  degree: %d", n.Sent, n.Received, n.Degree))
		}
	}
	lines = append(lines, muted.Render("Dismiss: Enter or Esc"))
	panel := lipgloss.NewStyle().
		Border(styles.BorderStyleForTheme(palette)).
		BorderForeground(lipgloss.Color(palette.Base.Border)).
		Background(lipgloss.Color(palette.Base.Background)).
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Padding(1, 2).
		Width(minInt(maxInt(40, width-10), 72))
	return lipgloss.Place(width, height, lipgloss.Center, lipgloss.Center, panel.Render(strings.Join(lines, "\n")))
}

func (v *graphView) renderCanvas(width, height int, palette styles.Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	// Build a simple canvas.
	c := newRuneCanvas(width, height)

	// Re-layout using current canvas bounds.
	layoutCircle(&v.graph, width, height, 12, v.zoom)

	// Cluster rectangles (very coarse).
	if v.clustersOn && !v.topicOverlay {
		ids := make([]string, 0, len(v.graph.Nodes))
		for _, n := range v.graph.Nodes {
			ids = append(ids, n.ID)
		}
		clusters := computeClusters(v.graph, 5)
		for _, members := range clusters {
			if len(members) < 2 {
				continue
			}
			minX, minY := width, height
			maxX, maxY := 0, 0
			for _, id := range members {
				n := v.graph.ByID[id]
				if n == nil {
					continue
				}
				x0, y0, x1, y1 := nodeRect(n, width, height)
				minX = minInt(minX, x0)
				minY = minInt(minY, y0)
				maxX = maxInt(maxX, x1)
				maxY = maxInt(maxY, y1)
			}
			if minX < maxX && minY < maxY {
				c.drawRectDotted(minX-1, minY-1, maxX+1, maxY+1)
			}
		}
	}

	// Edges.
	for _, e := range v.graph.Edges {
		if e == nil {
			continue
		}
		from := v.graph.ByID[e.From]
		to := v.graph.ByID[e.To]
		if from == nil || to == nil {
			continue
		}
		fx, fy := clampInt(from.X+v.panX, 0, width-1), clampInt(from.Y+v.panY, 0, height-1)
		tx, ty := clampInt(to.X+v.panX, 0, width-1), clampInt(to.Y+v.panY, 0, height-1)
		h, vv, arrow := edgeChars(e.Count + e.Rev)
		c.drawEdge(fx, fy, tx, ty, h, vv, arrow)
	}

	// Nodes on top.
	for i, n := range v.graph.Nodes {
		if n == nil {
			continue
		}
		selected := v.selectMode == graphSelectNodes && i == v.selected
		label := fmt.Sprintf("%s (%d)", truncate(n.ID, 10), n.Sent)
		if selected {
			label = ">" + label + "<"
		}
		c.drawBoxCenter(clampInt(n.X+v.panX, 0, width-1), clampInt(n.Y+v.panY, 0, height-1), label, selected)
	}

	if v.selectMode == graphSelectEdges && len(v.graph.Edges) > 0 {
		e := v.graph.Edges[clampInt(v.selected, 0, len(v.graph.Edges)-1)]
		if e != nil {
			tag := fmt.Sprintf("[%s -> %s] %d", truncate(e.From, 10), truncate(e.To, 10), e.Count)
			c.drawString(0, height-1, truncate(tag, width))
		}
	}

	return c.String()
}

func edgeChars(weight int) (h, v, arrow rune) {
	switch {
	case weight > 20:
		return '━', '┃', '▶'
	case weight >= 6:
		return '═', '║', '→'
	default:
		return '─', '│', '→'
	}
}

type runeCanvas struct {
	w int
	h int
	b [][]rune
}

func newRuneCanvas(w, h int) *runeCanvas {
	b := make([][]rune, h)
	for y := 0; y < h; y++ {
		row := make([]rune, w)
		for x := 0; x < w; x++ {
			row[x] = ' '
		}
		b[y] = row
	}
	return &runeCanvas{w: w, h: h, b: b}
}

func (c *runeCanvas) set(x, y int, r rune) {
	if x < 0 || y < 0 || x >= c.w || y >= c.h {
		return
	}
	if c.b[y][x] == ' ' {
		c.b[y][x] = r
		return
	}
	// Keep boxes visible.
	if c.b[y][x] == '┌' || c.b[y][x] == '┐' || c.b[y][x] == '└' || c.b[y][x] == '┘' {
		return
	}
	c.b[y][x] = r
}

func (c *runeCanvas) drawString(x, y int, s string) {
	if y < 0 || y >= c.h {
		return
	}
	runes := []rune(s)
	for i, r := range runes {
		if x+i >= c.w {
			break
		}
		if x+i < 0 {
			continue
		}
		c.b[y][x+i] = r
	}
}

func (c *runeCanvas) drawBoxCenter(cx, cy int, label string, selected bool) {
	label = strings.TrimSpace(label)
	if label == "" {
		return
	}
	w := minInt(c.w-2, maxInt(10, len([]rune(label))+2))
	h := 3
	x0 := clampInt(cx-w/2, 0, maxInt(0, c.w-w))
	y0 := clampInt(cy-h/2, 0, maxInt(0, c.h-h))
	x1 := x0 + w - 1
	y1 := y0 + h - 1

	tl, tr, bl, br := '┌', '┐', '└', '┘'
	hr, vr := '─', '│'
	if selected {
		tl, tr, bl, br = '╔', '╗', '╚', '╝'
		hr, vr = '═', '║'
	}
	c.set(x0, y0, tl)
	c.set(x1, y0, tr)
	c.set(x0, y1, bl)
	c.set(x1, y1, br)
	for x := x0 + 1; x < x1; x++ {
		c.set(x, y0, hr)
		c.set(x, y1, hr)
	}
	for y := y0 + 1; y < y1; y++ {
		c.set(x0, y, vr)
		c.set(x1, y, vr)
	}
	c.drawString(x0+1, y0+1, truncate(label, w-2))
}

func (c *runeCanvas) drawEdge(x0, y0, x1, y1 int, h, v, arrow rune) {
	mx := (x0 + x1) / 2
	step := 1
	if x1 < x0 {
		step = -1
	}
	for x := x0; x != mx; x += step {
		c.set(x, y0, h)
	}
	step = 1
	if y1 < y0 {
		step = -1
	}
	for y := y0; y != y1; y += step {
		c.set(mx, y, v)
	}
	step = 1
	if x1 < mx {
		step = -1
	}
	for x := mx; x != x1; x += step {
		c.set(x, y1, h)
	}
	c.set(x1, y1, arrow)
}

func (c *runeCanvas) drawRectDotted(x0, y0, x1, y1 int) {
	x0 = clampInt(x0, 0, c.w-1)
	y0 = clampInt(y0, 0, c.h-1)
	x1 = clampInt(x1, 0, c.w-1)
	y1 = clampInt(y1, 0, c.h-1)
	if x0 >= x1 || y0 >= y1 {
		return
	}
	for x := x0; x <= x1; x++ {
		if x%2 == 0 {
			c.set(x, y0, '·')
			c.set(x, y1, '·')
		}
	}
	for y := y0; y <= y1; y++ {
		if y%2 == 0 {
			c.set(x0, y, '·')
			c.set(x1, y, '·')
		}
	}
}

func (c *runeCanvas) String() string {
	lines := make([]string, 0, c.h)
	for y := 0; y < c.h; y++ {
		lines = append(lines, string(c.b[y]))
	}
	return strings.Join(lines, "\n")
}

func nodeRect(n *graphNode, width, height int) (x0, y0, x1, y1 int) {
	if n == nil {
		return 0, 0, 0, 0
	}
	// Approximate box around the node center.
	w := 14
	h := 3
	x0 = clampInt(n.X-w/2, 0, maxInt(0, width-w))
	y0 = clampInt(n.Y-h/2, 0, maxInt(0, height-h))
	x1 = x0 + w
	y1 = y0 + h
	return x0, y0, x1, y1
}

func minFloat(a, b float64) float64 {
	if a < b {
		return a
	}
	return b
}

func maxFloat(a, b float64) float64 {
	if a > b {
		return a
	}
	return b
}
