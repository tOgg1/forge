// Package components provides reusable TUI components.
package components

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/models"
	"github.com/tOgg1/forge/internal/tui/styles"
)

const (
	maxReasonLength = 44
	agentCardWidth  = 60
)

// AgentCard contains data needed to render an agent card.
type AgentCard struct {
	Name          string
	Type          models.AgentType
	Model         string
	Profile       string
	State         models.AgentState
	Confidence    models.StateConfidence
	Reason        string
	QueueLength   int
	LastActivity  *time.Time
	CooldownUntil *time.Time
	RecentEvents  []time.Time              // Timestamps of recent state changes for activity pulse
	UsageMetrics  *models.UsageMetrics     // Usage metrics from adapter
	ClaimSummary  *models.FileClaimSummary // File claim status from Agent Mail
}

// RenderAgentCard renders a compact agent summary card.
func RenderAgentCard(styleSet styles.Styles, card AgentCard, focused bool, selected bool, selectionMode bool) string {
	highlighted := focused || selected
	headerStyle := styleSet.Muted
	textStyle := styleSet.Muted
	mutedStyle := styleSet.Muted
	if highlighted {
		headerStyle = styleSet.Accent
		textStyle = styleSet.Text
	}

	headerText := defaultIfEmpty(card.Name, "Agent")
	if selectionMode {
		box := "[ ]"
		if selected {
			box = "[x]"
		}
		headerText = fmt.Sprintf("%s %s", box, headerText)
	}
	header := headerStyle.Render(headerText)
	typeLine := textStyle.Render(fmt.Sprintf("Type: %s  Model: %s", formatAgentType(card.Type), defaultIfEmpty(card.Model, "--")))
	profileLine := mutedStyle.Render(fmt.Sprintf("Profile: %s", defaultIfEmpty(card.Profile, "--")))

	reason := strings.TrimSpace(card.Reason)
	if reason == "" {
		reason = "No reason reported"
	}
	reason = truncate(reason, maxReasonLength)
	stateBadge := RenderAgentStateBadge(styleSet, card.State)
	stateLabel := "State:"
	if !highlighted {
		stateLabel = styleSet.Muted.Render("State:")
	}
	stateLine := fmt.Sprintf("%s %s", stateLabel, stateBadge)
	reasonLine := mutedStyle.Render(fmt.Sprintf("Why: %s", reason))
	cooldownLine := renderCooldownLine(styleSet, card.CooldownUntil)
	confidenceLine := renderConfidenceLine(styleSet, card.Confidence)

	actionsLine := ""
	if focused && !selectionMode {
		actionsLine = styleSet.Info.Render("Actions: P pause | R restart | V view")
	}

	queueValue := "--"
	if card.QueueLength >= 0 {
		queueValue = fmt.Sprintf("%d", card.QueueLength)
	}
	queueLine := textStyle.Render(fmt.Sprintf("Queue: %s  Last: %s", queueValue, formatLastActivity(card.LastActivity)))

	// Activity pulse indicator
	pulse := NewActivityPulse(card.RecentEvents, card.State, card.LastActivity)
	activityLine := RenderActivityLine(styleSet, pulse)

	lines := []string{
		header,
		typeLine,
		profileLine,
		stateLine,
		reasonLine,
	}
	if cooldownLine != "" {
		lines = append(lines, cooldownLine)
	}
	lines = append(lines, confidenceLine, activityLine, queueLine)

	// Usage summary line (if available)
	usageLine := RenderUsageSummaryLine(styleSet, card.UsageMetrics)
	if usageLine != "" {
		lines = append(lines, usageLine)
	}

	// File claims line (if available)
	claimsLine := RenderFileClaimsLine(styleSet, card.ClaimSummary)
	if claimsLine != "" {
		lines = append(lines, claimsLine)
	}

	if actionsLine != "" {
		lines = append(lines, actionsLine)
	}

	content := strings.Join(lines, "\n")

	cardStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		Padding(0, 1).
		Width(agentCardWidth).
		MaxWidth(agentCardWidth)
	if selected {
		cardStyle = cardStyle.
			BorderForeground(lipgloss.Color(styleSet.Theme.Tokens.Focus)).
			Background(lipgloss.Color(styleSet.Theme.Tokens.Panel))
	} else if focused {
		cardStyle = cardStyle.
			BorderForeground(lipgloss.Color(styleSet.Theme.Tokens.Focus))
	} else {
		cardStyle = cardStyle.
			BorderForeground(lipgloss.Color(styleSet.Theme.Tokens.Border)).
			Foreground(lipgloss.Color(styleSet.Theme.Tokens.TextMuted))
	}

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

func renderCooldownLine(styleSet styles.Styles, cooldownUntil *time.Time) string {
	if cooldownUntil == nil || cooldownUntil.IsZero() {
		return ""
	}
	remaining := time.Until(*cooldownUntil)
	if remaining <= 0 {
		return styleSet.Muted.Render("Cooldown: expired")
	}
	return styleSet.Warning.Render(fmt.Sprintf("Cooldown: %s", formatDuration(remaining)))
}

func formatDuration(value time.Duration) string {
	if value < time.Second {
		return "<1s"
	}
	if value < time.Minute {
		return value.Round(time.Second).String()
	}
	if value < time.Hour {
		rounded := value.Round(time.Second)
		minutes := int(rounded.Minutes())
		seconds := int(rounded.Seconds()) % 60
		return fmt.Sprintf("%dm%02ds", minutes, seconds)
	}
	return value.Round(time.Minute).String()
}

// RenderFileClaimsLine renders a summary line showing file claim status.
// Returns empty string if no claims are present.
func RenderFileClaimsLine(styleSet styles.Styles, summary *models.FileClaimSummary) string {
	if summary == nil || summary.TotalClaims == 0 {
		return ""
	}

	prefix := styleSet.Muted.Render("Claims:")

	// Build the claims part
	var claimsPart string
	if summary.ExclusiveClaims > 0 && summary.SharedClaims > 0 {
		claimsPart = fmt.Sprintf("%d exclusive, %d shared", summary.ExclusiveClaims, summary.SharedClaims)
	} else if summary.ExclusiveClaims > 0 {
		claimsPart = fmt.Sprintf("%d exclusive", summary.ExclusiveClaims)
	} else {
		claimsPart = fmt.Sprintf("%d shared", summary.SharedClaims)
	}

	// Style based on conflict status
	var claimsStyled string
	if summary.HasConflicts {
		claimsStyled = styleSet.Error.Render(fmt.Sprintf("%s [%d conflicts]", claimsPart, summary.Conflicts))
	} else {
		claimsStyled = styleSet.Success.Render(claimsPart)
	}

	return fmt.Sprintf("%s %s", prefix, claimsStyled)
}

// RenderFileClaimsBadge renders a compact badge for file claims.
// Returns empty string if no claims are present.
func RenderFileClaimsBadge(styleSet styles.Styles, summary *models.FileClaimSummary) string {
	if summary == nil || summary.TotalClaims == 0 {
		return ""
	}

	if summary.HasConflicts {
		return styleSet.Error.Render(fmt.Sprintf("!%d", summary.Conflicts))
	}

	return styleSet.Success.Render(fmt.Sprintf("F%d", summary.TotalClaims))
}
