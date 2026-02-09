package styles

// DefaultTheme is the baseline dark palette for fmail TUI.
var DefaultTheme = Theme{
	Name:         "default",
	BorderStyle:  "rounded",
	AgentPalette: append([]string(nil), AgentColorPalette...),
	Base: BaseColors{
		Background: "234",
		Foreground: "252",
		Muted:      "245",
		Accent:     "75",
		Border:     "240",
	},
	Message: MessageColors{
		Own:    "81",
		Other:  "147",
		System: "214",
	},
	Priority: PriorityColors{
		High:   "203",
		Normal: "252",
		Low:    "245",
	},
	Status: StatusColors{
		Online: "41",
		Recent: "220",
		Stale:  "243",
	},
	Chrome: ChromeColors{
		Header:       "111",
		Footer:       "110",
		Breadcrumb:   "109",
		SelectedItem: "75",
		Scrollbar:    "246",
	},
	Borders: BorderColors{
		ActivePane:   "75",
		InactivePane: "240",
		Divider:      "238",
	},
}
