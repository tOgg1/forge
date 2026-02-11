package fmailtui

import (
	"fmt"
	"sort"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	"github.com/tOgg1/forge/internal/fmailtui/state"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

const searchDebounce = 200 * time.Millisecond

type searchFocus int

const (
	searchFocusInput searchFocus = iota
	searchFocusSaved
	searchFocusSavePrompt
)

type searchDebounceMsg struct {
	rev int
}

type searchLoadedMsg struct {
	rev     int
	now     time.Time
	results []data.SearchResult
	err     error
	took    time.Duration
}

type searchMetaLoadedMsg struct {
	now            time.Time
	topics         []data.TopicInfo
	dms            []data.DMConversation
	agents         []fmail.AgentRecord
	saved          []state.SavedSearch
	totalTargets   int
	totalMessages  int
	metaLoadErr    error
	completionTags []string
}

type searchItem struct {
	r         data.SearchResult
	matchText string
	matches   int
}

type searchView struct {
	root     string
	self     string
	provider data.MessageProvider
	state    *state.Manager

	now time.Time

	focus searchFocus

	query  string
	cursor int
	rev    int

	searching bool
	lastErr   error
	took      time.Duration

	results  []data.SearchResult
	selected int

	metaLoaded     bool
	totalTargets   int
	totalMessages  int
	topics         []string
	agents         []string
	saved          []state.SavedSearch
	savedSelected  int
	saveName       string
	saveNameCursor int

	completeKey       string
	completePrefix    string
	completeOptions   []string
	completeIndex     int
	lastCompletionRev int
}

func newSearchView(root, self string, provider data.MessageProvider, st *state.Manager) *searchView {
	return &searchView{
		root:     root,
		self:     strings.TrimSpace(self),
		provider: provider,
		state:    st,
		cursor:   0,
	}
}

func (v *searchView) Init() tea.Cmd {
	v.now = time.Now().UTC()
	v.loadState()
	v.cursor = clampInt(v.cursor, 0, len([]rune(v.query)))
	return tea.Batch(v.loadMetaCmd(), v.debounceCmd())
}

func (v *searchView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case searchMetaLoadedMsg:
		v.now = typed.now
		v.metaLoaded = true
		v.totalTargets = typed.totalTargets
		v.totalMessages = typed.totalMessages
		v.lastErr = typed.metaLoadErr
		v.topics = v.topics[:0]
		for _, t := range typed.topics {
			if strings.TrimSpace(t.Name) != "" {
				v.topics = append(v.topics, t.Name)
			}
		}
		sort.Strings(v.topics)
		v.agents = v.agents[:0]
		for _, a := range typed.agents {
			if strings.TrimSpace(a.Name) != "" {
				v.agents = append(v.agents, a.Name)
			}
		}
		sort.Strings(v.agents)
		v.saved = append([]state.SavedSearch(nil), typed.saved...)
		v.savedSelected = clampInt(v.savedSelected, 0, maxInt(0, len(v.saved)-1))
		return nil
	case searchDebounceMsg:
		if typed.rev != v.rev {
			return nil
		}
		return v.searchCmd(typed.rev, v.query)
	case searchLoadedMsg:
		if typed.rev != v.rev {
			return nil
		}
		v.now = typed.now
		v.searching = false
		v.lastErr = typed.err
		v.took = typed.took
		v.results = typed.results
		v.selected = clampInt(v.selected, 0, maxInt(0, len(v.results)-1))
		return nil
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *searchView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	if v.now.IsZero() {
		v.now = time.Now().UTC()
	}

	palette := themePalette(theme)
	base := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Background(lipgloss.Color(palette.Base.Background))
	panel := styles.PanelStyle(palette, true)

	innerW := maxInt(0, width-(styles.LayoutInnerPadding*2)-2)
	innerH := maxInt(1, height-(styles.LayoutInnerPadding*2)-2)

	title := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render("Search")
	status := "Enter: open  j/k: move  Tab: next  Shift+Tab: prev  s: save  Ctrl+P: saved  Esc: back"
	if v.focus == searchFocusSaved {
		status = "Saved searches: ←/→ select  Enter load  x delete  Esc back"
	}
	if v.focus == searchFocusSavePrompt {
		status = "Save as: type name + Enter  Esc cancel"
	}
	if v.searching {
		status = "Searching... " + spinnerFrame((int(v.now.UnixNano()/int64(time.Millisecond))/100)%12)
	}
	if v.lastErr != nil {
		status = "error: " + truncate(v.lastErr.Error(), maxInt(0, innerW-8))
	}
	statusLine := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(truncateVis(status, innerW))

	queryLine := v.renderQueryLine(innerW, palette)
	savedLine := v.renderSavedLine(innerW, palette)

	header := lipgloss.JoinVertical(lipgloss.Left, title, queryLine, savedLine)

	used := lipgloss.Height(header) + lipgloss.Height(statusLine) + 1
	contentH := maxInt(1, innerH-used)

	body := v.renderResults(innerW, contentH, palette)
	stats := v.renderStatsLine(innerW, palette)

	content := lipgloss.JoinVertical(lipgloss.Left, header, stats, body, statusLine)
	return base.Render(panel.Width(width).Height(height).Render(content))
}

func (v *searchView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch v.focus {
	case searchFocusSavePrompt:
		return v.handleSavePromptKey(msg)
	case searchFocusSaved:
		return v.handleSavedKey(msg)
	default:
	}

	switch msg.Type {
	case tea.KeyEsc:
		return popViewCmd()
	case tea.KeyBackspace, tea.KeyDelete:
		if v.searching {
			return nil
		}
		if v.deleteLeft() {
			return v.bumpSearch()
		}
		return nil
	case tea.KeyLeft:
		v.cursor = maxInt(0, v.cursor-1)
		return nil
	case tea.KeyRight:
		v.cursor = minInt(len([]rune(v.query)), v.cursor+1)
		return nil
	case tea.KeyHome:
		v.cursor = 0
		return nil
	case tea.KeyEnd:
		v.cursor = len([]rune(v.query))
		return nil
	case tea.KeyEnter:
		return v.openSelected()
	case tea.KeyRunes:
		if v.searching {
			return nil
		}
		v.insertRunes(string(msg.Runes))
		return v.bumpSearch()
	}

	switch msg.String() {
	case "ctrl+p":
		if len(v.saved) > 0 {
			v.focus = searchFocusSaved
			v.savedSelected = clampInt(v.savedSelected, 0, len(v.saved)-1)
		}
		return nil
	case "j", "down":
		v.selected = clampInt(v.selected+1, 0, maxInt(0, len(v.results)-1))
		return nil
	case "k", "up":
		v.selected = clampInt(v.selected-1, 0, maxInt(0, len(v.results)-1))
		return nil
	case "tab":
		if v.tryComplete(+1) {
			return v.bumpSearch()
		}
		v.selected = clampInt(v.selected+1, 0, maxInt(0, len(v.results)-1))
		return nil
	case "shift+tab":
		v.selected = clampInt(v.selected-1, 0, maxInt(0, len(v.results)-1))
		return nil
	case "s":
		if v.searching {
			return nil
		}
		v.focus = searchFocusSavePrompt
		v.saveName = ""
		v.saveNameCursor = 0
		return nil
	case "b":
		if v.state == nil || v.selected < 0 || v.selected >= len(v.results) {
			return nil
		}
		res := v.results[v.selected]
		id := strings.TrimSpace(res.Message.ID)
		topic := strings.TrimSpace(res.Topic)
		if id == "" || topic == "" {
			return nil
		}
		v.state.ToggleBookmark(id, topic)
		v.state.SaveSoon()
		return nil
	case "o":
		return v.openSelected()
	}

	return nil
}

func (v *searchView) handleSavePromptKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.focus = searchFocusInput
		v.saveName = ""
		v.saveNameCursor = 0
		return nil
	case tea.KeyEnter:
		name := strings.TrimSpace(v.saveName)
		if name == "" {
			v.focus = searchFocusInput
			return nil
		}
		parsed := parseSearchInput(v.query, time.Now().UTC())
		if v.state != nil {
			v.state.UpsertSavedSearch(name, parsed.Query)
			v.state.SaveSoon()
		}
		v.loadState()
		v.focus = searchFocusInput
		return nil
	case tea.KeyBackspace, tea.KeyDelete:
		if v.saveNameCursor <= 0 || len(v.saveName) == 0 {
			return nil
		}
		r := []rune(v.saveName)
		if v.saveNameCursor > len(r) {
			v.saveNameCursor = len(r)
		}
		if v.saveNameCursor <= 0 {
			return nil
		}
		r = append(r[:v.saveNameCursor-1], r[v.saveNameCursor:]...)
		v.saveNameCursor--
		v.saveName = string(r)
		return nil
	case tea.KeyLeft:
		v.saveNameCursor = maxInt(0, v.saveNameCursor-1)
		return nil
	case tea.KeyRight:
		v.saveNameCursor = minInt(len([]rune(v.saveName)), v.saveNameCursor+1)
		return nil
	case tea.KeyHome:
		v.saveNameCursor = 0
		return nil
	case tea.KeyEnd:
		v.saveNameCursor = len([]rune(v.saveName))
		return nil
	case tea.KeyRunes:
		r := []rune(v.saveName)
		if v.saveNameCursor > len(r) {
			v.saveNameCursor = len(r)
		}
		insert := []rune(string(msg.Runes))
		next := make([]rune, 0, len(r)+len(insert))
		next = append(next, r[:v.saveNameCursor]...)
		next = append(next, insert...)
		next = append(next, r[v.saveNameCursor:]...)
		v.saveNameCursor += len(insert)
		v.saveName = string(next)
		return nil
	}
	return nil
}

func (v *searchView) handleSavedKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.focus = searchFocusInput
		return nil
	case tea.KeyLeft:
		v.savedSelected = clampInt(v.savedSelected-1, 0, maxInt(0, len(v.saved)-1))
		return nil
	case tea.KeyRight:
		v.savedSelected = clampInt(v.savedSelected+1, 0, maxInt(0, len(v.saved)-1))
		return nil
	case tea.KeyEnter:
		if v.savedSelected < 0 || v.savedSelected >= len(v.saved) {
			return nil
		}
		q := v.saved[v.savedSelected].Query
		v.query = renderSearchQuery(q)
		v.cursor = len([]rune(v.query))
		v.focus = searchFocusInput
		return v.bumpSearch()
	}
	switch msg.String() {
	case "x":
		if v.savedSelected < 0 || v.savedSelected >= len(v.saved) {
			return nil
		}
		name := strings.TrimSpace(v.saved[v.savedSelected].Name)
		if name != "" && v.state != nil {
			v.state.DeleteSavedSearch(name)
			v.state.SaveSoon()
			v.loadState()
		}
		v.savedSelected = clampInt(v.savedSelected, 0, maxInt(0, len(v.saved)-1))
		return nil
	}
	return nil
}

func (v *searchView) openSelected() tea.Cmd {
	if v.selected < 0 || v.selected >= len(v.results) {
		return nil
	}
	r := v.results[v.selected]
	target := strings.TrimSpace(r.Topic)
	if target == "" {
		target = strings.TrimSpace(r.Message.To)
	}
	if target == "" {
		return nil
	}
	return tea.Batch(openThreadCmd(target, r.Message.ID), pushViewCmd(ViewThread))
}

func (v *searchView) loadState() {
	if v.state == nil {
		return
	}
	v.saved = v.state.SavedSearches()
	sort.SliceStable(v.saved, func(i, j int) bool {
		return strings.ToLower(v.saved[i].Name) < strings.ToLower(v.saved[j].Name)
	})
}

func (v *searchView) loadMetaCmd() tea.Cmd {
	if v.provider == nil {
		return func() tea.Msg {
			return searchMetaLoadedMsg{now: time.Now().UTC(), metaLoadErr: fmt.Errorf("missing provider")}
		}
	}
	self := strings.TrimPrefix(strings.TrimSpace(v.self), "@")
	return func() tea.Msg {
		now := time.Now().UTC()
		topics, err := v.provider.Topics()
		if err != nil {
			return searchMetaLoadedMsg{now: now, metaLoadErr: err}
		}
		dms := []data.DMConversation{}
		if self != "" {
			if got, derr := v.provider.DMConversations(self); derr == nil {
				dms = got
			}
		}
		agents, aerr := v.provider.Agents()
		if aerr != nil {
			agents = nil
		}
		totalTargets := len(topics) + len(dms)
		totalMessages := 0
		for _, t := range topics {
			totalMessages += t.MessageCount
		}
		for _, d := range dms {
			totalMessages += d.MessageCount
		}
		saved := []state.SavedSearch(nil)
		if v.state != nil {
			saved = v.state.SavedSearches()
		}
		return searchMetaLoadedMsg{
			now:           now,
			topics:        topics,
			dms:           dms,
			agents:        agents,
			saved:         saved,
			totalTargets:  totalTargets,
			totalMessages: totalMessages,
		}
	}
}

func (v *searchView) debounceCmd() tea.Cmd {
	rev := v.rev
	return tea.Tick(searchDebounce, func(time.Time) tea.Msg {
		return searchDebounceMsg{rev: rev}
	})
}

func (v *searchView) bumpSearch() tea.Cmd {
	v.rev++
	v.searching = true
	v.lastErr = nil
	v.took = 0
	return v.debounceCmd()
}

func (v *searchView) searchCmd(rev int, raw string) tea.Cmd {
	if v.provider == nil {
		return func() tea.Msg {
			return searchLoadedMsg{rev: rev, now: time.Now().UTC(), err: fmt.Errorf("missing provider")}
		}
	}
	parsed := parseSearchInput(raw, time.Now().UTC())
	query := parsed.Query
	return func() tea.Msg {
		start := time.Now().UTC()
		results, err := v.provider.Search(query)
		took := time.Since(start)
		// If query is empty, avoid showing everything.
		if strings.TrimSpace(raw) == "" {
			results = nil
			err = nil
		}
		return searchLoadedMsg{rev: rev, now: time.Now().UTC(), results: results, err: err, took: took}
	}
}
