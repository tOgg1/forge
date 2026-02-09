package fmailtui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
)

type helpItem struct {
	key  string
	desc string
}

type helpSection struct {
	title string
	items []helpItem
}

func (m *Model) renderHelpOverlay(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	palette := themePalette(theme)

	sections := helpForView(m.activeViewID())
	lines := make([]string, 0, 64)
	head := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render("Help")
	lines = append(lines, head, "")

	keyStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Base.Accent))
	for _, sec := range sections {
		if strings.TrimSpace(sec.title) != "" {
			lines = append(lines, lipgloss.NewStyle().Bold(true).Render(sec.title))
		}
		for _, it := range sec.items {
			lines = append(lines, "  "+keyStyle.Render(it.key)+"  "+it.desc)
		}
		lines = append(lines, "")
	}

	lines = append(lines, lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Dismiss: ? or Esc"))
	content := strings.Join(lines, "\n")

	panelWidth := minInt(maxInt(50, width-10), 96)
	panel := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(palette.Base.Border)).
		Background(lipgloss.Color(palette.Base.Background)).
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Padding(1, 2).
		Width(panelWidth)

	return lipgloss.Place(width, height, lipgloss.Center, lipgloss.Center, panel.Render(content))
}

func helpForView(id ViewID) []helpSection {
	global := helpSection{
		title: "Global",
		items: []helpItem{
			{key: "q / Ctrl+C", desc: "quit"},
			{key: "Esc", desc: "back"},
			{key: ":", desc: "quick send"},
			{key: "n", desc: "new message"},
			{key: "?", desc: "toggle help"},
		},
	}

	switch id {
	case ViewTopics:
		return []helpSection{
			global,
			{title: "Topics", items: []helpItem{
				{key: "j/k", desc: "move selection"},
				{key: "Enter", desc: "open thread"},
				{key: "d", desc: "toggle topics/DMs"},
				{key: "s", desc: "cycle sort"},
				{key: "*", desc: "star topic"},
				{key: "/", desc: "filter"},
			}},
		}
	case ViewThread:
		return []helpSection{
			global,
			{title: "Thread", items: []helpItem{
				{key: "j/k", desc: "move selection"},
				{key: "Ctrl+D / Ctrl+U", desc: "page"},
				{key: "g/G", desc: "top/bottom"},
				{key: "Enter", desc: "expand/collapse"},
				{key: "f", desc: "toggle flat/threaded"},
				{key: "r / R", desc: "reply / DM reply"},
			}},
		}
	case ViewAgents:
		return []helpSection{
			global,
			{title: "Agents", items: []helpItem{
				{key: "j/k", desc: "move selection"},
				{key: "Enter", desc: "toggle history"},
				{key: "s", desc: "cycle sort"},
				{key: "[ / ]", desc: "adjust activity window"},
				{key: "/", desc: "filter"},
			}},
		}
	default:
		return []helpSection{global}
	}
}
