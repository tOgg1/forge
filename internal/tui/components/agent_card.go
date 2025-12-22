// Package components provides reusable TUI components.
package components

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"

	"github.com/opencode-ai/swarm/internal/models"
	"github.com/opencode-ai/swarm/internal/tui/styles"
)

const maxReasonLength = 44

// AgentCard contains data needed to render an agent card.
type AgentCard struct {
	Name         string
	Type         models.AgentType
	Model        string
	Profile      string
	State        models.AgentState
	Confidence   models.StateConfidence
	Reason       string
	QueueLength  int
	LastActivity *time.Time
}

// RenderAgentCard renders a compact agent summary card.
func RenderAgentCard(styleSet styles.Styles, card AgentCard) string {
	header := styleSet.Accent.Render(defaultIfEmpty(card.Name, "Agent"))
	typeLine := styleSet.Text.Render(fmt.Sprintf("Type: %s  Model: %s", formatAgentType(card.Type), defaultIfEmpty(card.Model, "--")))
	profileLine := styleSet.Muted.Render(fmt.Sprintf("Profile: %s", defaultIfEmpty(card.Profile, "--")))

	reason := strings.TrimSpace(card.Reason)
	if reason == "" {
		reason = "No reason reported"
	}
	reason = truncate(reason, maxReasonLength)
	stateBadge := RenderAgentStateBadge(styleSet, card.State)
	stateLine := fmt.Sprintf("State: %s %s", stateBadge, styleSet.Muted.Render(reason))
	confidenceLine := renderConfidenceLine(styleSet, card.Confidence)

	queueValue := "--"
	if card.QueueLength >= 0 {
		queueValue = fmt.Sprintf("%d", card.QueueLength)
	}
	queueLine := styleSet.Text.Render(fmt.Sprintf("Queue: %s  Last: %s", queueValue, formatLastActivity(card.LastActivity)))

	content := strings.Join([]string{
		header,
		typeLine,
		profileLine,
		stateLine,
		confidenceLine,
		queueLine,
	}, "\n")

	cardStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		Padding(0, 1)

	return cardStyle.Render(content)
}

func formatAgentType(agentType models.AgentType) string {
	if strings.TrimSpace(string(agentType)) == "" {
		return "unknown"
	}
	return string(agentType)
}

func formatLastActivity(ts *time.Time) string {
	if ts == nil || ts.IsZero() {
		return "--"
	}
	return ts.Format("15:04:05")
}

func renderConfidenceLine(styleSet styles.Styles, confidence models.StateConfidence) string {
	label, bars, style := confidenceDescriptor(styleSet, confidence)
	prefix := styleSet.Muted.Render("Confidence:")
	return fmt.Sprintf("%s %s", prefix, style.Render(fmt.Sprintf("%s %s", label, bars)))
}

func confidenceDescriptor(styleSet styles.Styles, confidence models.StateConfidence) (string, string, lipgloss.Style) {
	switch confidence {
	case models.StateConfidenceHigh:
		return "High", "###", styleSet.Success
	case models.StateConfidenceMedium:
		return "Medium", "##-", styleSet.Warning
	case models.StateConfidenceLow:
		return "Low", "#--", styleSet.Error
	default:
		return "Unknown", "---", styleSet.Muted
	}
}
