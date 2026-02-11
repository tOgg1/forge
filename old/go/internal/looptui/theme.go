package looptui

import "strings"

type tuiPalette struct {
	Name       string
	Background string
	Panel      string
	PanelAlt   string
	Text       string
	TextMuted  string
	Border     string
	Accent     string
	Focus      string
	Success    string
	Warning    string
	Error      string
	Info       string
}

var paletteOrder = []string{"default", "high-contrast", "ocean", "sunset"}

var palettes = map[string]tuiPalette{
	"default": {
		Name:       "default",
		Background: "#0B0F14",
		Panel:      "#121821",
		PanelAlt:   "#10161E",
		Text:       "#E6EDF3",
		TextMuted:  "#8B9AAE",
		Border:     "#223043",
		Accent:     "#5B8DEF",
		Focus:      "#7AA2F7",
		Success:    "#3FB950",
		Warning:    "#D29922",
		Error:      "#F85149",
		Info:       "#58A6FF",
	},
	"high-contrast": {
		Name:       "high-contrast",
		Background: "#000000",
		Panel:      "#0A0A0A",
		PanelAlt:   "#000000",
		Text:       "#FFFFFF",
		TextMuted:  "#C0C0C0",
		Border:     "#FFFFFF",
		Accent:     "#00A2FF",
		Focus:      "#FFD400",
		Success:    "#00FF5A",
		Warning:    "#FFB000",
		Error:      "#FF4040",
		Info:       "#66CCFF",
	},
	"ocean": {
		Name:       "ocean",
		Background: "#07121A",
		Panel:      "#0C1B27",
		PanelAlt:   "#102230",
		Text:       "#D8ECF7",
		TextMuted:  "#78A2B8",
		Border:     "#1E4A61",
		Accent:     "#3DD3FF",
		Focus:      "#71E0FF",
		Success:    "#55E39F",
		Warning:    "#FFC857",
		Error:      "#FF6B6B",
		Info:       "#4CC9F0",
	},
	"sunset": {
		Name:       "sunset",
		Background: "#140C10",
		Panel:      "#201218",
		PanelAlt:   "#28171F",
		Text:       "#F6E7E4",
		TextMuted:  "#C89A90",
		Border:     "#5D2E3F",
		Accent:     "#FF8C5A",
		Focus:      "#FFB077",
		Success:    "#7ED957",
		Warning:    "#FFD166",
		Error:      "#FF5D73",
		Info:       "#7FD1FF",
	},
}

func resolvePalette(name string) tuiPalette {
	trimmed := strings.ToLower(strings.TrimSpace(name))
	if palette, ok := palettes[trimmed]; ok {
		return palette
	}
	return palettes["default"]
}

func cyclePalette(current string, delta int) tuiPalette {
	if len(paletteOrder) == 0 {
		return palettes["default"]
	}
	current = strings.ToLower(strings.TrimSpace(current))
	idx := 0
	for i, candidate := range paletteOrder {
		if candidate == current {
			idx = i
			break
		}
	}
	idx += delta
	for idx < 0 {
		idx += len(paletteOrder)
	}
	idx %= len(paletteOrder)
	return resolvePalette(paletteOrder[idx])
}
