package fmailtui

import (
	"fmt"
	"math"
	"sort"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
)

type graphLoadedMsg struct {
	now  time.Time
	msgs []fmail.Message
	err  error
}

type graphIncomingMsg struct {
	msg fmail.Message
}

type graphView struct {
	root     string
	self     string
	provider data.MessageProvider

	windows   []time.Duration
	windowIdx int
	windowEnd time.Time

	loading bool
	lastErr error

	all  []fmail.Message
	seen map[string]struct{}

	snap graphSnapshot

	zoom     int
	panX     int
	panY     int
	selected int

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
		windows: []time.Duration{
			1 * time.Hour,
			4 * time.Hour,
			12 * time.Hour,
			24 * time.Hour,
			7 * 24 * time.Hour,
			0, // all-time
		},
		windowIdx: 1,
		seen:      make(map[string]struct{}, 1024),
		zoom:      0,
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
	case "r", "ctrl+r":
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
	case "h", "left":
		if v.windows[v.windowIdx] > 0 {
			v.windowEnd = v.windowEnd.Add(-v.panStep())
			v.loading = true
			return v.loadCmd()
		}
	case "l", "right":
		if v.windows[v.windowIdx] > 0 {
			v.windowEnd = v.windowEnd.Add(v.panStep())
			v.loading = true
			return v.loadCmd()
		}
	case "tab":
		if len(v.snap.Nodes) > 0 {
			v.selected = (v.selected + 1) % len(v.snap.Nodes)
		}
	case "shift+tab":
		if len(v.snap.Nodes) > 0 {
			v.selected--
			if v.selected < 0 {
				v.selected = len(v.snap.Nodes) - 1
			}
		}
	case "up":
		v.panY--
	case "down":
		v.panY++
	case "ctrl+left":
		v.panX--
	case "ctrl+right":
		v.panX++
	case "+":
		if v.zoom < 6 {
			v.zoom++
		}
	case "-":
		if v.zoom > -3 {
			v.zoom--
		}
	}
	return nil
}

func (v *graphView) panStep() time.Duration {
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

func (v *graphView) loadCmd() tea.Cmd {
	provider := v.provider
	self := v.self
	windowIdx := v.windowIdx
	windowEnd := v.windowEnd
	windows := append([]time.Duration(nil), v.windows...)

	return func() tea.Msg {
		now := time.Now().UTC()
		if provider == nil {
			return graphLoadedMsg{now: now}
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
		return graphLoadedMsg{now: now, msgs: merged}
	}
}

func (v *graphView) applyLoaded(msg graphLoadedMsg) {
	v.loading = false
	v.lastErr = msg.err
	if msg.err != nil {
		return
	}

	v.all = append(v.all[:0], msg.msgs...)
	v.seen = make(map[string]struct{}, len(v.all))
	for i := range v.all {
		v.seen[statsDedupKey(v.all[i])] = struct{}{}
	}

	v.snap = buildGraphSnapshot(v.all, graphMaxNodesDefault)
	if v.selected < 0 || v.selected >= len(v.snap.Nodes) {
		v.selected = 0
	}
}

func (v *graphView) applyIncoming(msg fmail.Message) {
	key := statsDedupKey(msg)
	if _, ok := v.seen[key]; ok {
		return
	}
	v.seen[key] = struct{}{}
	v.all = append(v.all, msg)

	// Cheap and safe: rebuild. (Message volume is low enough for this view.)
	v.snap = buildGraphSnapshot(v.all, graphMaxNodesDefault)
	if v.selected < 0 || v.selected >= len(v.snap.Nodes) {
		v.selected = 0
	}
}

func (v *graphView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}

	title := v.headerLine()
	if v.loading {
		return truncateLines([]string{
			truncateVis(title, width),
			truncateVis("loading…", width),
		}, height)
	}
	if v.lastErr != nil {
		return truncateLines([]string{
			truncateVis(title, width),
			truncateVis("error: "+v.lastErr.Error(), width),
		}, height)
	}

	top := []string{
		truncateVis(title, width),
		truncateVis(v.hintLine(), width),
	}

	canvasH := height - len(top) - 6
	if canvasH < 4 {
		canvasH = maxInt(0, height-len(top))
	}

	lines := make([]string, 0, height)
	lines = append(lines, top...)
	lines = append(lines, v.renderCanvas(width, canvasH)...)
	lines = append(lines, v.renderDetails(width)...)
	return truncateLines(lines, height)
}

func (v *graphView) headerLine() string {
	label := v.windowLabel()
	nodes := len(v.snap.Nodes)
	edges := len(v.snap.Edges)
	return fmt.Sprintf("Graph  last %s  %d messages  %d nodes  %d edges", label, v.snap.Messages, nodes, edges)
}

func (v *graphView) hintLine() string {
	return "[/]:range  h/l:time-pan  Tab:next node  +/-:zoom  arrows:pan  r:refresh"
}

func (v *graphView) windowLabel() string {
	if v.windowIdx < 0 || v.windowIdx >= len(v.windows) {
		return "?"
	}
	d := v.windows[v.windowIdx]
	if d == 0 {
		return "all"
	}
	if d%(24*time.Hour) == 0 {
		days := int(d / (24 * time.Hour))
		if days == 1 {
			return "24h"
		}
		return fmt.Sprintf("%dd", days)
	}
	if d%time.Hour == 0 {
		return fmt.Sprintf("%dh", int(d/time.Hour))
	}
	if d%time.Minute == 0 {
		return fmt.Sprintf("%dm", int(d/time.Minute))
	}
	return d.String()
}

type graphBox struct {
	x int
	y int
	w int
	h int
}

func (v *graphView) renderCanvas(width, height int) []string {
	if height <= 0 {
		return nil
	}
	grid := make([][]rune, height)
	for y := range grid {
		row := make([]rune, width)
		for x := range row {
			row[x] = ' '
		}
		grid[y] = row
	}

	boxes := v.layoutBoxes(width, height)
	v.drawEdges(grid, boxes)
	v.drawBoxes(grid, boxes)

	out := make([]string, 0, height)
	for y := range grid {
		out = append(out, string(grid[y]))
	}
	return out
}

func (v *graphView) layoutBoxes(width, height int) []graphBox {
	nodes := v.snap.Nodes
	if len(nodes) == 0 {
		return nil
	}

	baseR := float64(minInt(width, height)) * 0.32
	baseR += float64(v.zoom) * 2.0
	if baseR < 4 {
		baseR = 4
	}

	boxes := make([]graphBox, len(nodes))
	centerIdx := 0
	for i := range nodes {
		if nodes[i].Total > nodes[centerIdx].Total {
			centerIdx = i
		}
	}

	cx := float64(width/2 + v.panX)
	cy := float64(height/2 + v.panY)

	order := make([]int, 0, len(nodes))
	order = append(order, centerIdx)
	for i := range nodes {
		if i == centerIdx {
			continue
		}
		order = append(order, i)
	}

	outer := order[1:]
	for i, idx := range order {
		node := nodes[idx]
		label := node.Name
		count := fmt.Sprintf("(%d)", node.Sent)
		innerW := maxInt(8, maxInt(lipglossWidth(label), lipglossWidth(count))+2)
		bw := innerW + 2
		bh := 4

		x := int(cx) - bw/2
		y := int(cy) - bh/2
		if i > 0 {
			angle := 2 * math.Pi * float64(i-1) / float64(maxInt(1, len(outer)))
			x = int(cx+baseR*math.Cos(angle)) - bw/2
			y = int(cy+baseR*math.Sin(angle)) - bh/2
		}

		// Keep within bounds.
		x = clampInt(x, 0, maxInt(0, width-bw))
		y = clampInt(y, 0, maxInt(0, height-bh))
		boxes[idx] = graphBox{x: x, y: y, w: bw, h: bh}
	}

	return boxes
}

func (v *graphView) drawBoxes(grid [][]rune, boxes []graphBox) {
	for i := range boxes {
		b := boxes[i]
		if b.w <= 0 || b.h <= 0 {
			continue
		}
		selected := i == v.selected
		topL, topR, botL, botR, h, vbar := boxRunes(selected)

		drawText(grid, b.x, b.y, topL+strings.Repeat(string(h), maxInt(0, b.w-2))+topR)

		name := v.snap.Nodes[i].Name
		count := fmt.Sprintf("(%d)", v.snap.Nodes[i].Sent)
		drawText(grid, b.x, b.y+1, string(vbar)+centerPad(name, b.w-2)+string(vbar))
		drawText(grid, b.x, b.y+2, string(vbar)+centerPad(count, b.w-2)+string(vbar))

		drawText(grid, b.x, b.y+3, botL+strings.Repeat(string(h), maxInt(0, b.w-2))+botR)
	}
}

func boxRunes(selected bool) (topL, topR, botL, botR string, h, vbar rune) {
	if selected {
		return "╔", "╗", "╚", "╝", '═', '║'
	}
	return "┌", "┐", "└", "┘", '─', '│'
}

func (v *graphView) drawEdges(grid [][]rune, boxes []graphBox) {
	type edgeDraw struct {
		from  int
		to    int
		count int
	}
	idx := make(map[string]int, len(v.snap.Nodes))
	for i := range v.snap.Nodes {
		idx[v.snap.Nodes[i].Name] = i
	}
	edges := make([]edgeDraw, 0, len(v.snap.Edges))
	for _, e := range v.snap.Edges {
		fi, ok := idx[e.From]
		if !ok {
			continue
		}
		ti, ok := idx[e.To]
		if !ok {
			continue
		}
		edges = append(edges, edgeDraw{from: fi, to: ti, count: e.Count})
	}
	sort.Slice(edges, func(i, j int) bool { return edges[i].count > edges[j].count })

	for _, e := range edges {
		v.drawEdge(grid, boxes[e.from], boxes[e.to], e.count)
	}
}

func (v *graphView) drawEdge(grid [][]rune, from, to graphBox, count int) {
	if len(grid) == 0 || len(grid[0]) == 0 {
		return
	}
	w := len(grid[0])

	fromCX := from.x + from.w/2
	fromCY := from.y + from.h/2
	toCX := to.x + to.w/2
	toCY := to.y + to.h/2

	startX := fromCX
	startY := fromCY
	endX := toCX
	endY := toCY

	if toCX >= fromCX {
		startX = from.x + from.w
		endX = to.x - 1
	} else {
		startX = from.x - 1
		endX = to.x + to.w
	}
	startX = clampInt(startX, 0, w-1)
	endX = clampInt(endX, 0, w-1)

	midX := (startX + endX) / 2
	midX = clampInt(midX, 0, w-1)

	hRune, vRune := edgeRunes(count)

	drawH(grid, startY, startX, midX, hRune)
	drawV(grid, midX, startY, endY, vRune)
	drawH(grid, endY, midX, endX, hRune)

	// Arrow head at end.
	arrow := '→'
	if endX < to.x {
		arrow = '→'
	} else if endX > to.x+to.w {
		arrow = '←'
	}
	if endY < to.y {
		arrow = '↓'
	} else if endY > to.y+to.h {
		arrow = '↑'
	}
	setRuneIfEmpty(grid, endX, endY, arrow)

	// Label near mid.
	label := fmt.Sprintf("%d", count)
	labelX := clampInt(midX-len([]rune(label))/2, 0, maxInt(0, w-1))
	for i, r := range []rune(label) {
		setRuneIfEmpty(grid, labelX+i, startY, r)
	}
}

func edgeRunes(count int) (h, v rune) {
	switch {
	case count >= 20:
		return '━', '┃'
	case count >= 6:
		return '═', '║'
	default:
		return '─', '│'
	}
}

func drawH(grid [][]rune, y, x1, x2 int, r rune) {
	if y < 0 || y >= len(grid) {
		return
	}
	if x2 < x1 {
		x1, x2 = x2, x1
	}
	row := grid[y]
	for x := x1; x <= x2 && x >= 0 && x < len(row); x++ {
		setRuneIfEmpty(grid, x, y, r)
	}
}

func drawV(grid [][]rune, x, y1, y2 int, r rune) {
	if y2 < y1 {
		y1, y2 = y2, y1
	}
	for y := y1; y <= y2 && y >= 0 && y < len(grid); y++ {
		setRuneIfEmpty(grid, x, y, r)
	}
}

func setRuneIfEmpty(grid [][]rune, x, y int, r rune) {
	if y < 0 || y >= len(grid) {
		return
	}
	if x < 0 || x >= len(grid[y]) {
		return
	}
	if grid[y][x] != ' ' {
		return
	}
	grid[y][x] = r
}

func drawText(grid [][]rune, x, y int, s string) {
	if y < 0 || y >= len(grid) {
		return
	}
	row := grid[y]
	col := x
	for _, r := range s {
		if col < 0 {
			col++
			continue
		}
		if col >= len(row) {
			break
		}
		row[col] = r
		col++
	}
}

func centerPad(s string, width int) string {
	if width <= 0 {
		return ""
	}
	s = strings.TrimSpace(s)
	if lipglossWidth(s) >= width {
		return truncateVis(s, width)
	}
	pad := width - lipglossWidth(s)
	left := pad / 2
	right := pad - left
	return strings.Repeat(" ", left) + s + strings.Repeat(" ", right)
}

func lipglossWidth(s string) int {
	// Minimal width helper without importing lipgloss into this file's API surface.
	return len([]rune(s))
}

func truncateLines(lines []string, maxLines int) string {
	if maxLines <= 0 {
		return ""
	}
	if len(lines) > maxLines {
		lines = lines[:maxLines]
	}
	return strings.Join(lines, "\n")
}

func (v *graphView) renderDetails(width int) []string {
	if width <= 0 {
		return nil
	}
	if len(v.snap.Nodes) == 0 {
		return []string{truncateVis("no data", width)}
	}
	if v.selected < 0 || v.selected >= len(v.snap.Nodes) {
		v.selected = 0
	}
	node := v.snap.Nodes[v.selected]

	lines := []string{
		fmt.Sprintf("Selected: %s  sent:%d recv:%d", node.Name, node.Sent, node.Recv),
	}

	// Top outgoing edges.
	type edge struct {
		to    string
		count int
	}
	out := make([]edge, 0, 8)
	for _, e := range v.snap.Edges {
		if e.From != node.Name {
			continue
		}
		out = append(out, edge{to: e.To, count: e.Count})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].count > out[j].count })
	if len(out) > 5 {
		out = out[:5]
	}
	if len(out) == 0 {
		lines = append(lines, "Top: (no edges)")
	} else {
		parts := make([]string, 0, len(out))
		for _, e := range out {
			parts = append(parts, fmt.Sprintf("%s:%d", e.to, e.count))
		}
		lines = append(lines, "Top out: "+strings.Join(parts, "  "))
	}

	for i := range lines {
		lines[i] = truncateVis(lines[i], width)
	}

	// Pad to a stable height so the graph doesn't jump.
	for len(lines) < 6 {
		lines = append(lines, "")
	}
	return lines
}
