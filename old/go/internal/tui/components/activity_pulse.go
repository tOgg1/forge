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

// ActivityLevel represents the intensity of recent activity.
type ActivityLevel int

const (
	ActivityNone   ActivityLevel = iota // No recent activity
	ActivityLow                         // Some activity in past 5 minutes
	ActivityMedium                      // Activity in past 1 minute
	ActivityHigh                        // Activity in past 10 seconds
)

// ActivityPulse holds data for rendering an activity indicator.
type ActivityPulse struct {
	// RecentEvents tracks timestamps of recent state changes (last 10)
	RecentEvents []time.Time
	// CurrentState is the agent's current state
	CurrentState models.AgentState
	// LastActivity is the most recent activity timestamp
	LastActivity *time.Time
}

// NewActivityPulse creates a new ActivityPulse from state change data.
func NewActivityPulse(events []time.Time, state models.AgentState, lastActivity *time.Time) ActivityPulse {
	return ActivityPulse{
		RecentEvents: events,
		CurrentState: state,
		LastActivity: lastActivity,
	}
}

// Level calculates the activity level based on recent events.
func (ap ActivityPulse) Level() ActivityLevel {
	if ap.LastActivity == nil {
		return ActivityNone
	}

	elapsed := time.Since(*ap.LastActivity)
	switch {
	case elapsed < 10*time.Second:
		return ActivityHigh
	case elapsed < 1*time.Minute:
		return ActivityMedium
	case elapsed < 5*time.Minute:
		return ActivityLow
	default:
		return ActivityNone
	}
}

// EventsInWindow counts events within the specified duration from now.
func (ap ActivityPulse) EventsInWindow(window time.Duration) int {
	cutoff := time.Now().Add(-window)
	count := 0
	for _, t := range ap.RecentEvents {
		if t.After(cutoff) {
			count++
		}
	}
	return count
}

// RenderActivityPulse renders a visual activity indicator.
// Format: "●●●○○" (filled dots for recent activity, empty for older)
// or sparkline: "▁▂▃▅▇" based on activity intensity.
func RenderActivityPulse(styleSet styles.Styles, pulse ActivityPulse) string {
	level := pulse.Level()
	return renderPulseDots(styleSet, level, pulse.CurrentState)
}

// renderPulseDots renders activity as a series of dots.
func renderPulseDots(styleSet styles.Styles, level ActivityLevel, state models.AgentState) string {
	// Select style based on state and activity
	var activeStyle, inactiveStyle lipgloss.Style

	switch state {
	case models.AgentStateWorking:
		activeStyle = styleSet.Success.Copy().Bold(true)
	case models.AgentStateError:
		activeStyle = styleSet.Error.Copy().Bold(true)
	case models.AgentStatePaused:
		activeStyle = styleSet.Warning
	case models.AgentStateRateLimited:
		activeStyle = styleSet.Warning
	default:
		activeStyle = styleSet.Accent
	}
	inactiveStyle = styleSet.Muted

	// Render dots based on level
	const totalDots = 5
	var activeDots int
	switch level {
	case ActivityHigh:
		activeDots = 5
	case ActivityMedium:
		activeDots = 3
	case ActivityLow:
		activeDots = 1
	default:
		activeDots = 0
	}

	var parts []string
	for i := 0; i < totalDots; i++ {
		if i < activeDots {
			parts = append(parts, activeStyle.Render("●"))
		} else {
			parts = append(parts, inactiveStyle.Render("○"))
		}
	}

	return strings.Join(parts, "")
}

// RenderActivitySparkline renders a mini sparkline based on event frequency.
// Uses block characters: ▁▂▃▄▅▆▇█
func RenderActivitySparkline(styleSet styles.Styles, pulse ActivityPulse, width int) string {
	if width <= 0 {
		width = 8
	}

	// Divide time into buckets and count events per bucket
	bucketDuration := 30 * time.Second
	buckets := make([]int, width)
	now := time.Now()

	for _, eventTime := range pulse.RecentEvents {
		elapsed := now.Sub(eventTime)
		bucketIndex := int(elapsed / bucketDuration)
		if bucketIndex >= 0 && bucketIndex < width {
			// Reverse index so most recent is on the right
			buckets[width-1-bucketIndex]++
		}
	}

	// Find max for normalization
	maxCount := 1
	for _, count := range buckets {
		if count > maxCount {
			maxCount = count
		}
	}

	// Render sparkline
	blocks := []rune{'▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'}
	var result strings.Builder

	for _, count := range buckets {
		var idx int
		if count == 0 {
			idx = 0
		} else {
			idx = (count * (len(blocks) - 1)) / maxCount
			if idx >= len(blocks) {
				idx = len(blocks) - 1
			}
		}
		result.WriteRune(blocks[idx])
	}

	// Style based on current state
	var style lipgloss.Style
	switch pulse.CurrentState {
	case models.AgentStateWorking:
		style = styleSet.Success
	case models.AgentStateError:
		style = styleSet.Error
	case models.AgentStatePaused, models.AgentStateRateLimited:
		style = styleSet.Warning
	default:
		style = styleSet.Muted
	}

	return style.Render(result.String())
}

// RenderCompactPulse renders a minimal pulse indicator for tight spaces.
// Shows: "◆" (active), "◇" (inactive), with color based on state.
func RenderCompactPulse(styleSet styles.Styles, pulse ActivityPulse) string {
	level := pulse.Level()

	var icon string
	var style lipgloss.Style

	switch pulse.CurrentState {
	case models.AgentStateWorking:
		style = styleSet.Success
	case models.AgentStateError:
		style = styleSet.Error
	case models.AgentStatePaused:
		style = styleSet.Warning
	default:
		style = styleSet.Muted
	}

	if level >= ActivityMedium {
		icon = "◆"
		style = style.Copy().Bold(true)
	} else if level >= ActivityLow {
		icon = "◇"
	} else {
		icon = "·"
		style = styleSet.Muted
	}

	return style.Render(icon)
}

// RenderActivityLine renders a full activity line with label.
// Format: "Activity: ●●●○○ (2 events/min)"
func RenderActivityLine(styleSet styles.Styles, pulse ActivityPulse) string {
	dots := RenderActivityPulse(styleSet, pulse)
	eventsPerMin := pulse.EventsInWindow(1 * time.Minute)

	label := styleSet.Muted.Render("Activity:")
	rate := styleSet.Muted.Render(fmt.Sprintf("(%d/min)", eventsPerMin))

	return fmt.Sprintf("%s %s %s", label, dots, rate)
}
