package fmailtui

import (
	"fmt"
	"regexp"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/reflow/wordwrap"

	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

var urlPattern = regexp.MustCompile(`https?://[^\s\])}>,"']+`)

func relativeTime(ts time.Time, now time.Time) string {
	if ts.IsZero() {
		return "unknown"
	}
	if now.IsZero() {
		now = time.Now().UTC()
	}
	delta := now.Sub(ts)
	if delta < 0 {
		delta = -delta
	}
	switch {
	case delta < time.Minute:
		return fmt.Sprintf("%ds ago", int(delta.Seconds()))
	case delta < time.Hour:
		return fmt.Sprintf("%dm ago", int(delta.Minutes()))
	case delta < 24*time.Hour:
		return fmt.Sprintf("%dh ago", int(delta.Hours()))
	default:
		return fmt.Sprintf("%dd ago", int(delta.Hours()/24))
	}
}

func renderBodyLines(body string, width int, palette styles.Theme) []string {
	if width <= 0 {
		width = 1
	}
	body = strings.ReplaceAll(body, "\r\n", "\n")
	if strings.TrimSpace(body) == "" {
		return []string{""}
	}

	codeStyle := lipgloss.NewStyle().Background(lipgloss.Color(palette.Borders.Divider)).Foreground(lipgloss.Color(palette.Base.Foreground))
	inlineCodeStyle := lipgloss.NewStyle().Background(lipgloss.Color(palette.Base.Border)).Foreground(lipgloss.Color(palette.Base.Foreground)).Bold(true)
	urlStyle := lipgloss.NewStyle().Underline(true)

	inCode := false
	lines := strings.Split(body, "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "```") {
			inCode = !inCode
			continue
		}

		if inCode {
			wrapped := wrapLines(line, width)
			for _, codeLine := range wrapped {
				out = append(out, codeStyle.Render(codeLine))
			}
			continue
		}

		wrapped := wrapLines(line, width)
		for _, textLine := range wrapped {
			out = append(out, highlightInlineCodeAndURLs(textLine, inlineCodeStyle, urlStyle))
		}
	}
	if len(out) == 0 {
		return []string{""}
	}
	return out
}

func highlightInlineCodeAndURLs(line string, inlineStyle lipgloss.Style, urlStyle lipgloss.Style) string {
	// ANSI-safe: operate on plain text and only inject styling once.
	if line == "" {
		return line
	}
	var b strings.Builder
	rest := line
	for {
		start := strings.IndexByte(rest, '`')
		if start < 0 {
			b.WriteString(highlightURLs(rest, urlStyle))
			break
		}
		end := strings.IndexByte(rest[start+1:], '`')
		if end < 0 {
			b.WriteString(highlightURLs(rest, urlStyle))
			break
		}
		end = start + 1 + end
		if start > 0 {
			b.WriteString(highlightURLs(rest[:start], urlStyle))
		}
		b.WriteString(inlineStyle.Render(rest[start : end+1]))
		rest = rest[end+1:]
		if rest == "" {
			break
		}
	}
	return b.String()
}

func wrapLines(line string, width int) []string {
	if width <= 0 {
		return []string{line}
	}
	wrapped := wordwrap.String(line, width)
	parts := strings.Split(wrapped, "\n")
	if len(parts) == 0 {
		return []string{""}
	}
	return parts
}

func highlightURLs(line string, style lipgloss.Style) string {
	matches := urlPattern.FindAllStringIndex(line, -1)
	if len(matches) == 0 {
		return line
	}
	var b strings.Builder
	cursor := 0
	for _, m := range matches {
		if m[0] > cursor {
			b.WriteString(line[cursor:m[0]])
		}
		b.WriteString(style.Render(line[m[0]:m[1]]))
		cursor = m[1]
	}
	if cursor < len(line) {
		b.WriteString(line[cursor:])
	}
	return b.String()
}

func messageBodyString(body any) string {
	if body == nil {
		return ""
	}
	if s, ok := body.(string); ok {
		return s
	}
	return fmt.Sprint(body)
}

func shortID(id string) string {
	id = strings.TrimSpace(id)
	if len(id) <= 8 {
		return id
	}
	return id[:8]
}
