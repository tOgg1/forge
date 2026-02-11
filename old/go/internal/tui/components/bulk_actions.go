// Package components provides reusable TUI components.
package components

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/tui/styles"
)

// RenderBulkActionPanel renders the bulk action helper panel.
func RenderBulkActionPanel(styleSet styles.Styles, selectedCount int) string {
	if selectedCount <= 0 {
		return ""
	}

	title := styleSet.Accent.Render(fmt.Sprintf("Bulk actions: %d agent(s) selected", selectedCount))
	line1 := styleSet.Text.Render("[P] Pause  [R] Resume  [T] Template")
	line2 := styleSet.Text.Render("[Q] Queue  [K] Kill    [I] Interrupt")
	line3 := styleSet.Text.Render("[S] Send message to all")
	line4 := styleSet.Muted.Render("[Esc] Clear selection  [Space] Toggle")

	content := strings.Join([]string{title, line1, line2, line3, line4}, "\n")

	panel := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		Padding(0, 1).
		BorderForeground(lipgloss.Color(styleSet.Theme.Tokens.Border))

	return panel.Render(content)
}
