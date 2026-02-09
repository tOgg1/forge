package fmailtui

import (
	"fmt"
	"math"
	"sort"
	"strings"
)

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

func (v *graphView) renderOverlayCanvas(width, height int) []string {
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

	agentBoxes := v.layoutBoxes(width, height)
	topicBoxes := v.layoutTopicBoxes(width, height)

	v.drawAgentTopicEdges(grid, agentBoxes, topicBoxes)
	v.drawTopicLabels(grid, topicBoxes)
	v.drawBoxes(grid, agentBoxes)

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

		x = clampInt(x, 0, maxInt(0, width-bw))
		y = clampInt(y, 0, maxInt(0, height-bh))
		boxes[idx] = graphBox{x: x, y: y, w: bw, h: bh}
	}

	return boxes
}

func (v *graphView) layoutTopicBoxes(width, height int) map[string]graphBox {
	topics := v.snap.Topics
	if len(topics) == 0 {
		return nil
	}

	baseR := float64(minInt(width, height)) * 0.18
	baseR += float64(v.zoom) * 1.5
	if baseR < 3 {
		baseR = 3
	}

	cx := float64(width/2 + v.panX)
	cy := float64(height/2 + v.panY)

	out := make(map[string]graphBox, len(topics))
	for i := range topics {
		topic := topics[i]
		name := strings.TrimSpace(topic.Name)
		if name == "" {
			continue
		}
		label := fmt.Sprintf("(%s %d)", truncateVis(name, 12), topic.MessageCount)
		bw := lipglossWidth(label)
		if bw < 6 {
			bw = 6
		}
		angle := 2 * math.Pi * float64(i) / float64(maxInt(1, len(topics)))
		x := int(cx+baseR*math.Cos(angle)) - bw/2
		y := int(cy + baseR*math.Sin(angle))
		x = clampInt(x, 0, maxInt(0, width-bw))
		y = clampInt(y, 0, maxInt(0, height-1))
		out[name] = graphBox{x: x, y: y, w: bw, h: 1}
	}
	return out
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

func (v *graphView) drawTopicLabels(grid [][]rune, topics map[string]graphBox) {
	if len(topics) == 0 {
		return
	}
	for _, topic := range v.snap.Topics {
		name := strings.TrimSpace(topic.Name)
		b, ok := topics[name]
		if !ok {
			continue
		}
		label := fmt.Sprintf("(%s %d)", truncateVis(name, 12), topic.MessageCount)
		drawText(grid, b.x, b.y, truncateVis(label, b.w))
	}
}

func (v *graphView) drawAgentTopicEdges(grid [][]rune, agentBoxes []graphBox, topicBoxes map[string]graphBox) {
	if len(agentBoxes) == 0 || len(topicBoxes) == 0 {
		return
	}
	agentIdx := make(map[string]int, len(v.snap.Nodes))
	for i := range v.snap.Nodes {
		agentIdx[v.snap.Nodes[i].Name] = i
	}
	edges := make([]graphEdge, 0, len(v.snap.AgentTopicEdges))
	edges = append(edges, v.snap.AgentTopicEdges...)
	sort.Slice(edges, func(i, j int) bool { return edges[i].Count > edges[j].Count })

	for _, e := range edges {
		fi, ok := agentIdx[e.From]
		if !ok {
			continue
		}
		tb, ok := topicBoxes[e.To]
		if !ok {
			continue
		}
		v.drawEdge(grid, agentBoxes[fi], tb, e.Count)
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
	return len([]rune(s))
}
