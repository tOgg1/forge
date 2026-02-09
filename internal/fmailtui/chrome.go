package fmailtui

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

func (m *Model) renderHeader() string {
	palette, ok := styles.Themes[string(m.theme)]
	if !ok {
		palette = styles.DefaultTheme
	}

	now := m.status.now
	if now.IsZero() {
		now = time.Now().UTC()
	}

	fg := lipgloss.Color(palette.Base.Foreground)
	bg := lipgloss.Color(palette.Chrome.Header)
	if !m.flashUntil.IsZero() && now.Before(m.flashUntil) {
		// Flash: invert-ish colors for alerting notifications.
		fg = lipgloss.Color(palette.Chrome.Header)
		bg = lipgloss.Color(palette.Base.Foreground)
	}

	style := lipgloss.NewStyle().
		Foreground(fg).
		Background(bg).
		Bold(true).
		Padding(0, 1)

	left := "fmail TUI"
	if crumb := strings.TrimSpace(m.breadcrumb()); crumb != "" {
		left = left + " | " + crumb
	}
	center := fmt.Sprintf("project: %s", m.projectID)
	right := fmt.Sprintf("agent: %s  %s", m.selfAgent, now.Format("15:04"))
	line := joinHeader(left, center, right, m.width)
	return style.Width(maxInt(0, m.width)).Render(line)
}

func (m *Model) renderFooter() string {
	return m.renderStatusBar()
}

func connectionStatus(root string, hasClient bool, dialErr error) string {
	if hasClient || forgedSocketExists(root) {
		return "connected"
	}
	if dialErr != nil {
		return "disconnected"
	}
	return "polling"
}

func joinHeader(left, center, right string, width int) string {
	left = strings.TrimSpace(left)
	center = strings.TrimSpace(center)
	right = strings.TrimSpace(right)
	if width <= 0 {
		return left
	}

	space := width - lipgloss.Width(left) - lipgloss.Width(center) - lipgloss.Width(right)
	if space < 2 {
		line := left
		if right != "" {
			line = left + "  " + right
		}
		return truncateVis(line, width)
	}

	leftGap := space / 2
	rightGap := space - leftGap
	return truncateVis(left+strings.Repeat(" ", leftGap)+center+strings.Repeat(" ", rightGap)+right, width)
}

func forgedSocketExists(root string) bool {
	if strings.TrimSpace(root) == "" {
		return false
	}
	path := filepath.Join(root, ".fmail", "forged.sock")
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return !info.IsDir()
}
