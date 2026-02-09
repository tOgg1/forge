package styles

import "github.com/charmbracelet/lipgloss"

const (
	// LayoutGap is the default space between columns.
	LayoutGap = 2

	// LayoutOuterPadding keeps panels off screen edges.
	LayoutOuterPadding = 1

	// LayoutInnerPadding is the default panel content padding.
	LayoutInnerPadding = 1

	// LayoutPanelMargin separates stacked panels.
	LayoutPanelMargin = 1
)

const (
	minInboxWidth   = 18
	maxInboxWidth   = 26
	minThreadsWidth = 24
	maxThreadsWidth = 36
	minMessageWidth = 40
)

// ColumnWidths defines responsive column widths for the fmail TUI.
type ColumnWidths struct {
	Inbox   int
	Threads int
	Message int
}

// ComputeColumnWidths returns responsive widths for inbox/thread/message columns.
func ComputeColumnWidths(totalWidth int) ColumnWidths {
	if totalWidth <= 0 {
		return ColumnWidths{}
	}

	if totalWidth < 78 {
		return narrowColumns(totalWidth)
	}

	inbox := clampInt(totalWidth/5, minInboxWidth, maxInboxWidth)
	threads := clampInt(totalWidth/3, minThreadsWidth, maxThreadsWidth)
	message := totalWidth - inbox - threads - (LayoutGap * 2)

	if message < minMessageWidth {
		needed := minMessageWidth - message

		// Prefer shrinking thread list first.
		shrinkThreads := minInt(needed, threads-minThreadsWidth)
		threads -= shrinkThreads
		needed -= shrinkThreads

		if needed > 0 {
			shrinkInbox := minInt(needed, inbox-minInboxWidth)
			inbox -= shrinkInbox
			needed -= shrinkInbox
		}

		message = totalWidth - inbox - threads - (LayoutGap * 2)
		if needed > 0 || message < minMessageWidth {
			return narrowColumns(totalWidth)
		}
	}

	return ColumnWidths{Inbox: inbox, Threads: threads, Message: message}
}

func narrowColumns(totalWidth int) ColumnWidths {
	threads := clampInt(totalWidth/3, minThreadsWidth, maxThreadsWidth)
	message := totalWidth - threads - LayoutGap
	if message < minThreadsWidth {
		threads = 0
		message = totalWidth
	}
	if message < 0 {
		message = 0
	}
	return ColumnWidths{Inbox: 0, Threads: threads, Message: message}
}

// PanelStyle returns a focused/unfocused border style for panes.
func PanelStyle(theme Theme, focused bool) lipgloss.Style {
	return lipgloss.NewStyle().
		BorderStyle(panelBorderStyle(theme)).
		BorderForeground(lipgloss.Color(panelBorderColor(theme, focused))).
		Padding(LayoutInnerPadding).
		MarginBottom(LayoutPanelMargin)
}

// DividerStyle returns the divider style between sections.
func DividerStyle(theme Theme) lipgloss.Style {
	return lipgloss.NewStyle().Foreground(lipgloss.Color(theme.Borders.Divider))
}

func panelBorderColor(theme Theme, focused bool) string {
	if focused {
		return theme.Borders.ActivePane
	}
	return theme.Borders.InactivePane
}

func panelBorderStyle(theme Theme) lipgloss.Border {
	switch theme.BorderStyle {
	case "double":
		return lipgloss.DoubleBorder()
	case "sharp":
		return lipgloss.NormalBorder()
	case "hidden":
		return lipgloss.HiddenBorder()
	default:
		return lipgloss.RoundedBorder()
	}
}

func clampInt(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}
