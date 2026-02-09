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
	status := "Enter: open  j/k: move  Tab: next  Shift+Tab: prev  s: save  p: saved  Esc: back"
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
	case "p":
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

func (v *searchView) renderQueryLine(width int, palette styles.Theme) string {
	prompt := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("> ")
	if v.focus == searchFocusSavePrompt {
		name := v.saveName
		if name == "" {
			name = ""
		}
		cursor := "_"
		r := []rune(name)
		pos := clampInt(v.saveNameCursor, 0, len(r))
		withCursor := string(r[:pos]) + cursor + string(r[pos:])
		return prompt + lipgloss.NewStyle().Bold(true).Render("save: ") + truncateVis(withCursor, maxInt(0, width-8))
	}

	rendered := renderSearchQueryTokens(v.query, palette)
	cursor := "_"
	if v.searching {
		cursor = ""
	}
	q := []rune(v.query)
	pos := clampInt(v.cursor, 0, len(q))
	plainWithCursor := string(q[:pos]) + cursor + string(q[pos:])

	// If we can't safely re-insert cursor into styled tokens, show plain for now.
	// (Cursor styling is less important than token highlighting.)
	if v.cursor != len(q) {
		rendered = renderSearchQueryTokens(plainWithCursor, palette)
	} else {
		rendered = renderSearchQueryTokens(v.query+cursor, palette)
	}

	if strings.TrimSpace(v.query) == "" && v.cursor == 0 {
		rendered = renderSearchQueryTokens(cursor, palette)
	}
	if v.focus == searchFocusSaved {
		rendered = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("(saved selected)")
	}
	_ = plainWithCursor
	return prompt + truncateVis(rendered, width)
}

func (v *searchView) renderSavedLine(width int, palette styles.Theme) string {
	label := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Saved: ")
	if len(v.saved) == 0 {
		return label + lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("(none)")
	}
	parts := make([]string, 0, len(v.saved))
	for i := range v.saved {
		name := strings.TrimSpace(v.saved[i].Name)
		if name == "" {
			continue
		}
		style := lipgloss.NewStyle().
			Foreground(lipgloss.Color(palette.Base.Foreground)).
			Background(lipgloss.Color(palette.Base.Border)).
			Padding(0, 1)
		if v.focus == searchFocusSaved && i == v.savedSelected {
			style = style.Background(lipgloss.Color(palette.Chrome.SelectedItem)).Bold(true)
		}
		parts = append(parts, style.Render(name))
	}
	line := label + strings.Join(parts, " ")
	return truncateVis(line, width)
}

func (v *searchView) renderStatsLine(width int, palette styles.Theme) string {
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	n := len(v.results)
	stats := fmt.Sprintf("%d results", n)
	if v.metaLoaded && v.totalTargets > 0 {
		stats = fmt.Sprintf("%s (searched ~%d messages in %d targets)", stats, v.totalMessages, v.totalTargets)
	}
	if v.took > 0 {
		stats = fmt.Sprintf("%s in %s", stats, v.took.Truncate(time.Millisecond))
	}
	return muted.Render(truncateVis(stats, width))
}

func (v *searchView) renderResults(width, height int, palette styles.Theme) string {
	if height <= 0 {
		return ""
	}
	if v.searching && strings.TrimSpace(v.query) == "" {
		return ""
	}
	if len(v.results) == 0 {
		if strings.TrimSpace(v.query) == "" {
			return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Type to search. Example: from:coder-* tag:auth since:1h token refresh")
		}
		if v.searching {
			return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Searching...")
		}
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("No results")
	}

	parsed := parseSearchInput(v.query, v.now)
	matchText := strings.TrimSpace(parsed.Query.Text)

	bookmarked := make(map[string]bool)
	readMarkers := make(map[string]string)
	if v.state != nil {
		snap := v.state.Snapshot()
		for _, bm := range snap.Bookmarks {
			if strings.TrimSpace(bm.MessageID) != "" {
				bookmarked[bm.MessageID] = true
			}
		}
		for k, vv := range snap.ReadMarkers {
			readMarkers[k] = vv
		}
	}

	// Filter and score.
	itemsByTopic := make(map[string][]searchItem)
	for i := range v.results {
		r := v.results[i]
		if parsed.Query.HasBookmark && !bookmarked[r.Message.ID] {
			continue
		}
		if parsed.Query.IsUnread {
			marker := strings.TrimSpace(readMarkers[r.Topic])
			if marker != "" && r.Message.ID <= marker {
				continue
			}
		}

		count := 0
		if matchText != "" {
			count = countOccurrencesCI(messageBodyString(r.Message.Body), matchText)
		}
		if count == 0 && matchText != "" {
			count = 1
		}
		itemsByTopic[r.Topic] = append(itemsByTopic[r.Topic], searchItem{r: r, matchText: matchText, matches: count})
	}

	topics := make([]string, 0, len(itemsByTopic))
	for t := range itemsByTopic {
		topics = append(topics, t)
	}

	for t := range topics {
		list := itemsByTopic[topics[t]]
		sort.SliceStable(list, func(i, j int) bool {
			if list[i].matches != list[j].matches {
				return list[i].matches > list[j].matches
			}
			ti := list[i].r.Message.Time
			tj := list[j].r.Message.Time
			if !ti.Equal(tj) {
				return ti.After(tj)
			}
			return list[i].r.Message.ID > list[j].r.Message.ID
		})
		itemsByTopic[topics[t]] = list
	}

	sort.SliceStable(topics, func(i, j int) bool {
		a := itemsByTopic[topics[i]]
		b := itemsByTopic[topics[j]]
		ai, bi := 0, 0
		if len(a) > 0 {
			ai = a[0].matches
		}
		if len(b) > 0 {
			bi = b[0].matches
		}
		if ai != bi {
			return ai > bi
		}
		at, bt := time.Time{}, time.Time{}
		if len(a) > 0 {
			at = a[0].r.Message.Time
		}
		if len(b) > 0 {
			bt = b[0].r.Message.Time
		}
		if !at.Equal(bt) {
			return at.After(bt)
		}
		return topics[i] < topics[j]
	})

	mapper := styles.NewAgentColorMapperWithPalette(palette.AgentPalette)
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	hi := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Underline(true)

	lines := make([]string, 0, height*2)
	resultIndex := -1
	for _, topic := range topics {
		header := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render("[" + topic + "]")
		lines = append(lines, truncateVis(header, width))
		list := itemsByTopic[topic]
		for _, it := range list {
			resultIndex++
			msg := it.r.Message
			ts := msg.Time.UTC()
			if ts.IsZero() {
				ts = v.now
			}
			fromColored := mapper.Foreground(msg.From).Render(mapper.Plain(msg.From))
			matchSuffix := ""
			if it.matchText != "" {
				matchSuffix = fmt.Sprintf(" — %d matches", it.matches)
			}

			cursor := " "
			rowStyle := lipgloss.NewStyle()
			if resultIndex == v.selected {
				cursor = "▸"
				rowStyle = rowStyle.Foreground(lipgloss.Color(palette.Chrome.SelectedItem)).Bold(true)
			}

			head := fmt.Sprintf("%s %s (%s)%s", cursor, fromColored, ts.Format("15:04"), matchSuffix)
			lines = append(lines, rowStyle.Render(truncateVis(head, width)))

			if it.r.Prev != nil {
				prevLine := truncate(firstLine(messageBodyString(it.r.Prev.Body)), maxInt(0, width-6))
				lines = append(lines, muted.Render(truncateVis("    "+prevLine, width)))
			}

			body := strings.TrimSpace(messageBodyString(msg.Body))
			if body == "" {
				body = "(empty)"
			}
			bodyLine := truncate(firstLine(body), maxInt(0, width-6))
			if it.matchText != "" {
				bodyLine = highlightAllCI(bodyLine, it.matchText, hi)
			}
			lines = append(lines, truncateVis("  "+bodyLine, width))

			if it.r.Next != nil {
				nextLine := truncate(firstLine(messageBodyString(it.r.Next.Body)), maxInt(0, width-6))
				lines = append(lines, muted.Render(truncateVis("    "+nextLine, width)))
			}

			lines = append(lines, "")
			if len(lines) >= height {
				break
			}
		}
		if len(lines) >= height {
			break
		}
	}

	if len(lines) > 0 && lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	if len(lines) > height {
		lines = lines[:height]
	}
	return strings.Join(lines, "\n")
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

func (v *searchView) insertRunes(s string) {
	if s == "" {
		return
	}
	q := []rune(v.query)
	pos := clampInt(v.cursor, 0, len(q))
	ins := []rune(s)
	next := make([]rune, 0, len(q)+len(ins))
	next = append(next, q[:pos]...)
	next = append(next, ins...)
	next = append(next, q[pos:]...)
	v.query = string(next)
	v.cursor = pos + len(ins)
	v.resetCompletion()
}

func (v *searchView) deleteLeft() bool {
	q := []rune(v.query)
	pos := clampInt(v.cursor, 0, len(q))
	if pos <= 0 || len(q) == 0 {
		return false
	}
	next := append(q[:pos-1], q[pos:]...)
	v.query = string(next)
	v.cursor = pos - 1
	v.resetCompletion()
	return true
}

func (v *searchView) resetCompletion() {
	v.completeKey = ""
	v.completePrefix = ""
	v.completeOptions = nil
	v.completeIndex = 0
}

func (v *searchView) tryComplete(delta int) bool {
	raw := v.query
	cursor := clampInt(v.cursor, 0, len([]rune(raw)))
	start, end := tokenBoundsAt(raw, cursor)
	if start < 0 || end < start {
		return false
	}
	token := strings.TrimSpace(string([]rune(raw)[start:end]))
	if token == "" || !strings.Contains(token, ":") {
		return false
	}
	key, val, _ := strings.Cut(token, ":")
	key = strings.ToLower(strings.TrimSpace(key))
	val = strings.TrimSpace(val)

	cands := v.completionCandidates(key)
	if len(cands) == 0 {
		return false
	}
	matches := make([]string, 0, len(cands))
	for _, c := range cands {
		if val == "" || strings.HasPrefix(strings.ToLower(c), strings.ToLower(val)) {
			matches = append(matches, c)
		}
	}
	if len(matches) == 0 {
		return false
	}
	sort.Strings(matches)

	// Preserve cycling if the token prefix didn't change since last Tab.
	if v.lastCompletionRev != v.rev || v.completeKey != key || v.completePrefix != val {
		v.completeKey = key
		v.completePrefix = val
		v.completeOptions = matches
		v.completeIndex = 0
		v.lastCompletionRev = v.rev
	} else {
		v.completeIndex = (v.completeIndex + delta + len(v.completeOptions)) % len(v.completeOptions)
	}
	chosen := v.completeOptions[v.completeIndex]

	repl := key + ":" + chosen
	whole := []rune(raw)
	next := string(append(append([]rune(nil), whole[:start]...), append([]rune(repl), whole[end:]...)...))
	v.query = next
	v.cursor = start + len([]rune(repl))
	return true
}

func (v *searchView) completionCandidates(key string) []string {
	switch key {
	case "from":
		return v.agents
	case "to":
		out := make([]string, 0, len(v.topics)+len(v.agents))
		out = append(out, v.topics...)
		for _, a := range v.agents {
			out = append(out, "@"+a)
		}
		return out
	case "in":
		return v.topics
	case "priority":
		return []string{fmail.PriorityHigh, fmail.PriorityNormal, fmail.PriorityLow}
	case "has":
		return []string{"reply", "bookmark"}
	case "is":
		return []string{"unread"}
	case "since", "until":
		return []string{"15m", "1h", "6h", "1d", "7d"}
	default:
		return nil
	}
}

func renderSearchQueryTokens(raw string, palette styles.Theme) string {
	keyStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent))
	valStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Chrome.Breadcrumb))
	freeStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground))

	toks := strings.Fields(raw)
	if len(toks) == 0 {
		return freeStyle.Render(raw)
	}

	out := make([]string, 0, len(toks))
	for _, tok := range toks {
		if !strings.Contains(tok, ":") {
			out = append(out, freeStyle.Render(tok))
			continue
		}
		key, val, _ := strings.Cut(tok, ":")
		k := strings.ToLower(strings.TrimSpace(key))
		if k == "" {
			out = append(out, freeStyle.Render(tok))
			continue
		}
		out = append(out, keyStyle.Render(key+":")+valStyle.Render(val))
	}
	return strings.Join(out, " ")
}

func renderSearchQuery(q data.SearchQuery) string {
	parts := make([]string, 0, 10)
	if q.From != "" {
		parts = append(parts, "from:"+q.From)
	}
	if q.To != "" {
		parts = append(parts, "to:"+q.To)
	}
	if q.In != "" {
		parts = append(parts, "in:"+q.In)
	}
	if q.Priority != "" {
		parts = append(parts, "priority:"+q.Priority)
	}
	for _, t := range q.Tags {
		if strings.TrimSpace(t) != "" {
			parts = append(parts, "tag:"+t)
		}
	}
	if q.HasReply {
		parts = append(parts, "has:reply")
	}
	if q.HasBookmark {
		parts = append(parts, "has:bookmark")
	}
	if q.IsUnread {
		parts = append(parts, "is:unread")
	}
	if q.Text != "" {
		parts = append(parts, q.Text)
	}
	return strings.Join(parts, " ")
}

func countOccurrencesCI(haystack, needle string) int {
	h := strings.ToLower(haystack)
	n := strings.ToLower(strings.TrimSpace(needle))
	if n == "" {
		return 0
	}
	count := 0
	for {
		idx := strings.Index(h, n)
		if idx < 0 {
			break
		}
		count++
		h = h[idx+len(n):]
	}
	return count
}

func highlightAllCI(line, needle string, style lipgloss.Style) string {
	if strings.TrimSpace(needle) == "" || line == "" {
		return line
	}
	lower := strings.ToLower(line)
	n := strings.ToLower(needle)
	var out strings.Builder
	i := 0
	for {
		idx := strings.Index(lower[i:], n)
		if idx < 0 {
			out.WriteString(line[i:])
			break
		}
		idx += i
		out.WriteString(line[i:idx])
		out.WriteString(style.Render(line[idx : idx+len(needle)]))
		i = idx + len(needle)
		if i >= len(line) {
			break
		}
	}
	return out.String()
}

func tokenBoundsAt(raw string, cursor int) (int, int) {
	r := []rune(raw)
	if cursor < 0 {
		cursor = 0
	}
	if cursor > len(r) {
		cursor = len(r)
	}
	// token separated by whitespace; find bounds around cursor.
	start := cursor
	for start > 0 && !isSpaceRune(r[start-1]) {
		start--
	}
	end := cursor
	for end < len(r) && !isSpaceRune(r[end]) {
		end++
	}
	return start, end
}

func isSpaceRune(r rune) bool {
	return r == ' ' || r == '\t' || r == '\n' || r == '\r'
}
