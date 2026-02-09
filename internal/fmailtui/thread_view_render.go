package fmailtui

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

func (v *threadView) renderHeader(width int, palette styles.Theme) string {
	topic := strings.TrimSpace(v.topic)
	if topic == "" {
		topic = "(no topic)"
	}
	participants := v.participantCount(topic)
	left := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render(topic)

	total := v.total
	if total <= 0 {
		total = len(v.allMsgs)
	}
	loaded := len(v.allMsgs)
	countLabel := fmt.Sprintf("%d messages", total)
	if total > 0 && loaded > 0 && loaded < total {
		countLabel = fmt.Sprintf("%d/%d messages", loaded, total)
	}
	right := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(fmt.Sprintf("%s  %d participants", countLabel, participants))

	gap := maxInt(1, width-lipgloss.Width(left)-lipgloss.Width(right))
	return truncateVis(left+strings.Repeat(" ", gap)+right, width)
}

func (v *threadView) renderMeta(width int, palette styles.Theme) string {
	mode := "threaded"
	if v.mode == threadModeFlat {
		mode = "flat"
	}
	marker := strings.TrimSpace(v.readMarkers[v.topic])
	meta := fmt.Sprintf("mode:%s  j/k move  ctrl+d/u page  g/G top/bot  Enter expand/collapse  f toggle  [ ] topic", mode)
	if marker != "" {
		meta = meta + "  read:" + shortID(marker)
	}
	return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(truncateVis(meta, width))
}

func (v *threadView) renderRows(width, height int, palette styles.Theme) string {
	if height <= 0 {
		return ""
	}
	if len(v.rows) == 0 {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("No messages")
	}

	v.ensureVisible()
	start := clampInt(v.top, 0, maxInt(0, len(v.rows)-1))
	remaining := height
	out := make([]string, 0, height)
	mapper := styles.NewAgentColorMapper()
	msgStyles := styles.NewMessageStyles(palette, mapper)

	for i := start; i < len(v.rows) && remaining > 0; i++ {
		row := v.rows[i]
		if row.groupGap && len(out) > 0 && remaining > 0 {
			out = append(out, "")
			remaining--
			if remaining <= 0 {
				break
			}
		}

		selected := i == v.selected
		unread := v.isUnread(row.msg.ID)
		lines := v.renderRowCard(row, width, selected, unread, palette, mapper, msgStyles)
		if len(lines) > remaining {
			lines = lines[:remaining]
		}
		out = append(out, lines...)
		remaining -= len(lines)
	}

	if v.pendingNew > 0 && !v.isAtBottom() && len(out) > 0 {
		indicator := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Render(fmt.Sprintf("New messages (%d) - press G", v.pendingNew))
		out[len(out)-1] = truncateVis(indicator, width)
	}

	return strings.Join(out, "\n")
}

func (v *threadView) renderRowCard(row threadRow, width int, selected bool, unread bool, palette styles.Theme, mapper *styles.AgentColorMapper, msgStyles styles.MessageStyles) []string {
	agentColor := mapper.ColorCode(row.msg.From)
	borderColor := agentColor
	if selected {
		borderColor = palette.Chrome.SelectedItem
	}

	timeLabel := relativeTime(row.msg.Time, v.now)
	if selected {
		timeLabel = row.msg.Time.UTC().Format(time.RFC3339)
	}

	agentName := mapper.Foreground(row.msg.From).Render(strings.TrimSpace(row.msg.From))
	indent := row.connector
	if row.overflow {
		indent = indent + "... "
	}
	unreadDot := ""
	if unread {
		unreadDot = msgStyles.RenderUnreadIndicator(true) + " "
	}

	header := fmt.Sprintf("%s%s (%s)", indent+unreadDot+agentName, "", timeLabel)
	content := []string{header}

	bodyWidth := maxInt(10, width-8-lipgloss.Width(indent))
	bodyLines := renderBodyLines(messageBodyString(row.msg.Body), bodyWidth, palette)
	if row.truncated {
		limit := minInt(threadMaxBodyLines, len(bodyLines))
		bodyLines = bodyLines[:limit]
	}

	bodyPrefix := strings.Repeat(" ", lipgloss.Width(indent))
	for _, line := range bodyLines {
		content = append(content, bodyPrefix+line)
	}
	if row.truncated {
		content = append(content, bodyPrefix+fmt.Sprintf("... [show more] (%d lines)", row.hiddenLines))
	}

	footerParts := make([]string, 0, 4)
	if badge := msgStyles.RenderPriorityBadge(row.msg.Priority); badge != "" {
		footerParts = append(footerParts, badge)
	}
	if tags := msgStyles.RenderTagPills(row.msg.Tags); tags != "" {
		footerParts = append(footerParts, tags)
	}
	if row.replyTo != "" {
		reply := "↩ " + shortID(row.replyTo)
		if row.crossTarget != "" {
			reply = reply + " from " + row.crossTarget
		}
		footerParts = append(footerParts, reply)
	}
	if len(footerParts) > 0 {
		content = append(content, bodyPrefix+strings.Join(footerParts, "  "))
	}
	if selected {
		details := fmt.Sprintf("id:%s", row.msg.ID)
		if host := strings.TrimSpace(row.msg.Host); host != "" {
			details += "  host:" + host
		}
		content = append(content, bodyPrefix+details)
	}

	card := strings.Join(content, "\n")
	cardStyle := lipgloss.NewStyle().BorderLeft(true).BorderStyle(lipgloss.NormalBorder()).BorderForeground(lipgloss.Color(borderColor)).PaddingLeft(1)
	if selected {
		cardStyle = cardStyle.Bold(true)
	}
	return strings.Split(cardStyle.Width(maxInt(0, width)).Render(card), "\n")
}
