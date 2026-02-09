package fmailtui

import (
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"
)

func (v *replayView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	palette := themePalette(theme)
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	hi := lipgloss.NewStyle().Bold(true)

	if v.loading {
		return truncateLines([]string{
			truncateVis("REPLAY  loading…", width),
		}, height)
	}
	if v.lastErr != nil {
		return truncateLines([]string{
			truncateVis("REPLAY  error: "+v.lastErr.Error(), width),
		}, height)
	}
	if len(v.msgs) == 0 {
		return truncateLines([]string{
			truncateVis("REPLAY  (no messages)", width),
		}, height)
	}

	cursorT := v.cursorTime()
	speed := replaySpeedPresets[clampInt(v.speedIdx, 0, len(replaySpeedPresets)-1)]
	playGlyph := "▌▌"
	if v.playing {
		playGlyph = "▶"
	}
	mode := "feed"
	if v.mode == replayModeTimeline {
		mode = "timeline"
	}

	header := fmt.Sprintf("REPLAY  %s  %.0fx  %s / %s  mode:%s",
		playGlyph,
		speed,
		cursorT.Format("15:04:05"),
		v.end.Format("15:04:05"),
		mode,
	)

	agentsLine, topicsLine := v.presenceLines(cursorT)
	if strings.TrimSpace(v.statusLine) != "" {
		topicsLine = topicsLine + "  |  " + v.statusLine
	}

	lines := make([]string, 0, height)
	lines = append(lines, truncateVis(header, width))
	lines = append(lines, muted.Render(truncateVis(agentsLine, width)))
	lines = append(lines, muted.Render(truncateVis(topicsLine, width)))

	// Feed area.
	footerH := 3
	feedH := maxInt(0, height-len(lines)-footerH)
	if v.mode == replayModeTimeline {
		lines = append(lines, v.renderTimeline(width, feedH, hi, muted)...)
	} else {
		lines = append(lines, v.renderFeed(width, feedH, hi)...)
	}

	// Scrubber + controls.
	lines = append(lines, muted.Render(truncateVis(v.renderScrubber(width), width)))
	lines = append(lines, muted.Render(truncateVis("Space:play/pause  ←/→:step  Shift+←/→:±1m  1-4:speed  t:mode  m/':marks  e:export  Esc:back  (R: replay)", width)))

	if v.pendingMark {
		lines = append(lines, muted.Render(truncateVis("mark: press a-z (Esc cancel)", width)))
	} else if v.pendingJump {
		lines = append(lines, muted.Render(truncateVis("jump: press a-z (Esc cancel)", width)))
	} else {
		lines = append(lines, "")
	}

	return truncateLines(lines, height)
}

func (v *replayView) cursorTime() time.Time {
	if len(v.times) == 0 || v.idx < 0 || v.idx >= len(v.times) {
		return v.start
	}
	if t := v.times[v.idx]; !t.IsZero() {
		return t
	}
	return replayMessageTime(v.msgs[v.idx])
}

func (v *replayView) presenceLines(cursor time.Time) (string, string) {
	if cursor.IsZero() {
		cursor = time.Now().UTC()
	}
	cutoff := cursor.Add(-5 * time.Minute)

	agentLast := make(map[string]time.Time, 16)
	topicLast := make(map[string]time.Time, 16)

	for i := v.idx; i >= 0 && i < len(v.msgs); i-- {
		t := v.times[i]
		if t.IsZero() {
			t = replayMessageTime(v.msgs[i])
		}
		if !t.IsZero() && t.Before(cutoff) {
			break
		}
		msg := v.msgs[i]
		if strings.TrimSpace(msg.From) != "" {
			if prev, ok := agentLast[msg.From]; !ok || t.After(prev) {
				agentLast[msg.From] = t
			}
		}
		if strings.TrimSpace(msg.To) != "" && !strings.HasPrefix(strings.TrimSpace(msg.To), "@") {
			if prev, ok := topicLast[msg.To]; !ok || t.After(prev) {
				topicLast[msg.To] = t
			}
		}
	}

	type kv struct {
		k string
		t time.Time
	}
	agents := make([]kv, 0, len(agentLast))
	for k, t := range agentLast {
		agents = append(agents, kv{k: k, t: t})
	}
	sort.Slice(agents, func(i, j int) bool {
		if !agents[i].t.Equal(agents[j].t) {
			return agents[i].t.After(agents[j].t)
		}
		return agents[i].k < agents[j].k
	})
	if len(agents) > 6 {
		agents = agents[:6]
	}
	agentParts := make([]string, 0, len(agents))
	for _, a := range agents {
		active := "●"
		if a.t.Before(cutoff) {
			active = "◌"
		}
		agentParts = append(agentParts, fmt.Sprintf("%s %s", active, a.k))
	}
	if len(agentParts) == 0 {
		agentParts = append(agentParts, "no recent agents")
	}

	topics := make([]kv, 0, len(topicLast))
	for k, t := range topicLast {
		topics = append(topics, kv{k: k, t: t})
	}
	sort.Slice(topics, func(i, j int) bool {
		if !topics[i].t.Equal(topics[j].t) {
			return topics[i].t.After(topics[j].t)
		}
		return topics[i].k < topics[j].k
	})
	if len(topics) > 6 {
		topics = topics[:6]
	}
	topicParts := make([]string, 0, len(topics))
	for _, t := range topics {
		topicParts = append(topicParts, t.k)
	}
	if len(topicParts) == 0 {
		topicParts = append(topicParts, "no recent topics")
	}

	return "Agents: " + strings.Join(agentParts, "  "), "Topics: " + strings.Join(topicParts, "  ")
}

func (v *replayView) renderFeed(width, height int, hi lipgloss.Style) []string {
	if height <= 0 {
		return nil
	}
	start := maxInt(0, v.idx-height+1)
	lines := make([]string, 0, height)
	now := time.Now().UTC()
	for i := start; i <= v.idx && i < len(v.msgs); i++ {
		t := v.times[i]
		if t.IsZero() {
			t = replayMessageTime(v.msgs[i])
		}
		head := fmt.Sprintf("%s %s -> %s", t.Format("15:04:05"), v.msgs[i].From, v.msgs[i].To)
		body := firstLine(v.msgs[i].Body)
		if strings.TrimSpace(body) != "" {
			head += ": " + body
		}
		line := truncateVis(head, width)
		if i == v.idx && !v.highlight.IsZero() && now.Before(v.highlight) {
			line = hi.Render(line)
		}
		lines = append(lines, line)
	}
	for len(lines) < height {
		lines = append(lines, "")
	}
	return lines
}

func (v *replayView) renderTimeline(width, height int, hi, muted lipgloss.Style) []string {
	if height <= 0 {
		return nil
	}
	now := time.Now().UTC()
	lines := make([]string, 0, height)

	var prevBucket time.Time
	for i := v.idx; i >= 0 && len(lines) < height; i-- {
		t := v.times[i]
		if t.IsZero() {
			t = replayMessageTime(v.msgs[i])
		}
		bucket := t.Truncate(time.Minute)
		if prevBucket.IsZero() {
			prevBucket = bucket
		}
		if !bucket.Equal(prevBucket) && len(lines) < height {
			lines = append(lines, muted.Render(truncateVis(fmt.Sprintf("-- %s --", prevBucket.Format("15:04")), width)))
			prevBucket = bucket
		}

		head := fmt.Sprintf("%s %s -> %s", t.Format("15:04:05"), v.msgs[i].From, v.msgs[i].To)
		body := firstLine(v.msgs[i].Body)
		if strings.TrimSpace(body) != "" {
			head += ": " + body
		}
		line := truncateVis(head, width)
		if i == v.idx && !v.highlight.IsZero() && now.Before(v.highlight) {
			line = hi.Render(line)
		}
		lines = append(lines, line)
	}
	if len(lines) < height && !prevBucket.IsZero() {
		lines = append(lines, muted.Render(truncateVis(fmt.Sprintf("-- %s --", prevBucket.Format("15:04")), width)))
	}

	// Reverse into chronological order.
	for i, j := 0, len(lines)-1; i < j; i, j = i+1, j-1 {
		lines[i], lines[j] = lines[j], lines[i]
	}
	for len(lines) < height {
		lines = append(lines, "")
	}
	return lines
}

func (v *replayView) renderScrubber(width int) string {
	if width <= 0 {
		return ""
	}
	barW := maxInt(10, width-22)
	if barW > 80 {
		barW = 80
	}
	total := v.end.Sub(v.start)
	posT := v.cursorTime()
	ratio := 0.0
	if total > 0 && !posT.IsZero() {
		ratio = float64(posT.Sub(v.start)) / float64(total)
	}
	if ratio < 0 {
		ratio = 0
	}
	if ratio > 1 {
		ratio = 1
	}
	pos := int(ratio * float64(barW-1))
	if pos < 0 {
		pos = 0
	}
	if pos >= barW {
		pos = barW - 1
	}

	bar := make([]rune, barW)
	for i := range bar {
		bar[i] = '-'
	}
	for i := 0; i < pos; i++ {
		bar[i] = '='
	}
	bar[pos] = '>'

	for _, idx := range v.marks {
		if idx < 0 || idx >= len(v.times) {
			continue
		}
		mt := v.times[idx]
		if mt.IsZero() {
			continue
		}
		mr := 0.0
		if total > 0 {
			mr = float64(mt.Sub(v.start)) / float64(total)
		}
		mp := int(mr * float64(barW-1))
		if mp >= 0 && mp < barW && mp != pos {
			bar[mp] = '|'
		}
	}

	return fmt.Sprintf("[%s] %s - %s", string(bar), v.start.Format("15:04"), v.end.Format("15:04"))
}
