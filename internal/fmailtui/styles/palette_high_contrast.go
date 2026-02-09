package styles

// HighContrastTheme favors legibility on low-quality terminals.
var HighContrastTheme = Theme{
	Name: "high-contrast",
	Base: BaseColors{
		Background: "16",
		Foreground: "231",
		Muted:      "250",
		Accent:     "51",
		Border:     "231",
	},
	Message: MessageColors{
		Own:    "87",
		Other:  "225",
		System: "229",
	},
	Priority: PriorityColors{
		High:   "196",
		Normal: "231",
		Low:    "250",
	},
	Status: StatusColors{
		Online: "46",
		Recent: "226",
		Stale:  "244",
	},
	Chrome: ChromeColors{
		Header:       "117",
		Footer:       "159",
		Breadcrumb:   "195",
		SelectedItem: "51",
		Scrollbar:    "252",
	},
	Borders: BorderColors{
		ActivePane:   "231",
		InactivePane: "250",
		Divider:      "248",
	},
}
