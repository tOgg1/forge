package fmailtui

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

func (m *Model) renderHeader() string {
	palette, ok := styles.Themes[string(m.theme)]
	if !ok {
		palette = styles.DefaultTheme
	}

	style := lipgloss.NewStyle().
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Background(lipgloss.Color(palette.Chrome.Header)).
		Bold(true).
		Padding(0, 1)

	left := "fmail TUI"
	center := fmt.Sprintf("project: %s", m.projectID)
	right := connectionStatus(m.root, m.forgedClient != nil, m.forgedErr)
	line := joinHeader(left, center, right, m.width)
	return style.Width(maxInt(0, m.width)).Render(line)
}

func (m *Model) renderFooter() string {
	palette, ok := styles.Themes[string(m.theme)]
	if !ok {
		palette = styles.DefaultTheme
	}

	style := lipgloss.NewStyle().
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Background(lipgloss.Color(palette.Chrome.Footer)).
		Padding(0, 1)

	base := "[T]opics [A]gents [/]Search [L]ive tail [?]Help q Quit"
	if m.showHelp {
		base = base + "  (tab focus, arrows scroll/select, End/G resume feed)"
	}
	return style.Width(maxInt(0, m.width)).Render(truncate(base, maxInt(0, m.width-2)))
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
