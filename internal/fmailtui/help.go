package fmailtui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmailtui/styles"
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
		Border(styles.BorderStyleForTheme(palette)).
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
			{key: "Ctrl+B", desc: "open bookmarks"},
			{key: "Ctrl+T", desc: "cycle theme"},
			{key: "Ctrl+R", desc: "refresh view"},
			{key: "Ctrl+Z", desc: "toggle zen layout"},
			{key: "Ctrl+N", desc: "open notifications"},
			{key: "Tab", desc: "cycle pane focus"},
			{key: "Ctrl+W h/j/k/l", desc: "move pane focus"},
			{key: "Ctrl+W +/-", desc: "adjust split ratio"},
			{key: "Ctrl+W o", desc: "expand focused pane"},
			{key: "| / Ctrl+\\", desc: "split resize / collapse"},
			{key: "Ctrl+G", desc: "cycle dashboard grid"},
			{key: "Ctrl+1..4", desc: "cycle dashboard slot"},
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
	case ViewTimeline:
		return []helpSection{
			global,
			{title: "Timeline", items: []helpItem{
				{key: "s", desc: "toggle chronological/swim-lane"},
				{key: "+ / -", desc: "zoom time window"},
				{key: "h/l or ←/→", desc: "pan window"},
				{key: "f", desc: "set filter query"},
				{key: "t", desc: "jump to time/date"},
				{key: "n", desc: "jump to now"},
				{key: "Enter", desc: "toggle detail popup"},
				{key: "o", desc: "open selected in thread view"},
				{key: "b", desc: "toggle bookmark"},
			}},
		}
	case ViewBookmarks:
		return []helpSection{
			global,
			{title: "Bookmarks", items: []helpItem{
				{key: "j/k", desc: "move selection"},
				{key: "Enter", desc: "open in thread view"},
				{key: "e", desc: "edit note"},
				{key: "d", desc: "delete bookmark"},
				{key: "x", desc: "export markdown"},
				{key: "/", desc: "filter"},
				{key: "s", desc: "cycle sort"},
			}},
		}
	case ViewStats:
		return []helpSection{
			global,
			{title: "Stats", items: []helpItem{
				{key: "r", desc: "refresh"},
				{key: "[ / ]", desc: "adjust time range"},
				{key: "h/l or ←/→", desc: "pan window"},
			}},
		}
	case ViewHeatmap:
		return []helpSection{
			global,
			{title: "Heatmap", items: []helpItem{
				{key: "Arrow keys", desc: "move cell selection"},
				{key: "Enter", desc: "open in Timeline view"},
				{key: "t", desc: "toggle agent/topic mode"},
				{key: "s", desc: "cycle sort"},
				{key: "[ / ]", desc: "adjust time range"},
				{key: "h/l or ←/→", desc: "pan window"},
			}},
		}
	case ViewGraph:
		return []helpSection{
			global,
			{title: "Graph", items: []helpItem{
				{key: "[ / ]", desc: "adjust time range"},
				{key: "h/l", desc: "pan time window"},
				{key: "t", desc: "toggle topic overlay"},
				{key: "c", desc: "toggle clustering"},
				{key: "Arrow keys", desc: "pan layout"},
				{key: "Tab", desc: "next node"},
				{key: "Enter", desc: "toggle details panel"},
			}},
		}
	case ViewReplay:
		return []helpSection{
			global,
			{title: "Replay", items: []helpItem{
				{key: "Space", desc: "play/pause"},
				{key: "←/→", desc: "skip 1 message"},
				{key: "Shift+←/→", desc: "seek ±1 minute"},
				{key: "Home/End", desc: "jump start/end"},
				{key: "1-4", desc: "speed 1x/5x/10x/50x"},
				{key: "t", desc: "toggle feed/timeline"},
				{key: "m", desc: "set mark"},
				{key: "' + letter", desc: "jump to mark"},
				{key: "e", desc: "export markdown"},
			}},
		}
	case ViewNotify:
		return []helpSection{
			global,
			{title: "Notifications", items: []helpItem{
				{key: "j/k", desc: "move selection"},
				{key: "Tab", desc: "switch notifications/rules"},
				{key: "Enter", desc: "open in thread view"},
				{key: "x / c", desc: "dismiss one / clear all"},
				{key: "n / e / d", desc: "new, edit, delete rule"},
				{key: "Space", desc: "toggle selected rule"},
			}},
		}
	default:
		return []helpSection{global}
	}
}
