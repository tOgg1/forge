package fmailtui

import (
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmailtui/data"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

func (v *searchView) renderQueryLine(width int, palette styles.Theme) string {
	prompt := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("> ")
	if v.focus == searchFocusSavePrompt {
		cursor := "_"
		r := []rune(v.saveName)
		pos := clampInt(v.saveNameCursor, 0, len(r))
		withCursor := string(r[:pos]) + cursor + string(r[pos:])
		return prompt + lipgloss.NewStyle().Bold(true).Render("save: ") + truncateVis(withCursor, maxInt(0, width-8))
	}

	cursor := "_"
	if v.searching {
		cursor = ""
	}
	q := []rune(v.query)
	pos := clampInt(v.cursor, 0, len(q))
	plainWithCursor := string(q[:pos]) + cursor + string(q[pos:])

	if strings.TrimSpace(v.query) == "" && v.cursor == 0 {
		plainWithCursor = cursor
	}
	rendered := renderSearchQueryTokens(plainWithCursor, palette)
	return prompt + truncateVis(rendered, width)
}

func (v *searchView) renderSavedLine(width int, palette styles.Theme) string {
	label := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Saved: ")
	if len(v.saved) == 0 {
		return label + lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("(none)")
	}
	parts := make([]string, 0, len(v.saved))
	for i := range v.saved {
		name := strings.TrimSpace(v.saved[i].Name)
		if name == "" {
			continue
		}
		style := lipgloss.NewStyle().
			Foreground(lipgloss.Color(palette.Base.Foreground)).
			Background(lipgloss.Color(palette.Base.Border)).
			Padding(0, 1)
		if v.focus == searchFocusSaved && i == v.savedSelected {
			style = style.Background(lipgloss.Color(palette.Chrome.SelectedItem)).Bold(true)
		}
		parts = append(parts, style.Render(name))
	}
	line := label + strings.Join(parts, " ")
	return truncateVis(line, width)
}

func (v *searchView) renderStatsLine(width int, palette styles.Theme) string {
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	n := len(v.results)
	stats := fmt.Sprintf("%d results", n)
	if v.metaLoaded && v.totalTargets > 0 {
		stats = fmt.Sprintf("%s (searched ~%d messages in %d targets)", stats, v.totalMessages, v.totalTargets)
	}
	if v.took > 0 {
		stats = fmt.Sprintf("%s in %s", stats, v.took.Truncate(time.Millisecond))
	}
	return muted.Render(truncateVis(stats, width))
}

func (v *searchView) renderResults(width, height int, palette styles.Theme) string {
	if height <= 0 {
		return ""
	}
	if v.searching && strings.TrimSpace(v.query) == "" {
		return ""
	}
	if len(v.results) == 0 {
		if strings.TrimSpace(v.query) == "" {
			return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Type to search. Example: from:coder-* tag:auth since:1h token refresh")
		}
		if v.searching {
			return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Searching...")
		}
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("No results")
	}

	parsed := parseSearchInput(v.query, v.now)
	matchText := strings.TrimSpace(parsed.Query.Text)

	bookmarked := make(map[string]bool)
	annotated := make(map[string]bool)
	readMarkers := make(map[string]string)
	if v.state != nil {
		snap := v.state.Snapshot()
		for _, bm := range snap.Bookmarks {
			if strings.TrimSpace(bm.MessageID) != "" {
				bookmarked[bm.MessageID] = true
			}
		}
		for id, note := range snap.Annotations {
			id = strings.TrimSpace(id)
			note = strings.TrimSpace(note)
			if id != "" && note != "" {
				annotated[id] = true
			}
		}
		for k, vv := range snap.ReadMarkers {
			readMarkers[k] = vv
		}
	}

	itemsByTopic := make(map[string][]searchItem)
	for i := range v.results {
		r := v.results[i]
		if parsed.Query.HasBookmark && !bookmarked[r.Message.ID] {
			continue
		}
		if parsed.Query.HasAnnotation && !annotated[r.Message.ID] {
			continue
		}
		if parsed.Query.IsUnread {
			marker := strings.TrimSpace(readMarkers[r.Topic])
			if marker != "" && r.Message.ID <= marker {
				continue
			}
		}

		count := 0
		if matchText != "" {
			count = countOccurrencesCI(messageBodyString(r.Message.Body), matchText)
		}
		if count == 0 && matchText != "" {
			count = 1
		}
		itemsByTopic[r.Topic] = append(itemsByTopic[r.Topic], searchItem{r: r, matchText: matchText, matches: count})
	}

	topics := make([]string, 0, len(itemsByTopic))
	for t := range itemsByTopic {
		topics = append(topics, t)
	}

	for t := range topics {
		list := itemsByTopic[topics[t]]
		sort.SliceStable(list, func(i, j int) bool {
			if list[i].matches != list[j].matches {
				return list[i].matches > list[j].matches
			}
			ti := list[i].r.Message.Time
			tj := list[j].r.Message.Time
			if !ti.Equal(tj) {
				return ti.After(tj)
			}
			return list[i].r.Message.ID > list[j].r.Message.ID
		})
		itemsByTopic[topics[t]] = list
	}

	sort.SliceStable(topics, func(i, j int) bool {
		a := itemsByTopic[topics[i]]
		b := itemsByTopic[topics[j]]
		ai, bi := 0, 0
		if len(a) > 0 {
			ai = a[0].matches
		}
		if len(b) > 0 {
			bi = b[0].matches
		}
		if ai != bi {
			return ai > bi
		}
		at, bt := time.Time{}, time.Time{}
		if len(a) > 0 {
			at = a[0].r.Message.Time
		}
		if len(b) > 0 {
			bt = b[0].r.Message.Time
		}
		if !at.Equal(bt) {
			return at.After(bt)
		}
		return topics[i] < topics[j]
	})

	mapper := styles.NewAgentColorMapperWithPalette(palette.AgentPalette)
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	hi := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Underline(true)

	lines := make([]string, 0, height*2)
	resultIndex := -1
	for _, topic := range topics {
		header := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render("[" + topic + "]")
		lines = append(lines, truncateVis(header, width))
		list := itemsByTopic[topic]
		for _, it := range list {
			resultIndex++
			msg := it.r.Message
			ts := msg.Time.UTC()
			if ts.IsZero() {
				ts = v.now
			}
			fromColored := mapper.Foreground(msg.From).Render(mapper.Plain(msg.From))
			matchSuffix := ""
			if it.matchText != "" {
				matchSuffix = fmt.Sprintf(" — %d matches", it.matches)
			}

			cursor := " "
			rowStyle := lipgloss.NewStyle()
			if resultIndex == v.selected {
				cursor = "▸"
				rowStyle = rowStyle.Foreground(lipgloss.Color(palette.Chrome.SelectedItem)).Bold(true)
			}

			head := fmt.Sprintf("%s %s (%s)%s", cursor, fromColored, ts.Format("15:04"), matchSuffix)
			lines = append(lines, rowStyle.Render(truncateVis(head, width)))

			if it.r.Prev != nil {
				prevLine := truncate(firstLine(messageBodyString(it.r.Prev.Body)), maxInt(0, width-6))
				lines = append(lines, muted.Render(truncateVis("    "+prevLine, width)))
			}

			body := strings.TrimSpace(messageBodyString(msg.Body))
			if body == "" {
				body = "(empty)"
			}
			bodyLine := truncate(firstLine(body), maxInt(0, width-6))
			if it.matchText != "" {
				bodyLine = highlightAllCI(bodyLine, it.matchText, hi)
			}
			lines = append(lines, truncateVis("  "+bodyLine, width))

			if it.r.Next != nil {
				nextLine := truncate(firstLine(messageBodyString(it.r.Next.Body)), maxInt(0, width-6))
				lines = append(lines, muted.Render(truncateVis("    "+nextLine, width)))
			}

			lines = append(lines, "")
			if len(lines) >= height {
				break
			}
		}
		if len(lines) >= height {
			break
		}
	}

	if len(lines) > 0 && lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	if len(lines) > height {
		lines = lines[:height]
	}
	return strings.Join(lines, "\n")
}

func renderSearchQueryTokens(raw string, palette styles.Theme) string {
	keyStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent))
	valStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Chrome.Breadcrumb))
	freeStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground))

	toks := strings.Fields(raw)
	if len(toks) == 0 {
		return freeStyle.Render(raw)
	}

	out := make([]string, 0, len(toks))
	for _, tok := range toks {
		if !strings.Contains(tok, ":") {
			out = append(out, freeStyle.Render(tok))
			continue
		}
		key, val, _ := strings.Cut(tok, ":")
		k := strings.ToLower(strings.TrimSpace(key))
		if k == "" {
			out = append(out, freeStyle.Render(tok))
			continue
		}
		out = append(out, keyStyle.Render(key+":")+valStyle.Render(val))
	}
	return strings.Join(out, " ")
}

func renderSearchQuery(q data.SearchQuery) string {
	parts := make([]string, 0, 10)
	if q.From != "" {
		parts = append(parts, "from:"+q.From)
	}
	if q.To != "" {
		parts = append(parts, "to:"+q.To)
	}
	if q.In != "" {
		parts = append(parts, "in:"+q.In)
	}
	if q.Priority != "" {
		parts = append(parts, "priority:"+q.Priority)
	}
	for _, t := range q.Tags {
		if strings.TrimSpace(t) != "" {
			parts = append(parts, "tag:"+t)
		}
	}
	if q.HasReply {
		parts = append(parts, "has:reply")
	}
	if q.HasBookmark {
		parts = append(parts, "has:bookmark")
	}
	if q.HasAnnotation {
		parts = append(parts, "has:annotation")
	}
	if q.IsUnread {
		parts = append(parts, "is:unread")
	}
	if q.Text != "" {
		parts = append(parts, q.Text)
	}
	return strings.Join(parts, " ")
}

func countOccurrencesCI(haystack, needle string) int {
	h := strings.ToLower(haystack)
	n := strings.ToLower(strings.TrimSpace(needle))
	if n == "" {
		return 0
	}
	count := 0
	for {
		idx := strings.Index(h, n)
		if idx < 0 {
			break
		}
		count++
		h = h[idx+len(n):]
	}
	return count
}

func highlightAllCI(line, needle string, style lipgloss.Style) string {
	if strings.TrimSpace(needle) == "" || line == "" {
		return line
	}
	lower := strings.ToLower(line)
	n := strings.ToLower(needle)
	var out strings.Builder
	i := 0
	for {
		idx := strings.Index(lower[i:], n)
		if idx < 0 {
			out.WriteString(line[i:])
			break
		}
		idx += i
		out.WriteString(line[i:idx])
		out.WriteString(style.Render(line[idx : idx+len(needle)]))
		i = idx + len(needle)
		if i >= len(line) {
			break
		}
	}
	return out.String()
}
