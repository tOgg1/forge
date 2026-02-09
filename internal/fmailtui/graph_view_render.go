package fmailtui

import (
	"fmt"
	"sort"
	"strings"
	"time"
)

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

	detailsH := 0
	if v.showDetails {
		detailsH = 6
	}

	canvasH := height - len(top) - detailsH
	if canvasH < 4 {
		canvasH = maxInt(0, height-len(top))
	}

	lines := make([]string, 0, height)
	lines = append(lines, top...)
	if v.topicOverlay {
		lines = append(lines, v.renderOverlayCanvas(width, canvasH)...)
	} else {
		lines = append(lines, v.renderCanvas(width, canvasH)...)
	}
	if v.showDetails {
		lines = append(lines, v.renderDetails(width)...)
	}
	return truncateLines(lines, height)
}

func (v *graphView) headerLine() string {
	label := v.windowLabel()
	nodes := len(v.snap.Nodes)
	edges := len(v.snap.Edges)
	mode := "agents"
	if v.topicOverlay {
		mode = "topics"
	}
	cluster := ""
	if v.clusters {
		cluster = "  clusters:on"
	}
	return fmt.Sprintf("Graph  last %s  mode:%s%s  %d messages  %d nodes  %d edges", label, mode, cluster, v.snap.Messages, nodes, edges)
}

func (v *graphView) hintLine() string {
	return "[/]:range  h/l:time-pan  t:overlay  c:clusters  Tab:next node  +/-:zoom  arrows:pan  Enter:details  r:refresh"
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
		if d == 24*time.Hour {
			return "24h"
		}
		return fmt.Sprintf("%dd", int(d/(24*time.Hour)))
	}
	if d%time.Hour == 0 {
		return fmt.Sprintf("%dh", int(d/time.Hour))
	}
	if d%time.Minute == 0 {
		return fmt.Sprintf("%dm", int(d/time.Minute))
	}
	return d.String()
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
	for len(lines) < 6 {
		lines = append(lines, "")
	}
	return lines
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
