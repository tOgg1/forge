package styles

import "github.com/charmbracelet/lipgloss"

// BaseColors defines global UI colors.
type BaseColors struct {
	Background string
	Foreground string
	Muted      string
	Accent     string
	Border     string
}

// MessageColors defines colors for message types.
type MessageColors struct {
	Own    string
	Other  string
	System string
}

// PriorityColors defines colors for message priorities.
type PriorityColors struct {
	High   string
	Normal string
	Low    string
}

// StatusColors defines colors for agent and message freshness state.
type StatusColors struct {
	Online string
	Recent string
	Stale  string
}

// ChromeColors defines non-content UI colors.
type ChromeColors struct {
	Header       string
	Footer       string
	Breadcrumb   string
	SelectedItem string
	Scrollbar    string
}

// BorderColors defines border colors for pane state.
type BorderColors struct {
	ActivePane   string
	InactivePane string
	Divider      string
}

// Theme defines the fmail TUI style/theme tokens.
type Theme struct {
	Name        string
	BorderStyle string   // "rounded", "sharp", "double", "hidden"
	AgentPalette []string // optional override for agent identity colors (ANSI-256 codes)

	Base     BaseColors
	Message  MessageColors
	Priority PriorityColors
	Status   StatusColors
	Chrome   ChromeColors
	Borders  BorderColors
}

// Themes lists available palettes by name.
var Themes = map[string]Theme{
	"default":       DefaultTheme,
	"high-contrast": HighContrastTheme,
}

func (t Theme) baseStyle() lipgloss.Style {
	return lipgloss.NewStyle().Foreground(lipgloss.Color(t.Base.Foreground)).Background(lipgloss.Color(t.Base.Background))
}

func (t Theme) mutedStyle() lipgloss.Style {
	return lipgloss.NewStyle().Foreground(lipgloss.Color(t.Base.Muted))
}

func (t Theme) accentStyle() lipgloss.Style {
	return lipgloss.NewStyle().Foreground(lipgloss.Color(t.Base.Accent))
}

func (t Theme) borderStyle() lipgloss.Style {
	return lipgloss.NewStyle().Foreground(lipgloss.Color(t.Base.Border))
}
