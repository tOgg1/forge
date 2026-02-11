package fmailtui

import (
	"fmt"
	"regexp"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	"github.com/tOgg1/forge/internal/fmailtui/state"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

const liveTailMaxMessages = 2000

type liveTailIncomingMsg struct {
	msg fmail.Message
}

type liveTailFlashClearMsg struct {
	until time.Time
}

type flashHeaderMsg struct {
	until time.Time
}

type liveTailFilter struct {
	From     string
	To       string
	Priority string
	Tags     []string
	Text     string
	DMOnly   bool
}

func (f liveTailFilter) activeLabel() string {
	parts := make([]string, 0, 6)
	if f.From != "" {
		parts = append(parts, "from:"+f.From)
	}
	if f.To != "" {
		parts = append(parts, "to:"+f.To)
	}
	if f.Priority != "" {
		parts = append(parts, "priority:"+f.Priority)
	}
	for _, t := range f.Tags {
		if strings.TrimSpace(t) == "" {
			continue
		}
		parts = append(parts, "tag:"+t)
	}
	if f.Text != "" {
		parts = append(parts, "text:"+f.Text)
	}
	if f.DMOnly {
		parts = append(parts, "dm:only")
	}
	if len(parts) == 0 {
		return "none"
	}
	return strings.Join(parts, " ")
}

type highlightPattern struct {
	raw string
	re  *regexp.Regexp
}

type liveTailView struct {
	root     string
	self     string
	provider data.MessageProvider
	state    *state.Manager

	subCh     <-chan fmail.Message
	subCancel func()

	feed     []fmail.Message
	buffered []fmail.Message

	paused bool
	offset int // messages from bottom when paused

	filter       liveTailFilter
	filterActive bool
	filterInput  string

	highlightActive bool
	highlightInput  string
	highlights      []highlightPattern

	flashUntil time.Time
}

func newLiveTailView(root, self string, provider data.MessageProvider, st *state.Manager) *liveTailView {
	return &liveTailView{
		root:     root,
		self:     strings.TrimSpace(self),
		provider: provider,
		state:    st,
	}
}

func (v *liveTailView) Init() tea.Cmd {
	v.startSubscription()
	v.loadHighlightsFromState()
	return tea.Batch(v.waitForMessageCmd())
}

func (v *liveTailView) Close() {
	if v.subCancel != nil {
		v.subCancel()
		v.subCancel = nil
	}
	v.subCh = nil
}

func (v *liveTailView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case liveTailIncomingMsg:
		cmd := v.applyIncoming(typed.msg)
		return tea.Batch(v.waitForMessageCmd(), cmd)
	case liveTailFlashClearMsg:
		if !v.flashUntil.IsZero() && !typed.until.IsZero() && (v.flashUntil.Equal(typed.until) || v.flashUntil.Before(typed.until)) {
			v.flashUntil = time.Time{}
		}
		return nil
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *liveTailView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}

	palette := themePalette(theme)
	base := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Background(lipgloss.Color(palette.Base.Background))
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	accent := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true).Underline(true)

	title := "LIVE TAIL"
	if v.paused {
		title = title + "  " + muted.Render("PAUSED")
		if n := len(v.buffered); n > 0 {
			title = title + muted.Render(fmt.Sprintf(" (+%d)", n))
		}
	}
	header := fmt.Sprintf("%s  filter: %s", title, v.filter.activeLabel())
	headerLine := lipgloss.NewStyle().Bold(true).Render(truncateVis(header, maxInt(0, width)))

	lines := make([]string, 0, height)
	lines = append(lines, headerLine)

	if v.filterActive {
		lines = append(lines, muted.Render("f filter: ")+v.filterInput)
	} else if v.highlightActive {
		lines = append(lines, muted.Render("h highlight (regex, comma-separated): ")+v.highlightInput)
	}

	contentH := height - len(lines)
	if contentH < 0 {
		contentH = 0
	}

	msgLines := v.renderMessages(width, contentH, palette, accent, muted)
	lines = append(lines, msgLines...)

	return base.Render(lipgloss.JoinVertical(lipgloss.Left, lines...))
}

func (v *liveTailView) MinSize() (int, int) {
	return 40, 10
}

func (v *liveTailView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.subCancel = cancel
}

func (v *liveTailView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return liveTailIncomingMsg{msg: msg}
	}
}

func (v *liveTailView) applyIncoming(msg fmail.Message) tea.Cmd {
	if v.paused {
		v.buffered = append(v.buffered, msg)
		if len(v.buffered) > liveTailMaxMessages {
			v.buffered = v.buffered[len(v.buffered)-liveTailMaxMessages:]
		}
	} else {
		v.feed = append(v.feed, msg)
		if len(v.feed) > liveTailMaxMessages {
			v.feed = v.feed[len(v.feed)-liveTailMaxMessages:]
		}
	}

	if strings.EqualFold(strings.TrimSpace(msg.Priority), fmail.PriorityHigh) {
		v.flashUntil = time.Now().UTC().Add(500 * time.Millisecond)
		cmds := []tea.Cmd{
			func() tea.Msg { return flashHeaderMsg{until: v.flashUntil} },
			tea.Tick(500*time.Millisecond, func(time.Time) tea.Msg { return liveTailFlashClearMsg{until: v.flashUntil} }),
		}
		if v.soundAlertsEnabled() {
			cmds = append(cmds, bellCmd())
		}
		return tea.Batch(cmds...)
	}
	return nil
}

func (v *liveTailView) handleKey(msg tea.KeyMsg) tea.Cmd {
	if v.filterActive {
		return v.handleFilterKey(msg)
	}
	if v.highlightActive {
		return v.handleHighlightKey(msg)
	}

	switch msg.String() {
	case "esc", "backspace":
		return popViewCmd()
	case " ":
		if v.paused {
			return v.resume()
		}
		v.paused = true
		v.offset = 0
		return nil
	case "end", "G":
		return v.resume()
	case "j", "down":
		if v.paused {
			v.offset = maxInt(0, v.offset-1)
		}
		return nil
	case "k", "up":
		if v.paused {
			v.offset = minInt(maxInt(0, len(v.visibleMessages())-1), v.offset+1)
		}
		return nil
	case "pgup", "ctrl+u":
		if v.paused {
			v.offset = minInt(maxInt(0, len(v.visibleMessages())-1), v.offset+10)
		}
		return nil
	case "pgdown", "ctrl+d":
		if v.paused {
			v.offset = maxInt(0, v.offset-10)
		}
		return nil
	case "f":
		v.filterActive = true
		v.filterInput = v.filter.activeLabel()
		if v.filterInput == "none" {
			v.filterInput = ""
		}
		return nil
	case "c":
		v.filter = liveTailFilter{}
		v.offset = 0
		return nil
	case "h":
		v.highlightActive = true
		v.highlightInput = strings.Join(v.highlightRaw(), ",")
		return nil
	case "1":
		v.filter.Priority = fmail.PriorityHigh
		v.offset = 0
		return nil
	case "2":
		v.filter.DMOnly = !v.filter.DMOnly
		v.offset = 0
		return nil
	case "3":
		if v.self != "" {
			v.filter.From = v.self
			v.offset = 0
		}
		return nil
	case "/":
		// Search view placeholder exists; route there.
		return pushViewCmd(ViewSearch)
	}
	return nil
}

func (v *liveTailView) handleFilterKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.filterActive = false
		return nil
	case tea.KeyEnter:
		v.filter = parseLiveTailFilter(v.filterInput)
		v.filterActive = false
		v.offset = 0
		return nil
	case tea.KeyBackspace:
		if len(v.filterInput) > 0 {
			v.filterInput = v.filterInput[:len(v.filterInput)-1]
		}
		return nil
	case tea.KeyRunes:
		v.filterInput += string(msg.Runes)
		return nil
	}
	return nil
}

func (v *liveTailView) handleHighlightKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.highlightActive = false
		return nil
	case tea.KeyEnter:
		raw := splitCSVLike(v.highlightInput)
		v.setHighlights(raw)
		v.saveHighlightsToState(raw)
		v.highlightActive = false
		return nil
	case tea.KeyBackspace:
		if len(v.highlightInput) > 0 {
			v.highlightInput = v.highlightInput[:len(v.highlightInput)-1]
		}
		return nil
	case tea.KeyRunes:
		v.highlightInput += string(msg.Runes)
		return nil
	}
	return nil
}

func (v *liveTailView) resume() tea.Cmd {
	if !v.paused {
		return nil
	}
	if len(v.buffered) > 0 {
		v.feed = append(v.feed, v.buffered...)
		if len(v.feed) > liveTailMaxMessages {
			v.feed = v.feed[len(v.feed)-liveTailMaxMessages:]
		}
		v.buffered = nil
	}
	v.paused = false
	v.offset = 0
	return nil
}

func (v *liveTailView) visibleMessages() []fmail.Message {
	combined := v.feed
	if !v.paused && len(v.buffered) > 0 {
		combined = append(append([]fmail.Message(nil), v.feed...), v.buffered...)
	}
	out := make([]fmail.Message, 0, len(combined))
	for i := range combined {
		if v.matchesFilter(combined[i]) {
			out = append(out, combined[i])
		}
	}
	return out
}

func (v *liveTailView) matchesFilter(msg fmail.Message) bool {
	if v.filter.DMOnly && !strings.HasPrefix(strings.TrimSpace(msg.To), "@") {
		return false
	}
	if v.filter.From != "" && !strings.EqualFold(strings.TrimSpace(msg.From), v.filter.From) {
		return false
	}
	if v.filter.To != "" {
		to := strings.TrimSpace(msg.To)
		if !strings.EqualFold(to, v.filter.To) {
			return false
		}
	}
	if v.filter.Priority != "" && !strings.EqualFold(strings.TrimSpace(msg.Priority), v.filter.Priority) {
		return false
	}
	if len(v.filter.Tags) > 0 {
		msgTags := make(map[string]struct{}, len(msg.Tags))
		for _, t := range msg.Tags {
			msgTags[strings.ToLower(strings.TrimSpace(t))] = struct{}{}
		}
		for _, want := range v.filter.Tags {
			want = strings.ToLower(strings.TrimSpace(want))
			if want == "" {
				continue
			}
			if _, ok := msgTags[want]; !ok {
				return false
			}
		}
	}
	if v.filter.Text != "" {
		blob := strings.ToLower(messageBodyString(msg.Body))
		if !strings.Contains(blob, strings.ToLower(v.filter.Text)) {
			return false
		}
	}
	return true
}

func (v *liveTailView) renderMessages(width, height int, palette styles.Theme, highlightStyle lipgloss.Style, muted lipgloss.Style) []string {
	if height <= 0 {
		return nil
	}
	mapper := styles.NewAgentColorMapperWithPalette(palette.AgentPalette)
	ruleStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Border))
	bodyStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground))

	visible := v.visibleMessages()
	// Show from the bottom; apply pause offset as "messages from bottom".
	end := len(visible) - v.offset
	if end < 0 {
		end = 0
	}
	start := maxInt(0, end-200) // keep render cost bounded
	window := visible[start:end]

	lines := make([]string, 0, height)
	prevTarget := ""
	for i := range window {
		if len(lines) >= height {
			break
		}
		msg := window[i]
		target := strings.TrimSpace(msg.To)
		tag := "[" + target + "]"
		if strings.HasPrefix(target, "@") {
			tag = "[DM]"
		}

		if prevTarget != "" && target != "" && prevTarget != target && len(lines) < height {
			lines = append(lines, ruleStyle.Render(truncateVis(strings.Repeat("─", maxInt(0, width)), width)))
		}
		prevTarget = target

		ts := msg.Time
		if ts.IsZero() {
			ts = time.Now().UTC()
		}
		head := fmt.Sprintf("%s %s %s → %s", ts.Format("15:04:05"), tag, mapper.Foreground(msg.From).Render(mapper.Plain(msg.From)), truncate(target, 20))
		lines = append(lines, bodyStyle.Render(truncateVis(head, width)))

		body := strings.TrimRight(messageBodyString(msg.Body), "\n")
		if strings.TrimSpace(body) == "" {
			continue
		}
		bodyLines := renderBodyLines(body, maxInt(0, width-4), palette)
		if len(bodyLines) > 5 {
			bodyLines = append(bodyLines[:5], muted.Render("..."))
		}
		for _, bl := range bodyLines {
			if len(lines) >= height {
				break
			}
			line := "  " + bl
			if v.matchesHighlight(bl) {
				lines = append(lines, highlightStyle.Render(truncateVis(line, width)))
			} else {
				lines = append(lines, muted.Render(truncateVis(line, width)))
			}
		}
		// Spacer.
		if len(lines) < height {
			lines = append(lines, "")
		}
	}

	// Auto-scroll indicator.
	if !v.paused && len(lines) < height {
		lines = append(lines, muted.Render("▌  (auto-scrolling — Space to pause)"))
	}
	return clampRenderedLines(lines, height)
}

func clampRenderedLines(lines []string, height int) []string {
	if height <= 0 || len(lines) == 0 {
		return nil
	}
	if len(lines) <= height {
		return lines
	}
	return lines[len(lines)-height:]
}

func parseLiveTailFilter(input string) liveTailFilter {
	in := strings.TrimSpace(input)
	if in == "" {
		return liveTailFilter{}
	}
	out := liveTailFilter{}
	tokens := strings.Fields(in)
	textTerms := make([]string, 0, 2)
	for _, tok := range tokens {
		if !strings.Contains(tok, ":") {
			textTerms = append(textTerms, tok)
			continue
		}
		key, val, _ := strings.Cut(tok, ":")
		key = strings.ToLower(strings.TrimSpace(key))
		val = strings.TrimSpace(val)
		switch key {
		case "from":
			out.From = val
		case "to":
			out.To = val
		case "priority":
			out.Priority = val
		case "tag":
			if val != "" {
				out.Tags = append(out.Tags, val)
			}
		case "text":
			if val != "" {
				textTerms = append(textTerms, val)
			}
		case "dm":
			if strings.EqualFold(val, "only") || strings.EqualFold(val, "true") || val == "1" {
				out.DMOnly = true
			}
		default:
			// Unknown: treat as free text.
			if val != "" {
				textTerms = append(textTerms, val)
			}
		}
	}
	out.Text = strings.TrimSpace(strings.Join(textTerms, " "))
	return out
}

func splitCSVLike(s string) []string {
	raw := strings.Split(s, ",")
	out := make([]string, 0, len(raw))
	for _, p := range raw {
		p = strings.TrimSpace(p)
		if p == "" {
			continue
		}
		out = append(out, p)
	}
	return out
}

func (v *liveTailView) highlightRaw() []string {
	out := make([]string, 0, len(v.highlights))
	for _, h := range v.highlights {
		if strings.TrimSpace(h.raw) != "" {
			out = append(out, h.raw)
		}
	}
	return out
}

func (v *liveTailView) matchesHighlight(line string) bool {
	for _, h := range v.highlights {
		if h.re != nil && h.re.MatchString(line) {
			return true
		}
	}
	return false
}

func (v *liveTailView) setHighlights(raw []string) {
	next := make([]highlightPattern, 0, len(raw))
	for _, p := range raw {
		p = strings.TrimSpace(p)
		if p == "" {
			continue
		}
		re, err := regexp.Compile(p)
		if err != nil {
			continue
		}
		next = append(next, highlightPattern{raw: p, re: re})
	}
	v.highlights = next
}

func (v *liveTailView) loadHighlightsFromState() {
	if v.state == nil {
		return
	}
	raw := v.state.HighlightPatterns()
	v.setHighlights(raw)
}

func (v *liveTailView) saveHighlightsToState(raw []string) {
	if v.state == nil {
		return
	}
	v.state.SetHighlightPatterns(raw)
	v.state.SaveSoon()
}

func (v *liveTailView) soundAlertsEnabled() bool {
	if v.state == nil {
		return false
	}
	return v.state.Preferences().SoundAlerts
}

func bellCmd() tea.Cmd {
	return func() tea.Msg {
		// Terminal bell.
		fmt.Print("\a")
		return nil
	}
}
