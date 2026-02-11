package styles

import (
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/reflow/wordwrap"
)

const replyPrefix = "│ "

// MessageStyles contains pre-built styles for message rendering.
type MessageStyles struct {
	Theme       Theme
	AgentColors *AgentColorMapper

	HeaderBase     lipgloss.Style
	Timestamp      lipgloss.Style
	Body           lipgloss.Style
	ReplyIndicator lipgloss.Style
	PriorityHigh   lipgloss.Style
	PriorityLow    lipgloss.Style
	Tag            lipgloss.Style
	Unread         lipgloss.Style
}

// NewMessageStyles builds a reusable style set for messages.
func NewMessageStyles(theme Theme, mapper *AgentColorMapper) MessageStyles {
	if mapper == nil {
		mapper = NewAgentColorMapperWithPalette(theme.AgentPalette)
	}

	return MessageStyles{
		Theme:          theme,
		AgentColors:    mapper,
		HeaderBase:     lipgloss.NewStyle().Foreground(lipgloss.Color(theme.Base.Foreground)),
		Timestamp:      lipgloss.NewStyle().Foreground(lipgloss.Color(theme.Base.Muted)),
		Body:           lipgloss.NewStyle().Foreground(lipgloss.Color(theme.Base.Foreground)),
		ReplyIndicator: lipgloss.NewStyle().Foreground(lipgloss.Color(theme.Base.Muted)).Bold(true),
		PriorityHigh: lipgloss.NewStyle().
			Foreground(lipgloss.Color(theme.Priority.High)).
			Bold(true),
		PriorityLow: lipgloss.NewStyle().
			Foreground(lipgloss.Color(theme.Priority.Low)).
			Faint(true),
		Tag: lipgloss.NewStyle().
			Foreground(lipgloss.Color(theme.Chrome.Breadcrumb)).
			Background(lipgloss.Color(theme.Base.Background)),
		Unread: lipgloss.NewStyle().
			Foreground(lipgloss.Color(theme.Base.Accent)).
			Bold(true),
	}
}

// RenderHeader renders message header with agent + timestamp.
func (s MessageStyles) RenderHeader(agent string, ts time.Time) string {
	agentName := strings.TrimSpace(agent)
	if agentName == "" {
		agentName = "unknown"
	}

	agentText := s.AgentColors.Foreground(agentName).Render(agentName)
	timeText := s.Timestamp.Render(ts.Format("15:04:05"))
	return s.HeaderBase.Render(agentText + " " + timeText)
}

// RenderBody renders wrapped body text.
func (s MessageStyles) RenderBody(body string, width int) string {
	return s.Body.Render(wrapMessageBody(body, width))
}

// RenderReply renders wrapped reply body with an indented vertical bar.
func (s MessageStyles) RenderReply(body string, width int) string {
	renderWidth := width - lipgloss.Width(replyPrefix)
	if renderWidth < 1 {
		renderWidth = 1
	}

	wrapped := wrapMessageBody(body, renderWidth)
	lines := strings.Split(wrapped, "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		out = append(out, s.ReplyIndicator.Render(replyPrefix)+s.Body.Render(line))
	}
	return strings.Join(out, "\n")
}

// RenderPriorityBadge renders [HIGH] / [LOW] badges.
func (s MessageStyles) RenderPriorityBadge(priority string) string {
	label := priorityBadgeLabel(priority)
	switch label {
	case "[HIGH]":
		return s.PriorityHigh.Render(label)
	case "[LOW]":
		return s.PriorityLow.Render(label)
	default:
		return ""
	}
}

// RenderTagPills renders a normalized list of [tag] pills.
func (s MessageStyles) RenderTagPills(tags []string) string {
	normalized := normalizeTags(tags)
	if len(normalized) == 0 {
		return ""
	}

	rendered := make([]string, 0, len(normalized))
	for _, tag := range normalized {
		rendered = append(rendered, s.Tag.Render("["+tag+"]"))
	}
	return strings.Join(rendered, " ")
}

// RenderUnreadIndicator renders a bold unread dot.
func (s MessageStyles) RenderUnreadIndicator(unread bool) string {
	if !unread {
		return ""
	}
	return s.Unread.Render("●")
}

func wrapMessageBody(body string, width int) string {
	if width <= 0 {
		return body
	}

	parts := strings.Split(body, "\n")
	for i := range parts {
		parts[i] = wordwrap.String(parts[i], width)
	}
	return strings.Join(parts, "\n")
}

func normalizePriority(priority string) string {
	switch strings.ToLower(strings.TrimSpace(priority)) {
	case "high", "urgent", "p0":
		return "high"
	case "low", "p3", "p4":
		return "low"
	default:
		return "normal"
	}
}

func priorityBadgeLabel(priority string) string {
	switch normalizePriority(priority) {
	case "high":
		return "[HIGH]"
	case "low":
		return "[LOW]"
	default:
		return ""
	}
}

func normalizeTags(tags []string) []string {
	if len(tags) == 0 {
		return nil
	}

	seen := make(map[string]struct{}, len(tags))
	out := make([]string, 0, len(tags))
	for _, tag := range tags {
		normalized := strings.ToLower(strings.TrimSpace(tag))
		if normalized == "" {
			continue
		}
		if _, ok := seen[normalized]; ok {
			continue
		}
		seen[normalized] = struct{}{}
		out = append(out, normalized)
	}
	return out
}
