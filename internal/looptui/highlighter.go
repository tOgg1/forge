package looptui

import (
	"encoding/json"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"
	"github.com/tOgg1/forge/internal/models"
)

type harnessLogHighlighter struct {
	harness     models.Harness
	inCodeFence bool
}

func newHarnessLogHighlighter(harness models.Harness) *harnessLogHighlighter {
	return &harnessLogHighlighter{harness: harness}
}

func (h *harnessLogHighlighter) HighlightLine(palette tuiPalette, line string) string {
	line = sanitizeLogLine(line)
	trimmed := strings.TrimSpace(line)
	if trimmed == "" {
		return line
	}

	if strings.HasPrefix(trimmed, "```") {
		h.inCodeFence = !h.inCodeFence
		return colorText(line, palette.Accent, true)
	}

	if ts, ok := parseTimestampPrefix(line); ok {
		msg := strings.TrimSpace(line[len(ts):])
		if msg == "" {
			return colorText(ts, palette.Info, false)
		}
		return colorText(ts, palette.Info, false) + " " + h.highlightMessage(palette, msg)
	}

	return h.highlightMessage(palette, line)
}

func (h *harnessLogHighlighter) highlightMessage(palette tuiPalette, line string) string {
	trimmed := strings.TrimSpace(line)
	if trimmed == "" {
		return line
	}

	if color := diffColor(palette, trimmed, h.inCodeFence); color != "" {
		return colorText(line, color, false)
	}

	switch h.harness {
	case models.HarnessCodex:
		if color, ok := codexLineColor(palette, trimmed); ok {
			return colorText(line, color, false)
		}
	case models.HarnessClaude:
		if color, ok := claudeLineColor(palette, trimmed); ok {
			return colorText(line, color, false)
		}
	case models.HarnessOpenCode:
		if color, ok := opencodeLineColor(palette, trimmed); ok {
			return colorText(line, color, false)
		}
	}

	if color := genericLineColor(palette, trimmed); color != "" {
		return colorText(line, color, false)
	}
	return line
}

func codexLineColor(palette tuiPalette, line string) (string, bool) {
	lower := strings.ToLower(line)
	switch lower {
	case "thinking":
		return palette.Focus, true
	case "exec":
		return palette.Info, true
	case "user":
		return palette.Success, true
	case "assistant":
		return palette.Accent, true
	case "system":
		return palette.Warning, true
	}
	if strings.HasPrefix(line, "$ ") || strings.HasPrefix(line, "$") {
		return palette.Accent, true
	}
	if strings.Contains(lower, "codex>") {
		return palette.Info, true
	}
	if evt, ok := jsonEventType(line); ok {
		return eventTypeColor(palette, evt), true
	}
	return "", false
}

func claudeLineColor(palette tuiPalette, line string) (string, bool) {
	lower := strings.ToLower(line)
	if strings.Contains(lower, "claude>") {
		return palette.Accent, true
	}
	if strings.Contains(lower, "permission") || strings.Contains(lower, "approval") {
		return palette.Warning, true
	}
	if evt, ok := jsonEventType(line); ok {
		return eventTypeColor(palette, evt), true
	}
	return "", false
}

func opencodeLineColor(palette tuiPalette, line string) (string, bool) {
	lower := strings.ToLower(line)
	if strings.Contains(lower, "opencode>") {
		return palette.Accent, true
	}
	if strings.HasPrefix(lower, "tool:") || strings.HasPrefix(lower, "action:") {
		return palette.Info, true
	}
	if evt, ok := jsonEventType(line); ok {
		return eventTypeColor(palette, evt), true
	}
	return "", false
}

func jsonEventType(line string) (string, bool) {
	trimmed := strings.TrimSpace(line)
	if !strings.HasPrefix(trimmed, "{") || !strings.HasSuffix(trimmed, "}") {
		return "", false
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(trimmed), &payload); err != nil {
		return "", false
	}
	for _, key := range []string{"type", "event", "status", "level"} {
		if value, ok := payload[key].(string); ok && strings.TrimSpace(value) != "" {
			return strings.ToLower(strings.TrimSpace(value)), true
		}
	}
	return "", false
}

func eventTypeColor(palette tuiPalette, event string) string {
	switch {
	case strings.Contains(event, "error"), strings.Contains(event, "fatal"), strings.Contains(event, "failed"):
		return palette.Error
	case strings.Contains(event, "warn"), strings.Contains(event, "approval"), strings.Contains(event, "permission"):
		return palette.Warning
	case strings.Contains(event, "start"), strings.Contains(event, "run"), strings.Contains(event, "busy"), strings.Contains(event, "working"):
		return palette.Success
	case strings.Contains(event, "stop"), strings.Contains(event, "idle"), strings.Contains(event, "done"), strings.Contains(event, "complete"):
		return palette.Info
	default:
		return palette.Accent
	}
}

func genericLineColor(palette tuiPalette, line string) string {
	lower := strings.ToLower(line)
	switch {
	case strings.Contains(lower, "error"), strings.Contains(lower, "failed"), strings.Contains(lower, "panic"), strings.Contains(lower, "fatal"):
		return palette.Error
	case strings.Contains(lower, "warn"):
		return palette.Warning
	case strings.Contains(lower, "start"), strings.Contains(lower, "running"), strings.Contains(lower, "ready"), strings.Contains(lower, "succeeded"):
		return palette.Success
	case strings.Contains(lower, "sleep"), strings.Contains(lower, "waiting"), strings.Contains(lower, "pause"):
		return palette.Info
	case strings.HasPrefix(line, "#"):
		return palette.Accent
	default:
		return ""
	}
}

func diffColor(palette tuiPalette, line string, inFence bool) string {
	switch {
	case strings.HasPrefix(line, "diff --git"), strings.HasPrefix(line, "index "):
		return palette.Focus
	case strings.HasPrefix(line, "@@"):
		return palette.Warning
	case strings.HasPrefix(line, "+++"):
		return palette.Success
	case strings.HasPrefix(line, "---"):
		return palette.Error
	case inFence && strings.HasPrefix(line, "+"):
		return palette.Success
	case inFence && strings.HasPrefix(line, "-"):
		return palette.Error
	default:
		return ""
	}
}

func parseTimestampPrefix(line string) (string, bool) {
	if strings.TrimSpace(line) == "" {
		return "", false
	}
	leading := len(line) - len(strings.TrimLeft(line, " "))
	rest := line[leading:]
	if !strings.HasPrefix(rest, "[") {
		return "", false
	}
	idx := strings.Index(rest, "]")
	if idx <= 1 {
		return "", false
	}
	ts := rest[1:idx]
	if _, err := time.Parse(time.RFC3339, ts); err != nil {
		return "", false
	}
	return line[:leading+idx+1], true
}

func colorText(text, color string, bold bool) string {
	style := lipgloss.NewStyle().Foreground(lipgloss.Color(color))
	if bold {
		style = style.Bold(true)
	}
	return style.Render(text)
}

func sanitizeLogLine(line string) string {
	line = stripANSI(line)
	line = strings.ReplaceAll(line, "\r", "")
	line = strings.ReplaceAll(line, "\t", "    ")
	return line
}

func lineMatchesLayer(harness models.Harness, line string, layer logLayer) bool {
	if layer == logLayerRaw {
		return true
	}
	trimmed := strings.TrimSpace(sanitizeLogLine(line))
	if trimmed == "" {
		return false
	}

	switch layer {
	case logLayerDiff:
		return looksLikeDiffLine(trimmed)
	case logLayerErrors:
		return looksLikeErrorLine(trimmed)
	case logLayerTools:
		return looksLikeToolLine(harness, trimmed)
	case logLayerEvents:
		return looksLikeEventLine(trimmed)
	default:
		return true
	}
}

func looksLikeDiffLine(line string) bool {
	switch {
	case strings.HasPrefix(line, "diff --git"),
		strings.HasPrefix(line, "index "),
		strings.HasPrefix(line, "@@"),
		strings.HasPrefix(line, "+++"),
		strings.HasPrefix(line, "---"),
		strings.HasPrefix(line, "+"),
		strings.HasPrefix(line, "-"):
		return true
	default:
		return false
	}
}

func looksLikeErrorLine(line string) bool {
	lower := strings.ToLower(line)
	return strings.Contains(lower, "error") ||
		strings.Contains(lower, "failed") ||
		strings.Contains(lower, "panic") ||
		strings.Contains(lower, "fatal") ||
		strings.Contains(lower, "exception") ||
		strings.Contains(lower, "traceback")
}

func looksLikeEventLine(line string) bool {
	if _, ok := parseTimestampPrefix(line); ok {
		return true
	}
	if _, ok := jsonEventType(line); ok {
		return true
	}
	lower := strings.ToLower(line)
	return strings.Contains(lower, "started") ||
		strings.Contains(lower, "running") ||
		strings.Contains(lower, "stopped") ||
		strings.Contains(lower, "completed") ||
		strings.Contains(lower, "queued") ||
		strings.Contains(lower, "status")
}

func looksLikeToolLine(harness models.Harness, line string) bool {
	lower := strings.ToLower(line)
	if strings.HasPrefix(line, "$ ") ||
		strings.HasPrefix(lower, "tool:") ||
		strings.HasPrefix(lower, "action:") ||
		strings.Contains(lower, "exec") ||
		strings.Contains(lower, "apply_patch") ||
		strings.Contains(lower, "functions.") {
		return true
	}

	switch harness {
	case models.HarnessCodex:
		return strings.HasPrefix(lower, "thinking") ||
			strings.HasPrefix(lower, "assistant") ||
			strings.HasPrefix(lower, "user") ||
			strings.Contains(lower, "codex>")
	case models.HarnessClaude:
		return strings.Contains(lower, "claude>")
	case models.HarnessOpenCode:
		return strings.Contains(lower, "opencode>")
	default:
		return false
	}
}
