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
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
)

type composeField int

const (
	composeFieldTo composeField = iota
	composeFieldPriority
	composeFieldTags
	composeFieldBody
)

var composePriorities = []string{fmail.PriorityLow, fmail.PriorityNormal, fmail.PriorityHigh}

const quickHistoryLimit = 100

type composeState struct {
	active      bool
	focus       composeField
	to          string
	priority    string
	tags        string
	replyTo     string
	parentLine  string
	body        string
	sending     bool
	err         string
	savePrompt  bool
	restoreAsk  bool
	draftCached tuistate.ComposeDraft

	toCompletionPrefix  string
	toCompletionIndex   int
	tagCompletionPrefix string
	tagCompletionIndex  int
}

type quickSendState struct {
	active  bool
	input   string
	err     string
	sending bool

	history      []string
	historyIndex int

	completionPrefix string
	completionIndex  int
}

type composeReplySeed struct {
	Target     string
	ReplyTo    string
	ParentLine string
}

type composeContextView interface {
	ComposeTarget() string
	ComposeReplySeed(dmDirect bool) (composeReplySeed, bool)
}

type providerSender interface {
	Send(req data.SendRequest) (fmail.Message, error)
}

type composeSendSource int

const (
	sendSourceCompose composeSendSource = iota
	sendSourceQuick
)

type composeSendResultMsg struct {
	source composeSendSource
	req    data.SendRequest
	msg    fmail.Message
	err    error
}

func (m *Model) handleComposerKey(msg tea.KeyMsg) (tea.Cmd, bool) {
	if m.compose.active {
		return m.handleComposeOverlayKey(msg), true
	}
	if m.quick.active {
		return m.handleQuickSendKey(msg), true
	}
	return nil, false
}

func (m *Model) handleComposeOverlayKey(msg tea.KeyMsg) tea.Cmd {
	if m.compose.restoreAsk {
		switch strings.ToLower(msg.String()) {
		case "y":
			m.compose.to = m.compose.draftCached.To
			m.compose.priority = m.compose.draftCached.Priority
			m.compose.tags = m.compose.draftCached.Tags
			m.compose.replyTo = m.compose.draftCached.ReplyTo
			m.compose.body = m.compose.draftCached.Body
			m.compose.restoreAsk = false
			return nil
		case "n", "esc", "backspace":
			m.compose.restoreAsk = false
			return nil
		default:
			return nil
		}
	}

	if m.compose.savePrompt {
		switch strings.ToLower(msg.String()) {
		case "y":
			m.persistDraft(true)
			m.closeComposeOverlay()
			m.setToast("Draft saved")
		case "n":
			m.persistDraft(false)
			m.closeComposeOverlay()
		case "esc", "backspace":
			m.compose.savePrompt = false
		}
		return nil
	}

	if m.compose.sending {
		if msg.String() == "esc" {
			return nil
		}
		return nil
	}

	switch msg.String() {
	case "tab":
		if m.compose.focus == composeFieldTo {
			before := m.compose.to
			m.completeComposeTarget()
			if m.compose.to != before {
				return nil
			}
		}
		if m.compose.focus == composeFieldTags {
			before := m.compose.tags
			m.completeComposeTag()
			if m.compose.tags != before {
				return nil
			}
		}
		m.compose.focus = composeField((int(m.compose.focus) + 1) % 4)
		return nil
	case "shift+tab":
		m.compose.focus = composeField((int(m.compose.focus) + 3) % 4)
		return nil
	case "esc":
		if strings.TrimSpace(m.compose.body) != "" {
			m.compose.savePrompt = true
			return nil
		}
		m.closeComposeOverlay()
		return nil
	case "ctrl+enter":
		return m.composeSendCmd(sendSourceCompose)
	case "enter":
		switch m.compose.focus {
		case composeFieldTo, composeFieldPriority, composeFieldTags:
			m.compose.focus = composeField((int(m.compose.focus) + 1) % 4)
			return nil
		case composeFieldBody:
			m.compose.body += "\n"
			return nil
		}
	case "up":
		if m.compose.focus == composeFieldPriority {
			m.cyclePriority(-1)
		}
		return nil
	case "down":
		if m.compose.focus == composeFieldPriority {
			m.cyclePriority(1)
		}
		return nil
	case "backspace", "delete":
		m.composeDeleteRune()
		return nil
	case "ctrl+h":
		m.composeDeleteRune()
		return nil
	case "ctrl+j":
		return m.composeSendCmd(sendSourceCompose)
	}

	if msg.String() == "alt+enter" {
		m.compose.body += "\n"
		return nil
	}

	switch msg.Type {
	case tea.KeyRunes:
		if len(msg.Runes) == 0 {
			return nil
		}
		r := string(msg.Runes)
		switch m.compose.focus {
		case composeFieldTo:
			m.compose.to += r
		case composeFieldPriority:
			m.compose.priority += strings.ToLower(r)
		case composeFieldTags:
			m.compose.tags += strings.ToLower(r)
		case composeFieldBody:
			m.compose.body += r
		}
		m.resetComposeCompletionState()
		return nil
	}
	return nil
}

func (m *Model) handleQuickSendKey(msg tea.KeyMsg) tea.Cmd {
	if m.quick.sending {
		if msg.String() == "esc" {
			return nil
		}
		return nil
	}

	switch msg.String() {
	case "esc", "backspace":
		if strings.TrimSpace(m.quick.input) == "" || m.quick.input == ":" {
			m.quick.active = false
			m.quick.err = ""
			m.quick.input = ""
			m.quick.historyIndex = -1
			return nil
		}
		fallthrough
	case "delete", "ctrl+h":
		if len(m.quick.input) > 0 {
			runes := []rune(m.quick.input)
			m.quick.input = string(runes[:len(runes)-1])
		}
		m.resetQuickCompletion()
		return nil
	case "up":
		m.quickHistoryStep(-1)
		return nil
	case "down":
		m.quickHistoryStep(1)
		return nil
	case "tab":
		m.completeQuickTarget()
		return nil
	case "enter":
		return m.composeSendCmd(sendSourceQuick)
	}

	switch msg.Type {
	case tea.KeyRunes:
		if len(msg.Runes) == 0 {
			return nil
		}
		m.quick.input += string(msg.Runes)
		m.quick.err = ""
		m.resetQuickCompletion()
	}
	return nil
}

func (m *Model) maybeOpenComposeForNewMessage() bool {
	seed := composeReplySeed{}
	target := ""
	if active := m.activeView(); active != nil {
		if ctx, ok := active.(composeContextView); ok {
			target = strings.TrimSpace(ctx.ComposeTarget())
		}
	}
	m.openComposeOverlay(target, seed)
	return true
}

func (m *Model) maybeOpenComposeReply(dmDirect bool) bool {
	active := m.activeView()
	if active == nil {
		return false
	}
	ctx, ok := active.(composeContextView)
	if !ok {
		return false
	}
	seed, ok := ctx.ComposeReplySeed(dmDirect)
	if !ok {
		return false
	}
	m.openComposeOverlay(seed.Target, seed)
	return true
}

func (m *Model) openComposeOverlay(target string, seed composeReplySeed) {
	m.quick.active = false
	m.quick.err = ""
	m.compose = composeState{
		active:             true,
		focus:              composeFieldBody,
		to:                 strings.TrimSpace(target),
		priority:           fmail.PriorityNormal,
		tags:               "",
		replyTo:            strings.TrimSpace(seed.ReplyTo),
		parentLine:         strings.TrimSpace(seed.ParentLine),
		body:               "",
		sending:            false,
		err:                "",
		savePrompt:         false,
		restoreAsk:         false,
		toCompletionIndex:  -1,
		tagCompletionIndex: -1,
	}

	if strings.TrimSpace(m.compose.to) == "" {
		m.compose.focus = composeFieldTo
	}

	if m.tuiState != nil && strings.TrimSpace(m.compose.to) != "" {
		if draft, ok := m.tuiState.Draft(m.compose.to); ok && strings.TrimSpace(draft.Body) != "" {
			m.compose.restoreAsk = true
			m.compose.draftCached = draft
		}
	}
}

func (m *Model) openQuickSendBar() {
	m.compose.active = false
	m.compose.err = ""
	m.quick.active = true
	if strings.TrimSpace(m.quick.input) == "" {
		m.quick.input = ":"
	}
	m.quick.err = ""
	m.quick.sending = false
	m.quick.historyIndex = -1
}

func (m *Model) closeComposeOverlay() {
	m.compose = composeState{}
}

func (m *Model) composeSendCmd(source composeSendSource) tea.Cmd {
	req, err := m.composeSendRequest(source)
	if err != nil {
		if source == sendSourceCompose {
			m.compose.err = err.Error()
		} else {
			m.quick.err = err.Error()
		}
		return nil
	}
	if source == sendSourceCompose {
		m.compose.sending = true
		m.compose.err = ""
	} else {
		m.quick.sending = true
		m.quick.err = ""
	}

	return func() tea.Msg {
		if sender, ok := m.provider.(providerSender); ok {
			msg, sendErr := sender.Send(req)
			return composeSendResultMsg{source: source, req: req, msg: msg, err: sendErr}
		}

		msg, sendErr := m.sendViaStore(req)
		return composeSendResultMsg{source: source, req: req, msg: msg, err: sendErr}
	}
}

func (m *Model) sendViaStore(req data.SendRequest) (fmail.Message, error) {
	msg, err := dataNormalizeSendRequest(req, m.selfAgent)
	if err != nil {
		return fmail.Message{}, err
	}
	if _, err := m.store.SaveMessage(&msg); err != nil {
		return fmail.Message{}, err
	}
	return msg, nil
}

// dataNormalizeSendRequest mirrors data.normalizeSendRequest without exporting internals.
func dataNormalizeSendRequest(req data.SendRequest, fallbackAgent string) (fmail.Message, error) {
	from := strings.TrimSpace(req.From)
	if from == "" {
		from = strings.TrimSpace(fallbackAgent)
	}
	if from == "" {
		from = defaultSelfAgent
	}
	normalizedFrom, err := fmail.NormalizeAgentName(from)
	if err != nil {
		return fmail.Message{}, err
	}

	to := strings.TrimSpace(req.To)
	if to == "" {
		return fmail.Message{}, fmt.Errorf("missing target")
	}
	if _, _, err := fmail.NormalizeTarget(to); err != nil {
		return fmail.Message{}, err
	}

	body := strings.TrimSpace(req.Body)
	if body == "" {
		return fmail.Message{}, fmt.Errorf("missing body")
	}

	priority := strings.TrimSpace(strings.ToLower(req.Priority))
	if priority == "" {
		priority = fmail.PriorityNormal
	}
	if err := fmail.ValidatePriority(priority); err != nil {
		return fmail.Message{}, err
	}

	tags := parseTagCSV(strings.Join(req.Tags, ","))
	if len(tags) > 0 {
		if err := fmail.ValidateTags(tags); err != nil {
			return fmail.Message{}, err
		}
	}

	msgTime := req.Time
	if msgTime.IsZero() {
		msgTime = time.Now().UTC()
	}

	return fmail.Message{
		From:     normalizedFrom,
		To:       to,
		Time:     msgTime,
		Body:     body,
		ReplyTo:  strings.TrimSpace(req.ReplyTo),
		Priority: priority,
		Tags:     tags,
	}, nil
}

func (m *Model) composeSendRequest(source composeSendSource) (data.SendRequest, error) {
	if source == sendSourceQuick {
		target, body, ok := parseQuickSendInput(m.quick.input)
		if !ok {
			return data.SendRequest{}, fmt.Errorf("expected :<target> <message>")
		}
		return data.SendRequest{
			From:     m.selfAgent,
			To:       target,
			Body:     body,
			Priority: fmail.PriorityNormal,
		}, nil
	}

	if strings.TrimSpace(m.compose.to) == "" {
		return data.SendRequest{}, fmt.Errorf("missing target")
	}
	if strings.TrimSpace(m.compose.body) == "" {
		return data.SendRequest{}, fmt.Errorf("message body is empty")
	}

	return data.SendRequest{
		From:     m.selfAgent,
		To:       strings.TrimSpace(m.compose.to),
		Body:     strings.TrimSpace(m.compose.body),
		ReplyTo:  strings.TrimSpace(m.compose.replyTo),
		Priority: normalizePriorityInput(m.compose.priority),
		Tags:     parseTagCSV(m.compose.tags),
	}, nil
}

func (m *Model) handleComposeSendResult(msg composeSendResultMsg) tea.Cmd {
	if msg.source == sendSourceCompose {
		m.compose.sending = false
		if msg.err != nil {
			m.compose.err = msg.err.Error()
			return nil
		}
		m.persistDraft(false)
		m.closeComposeOverlay()
		m.setToast("Sent ✓")
	} else {
		m.quick.sending = false
		if msg.err != nil {
			m.quick.err = msg.err.Error()
			return nil
		}
		m.quick.active = false
		m.quick.err = ""
		m.quick.input = ""
		m.quick.historyIndex = -1
		m.recordQuickHistory(msg.req)
		m.setToast("Sent ✓")
	}

	if active := m.activeView(); active != nil {
		return active.Init()
	}
	return nil
}

func (m *Model) recordQuickHistory(req data.SendRequest) {
	line := fmt.Sprintf(":%s %s", strings.TrimSpace(req.To), strings.TrimSpace(req.Body))
	line = strings.TrimSpace(line)
	if line == "" {
		return
	}
	if len(m.quick.history) == 0 || m.quick.history[len(m.quick.history)-1] != line {
		m.quick.history = append(m.quick.history, line)
		if len(m.quick.history) > quickHistoryLimit {
			m.quick.history = m.quick.history[len(m.quick.history)-quickHistoryLimit:]
		}
	}
}

func (m *Model) quickHistoryStep(delta int) {
	if len(m.quick.history) == 0 {
		return
	}
	if m.quick.historyIndex < 0 {
		if delta > 0 {
			return
		}
		m.quick.historyIndex = len(m.quick.history) - 1
		m.quick.input = m.quick.history[m.quick.historyIndex]
		return
	}
	next := m.quick.historyIndex + delta
	if next < 0 {
		next = 0
	}
	if next >= len(m.quick.history) {
		m.quick.historyIndex = -1
		m.quick.input = ":"
		return
	}
	m.quick.historyIndex = next
	m.quick.input = m.quick.history[next]
}

func (m *Model) renderComposeOverlay(width, height int, theme Theme) string {
	palette := themePalette(theme)
	panelWidth := minInt(maxInt(50, width-8), 96)
	panelHeight := minInt(maxInt(14, height-4), height)
	if panelHeight < 10 {
		panelHeight = height
	}

	c := m.compose
	to := c.to
	priority := normalizePriorityInput(c.priority)
	tags := c.tags
	body := c.body
	if body == "" {
		body = ""
	}

	cursor := "_"
	if c.sending {
		cursor = ""
	}

	if c.focus == composeFieldTo && !c.sending {
		to += cursor
	}
	if c.focus == composeFieldPriority && !c.sending {
		priority += cursor
	}
	if c.focus == composeFieldTags && !c.sending {
		tags += cursor
	}
	if c.focus == composeFieldBody && !c.sending {
		body += cursor
	}

	head := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render("Compose")
	if strings.TrimSpace(c.replyTo) != "" {
		head += "  " + lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("reply "+shortID(c.replyTo))
	}

	status := "[Ctrl+Enter: Send] [Esc: Close] [Tab: Next]"
	if c.sending {
		status = "Sending..."
	}
	if strings.TrimSpace(c.err) != "" {
		status = "Send failed: " + c.err
	}
	if c.restoreAsk {
		status = "Restore saved draft? [y/N]"
	}
	if c.savePrompt {
		status = "Save draft before closing? [y/N], Esc cancels"
	}

	lines := []string{
		head,
		"",
		"To: " + to,
		"Priority: " + priority,
		"Tags: " + tags,
	}
	if strings.TrimSpace(c.replyTo) != "" {
		replyLine := "Reply to: " + c.replyTo
		if strings.TrimSpace(c.parentLine) != "" {
			replyLine += " (" + truncate(c.parentLine, 40) + ")"
		}
		lines = append(lines, replyLine)
	}
	lines = append(lines, "", "Body:")

	bodyLines := strings.Split(strings.ReplaceAll(body, "\r\n", "\n"), "\n")
	maxBody := maxInt(3, panelHeight-len(lines)-4)
	if len(bodyLines) > maxBody {
		bodyLines = bodyLines[len(bodyLines)-maxBody:]
	}
	for _, line := range bodyLines {
		if strings.TrimSpace(line) == "" {
			lines = append(lines, "  ")
			continue
		}
		for _, wrapped := range wrapLines(line, maxInt(8, panelWidth-6)) {
			lines = append(lines, "  "+wrapped)
		}
	}

	lines = append(lines, "", status)
	content := strings.Join(lines, "\n")
	panel := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(palette.Borders.ActivePane)).
		Background(lipgloss.Color(palette.Base.Background)).
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Padding(1, 2).
		Width(panelWidth)

	return lipgloss.Place(width, height, lipgloss.Center, lipgloss.Center, panel.Render(content))
}

func (m *Model) renderQuickSendBar(width int, theme Theme) string {
	palette := themePalette(theme)
	input := m.quick.input
	if input == "" {
		input = ":"
	}
	if !m.quick.sending {
		input += "_"
	}
	status := ""
	if m.quick.sending {
		status = "  Sending..."
	}
	if strings.TrimSpace(m.quick.err) != "" {
		status = "  error: " + m.quick.err
	}
	line := truncateVis("quick-send  "+input+status, width)
	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Background(lipgloss.Color(palette.Chrome.Header)).
		Padding(0, 1).
		Width(maxInt(0, width)).
		Render(line)
}

func (m *Model) renderToast(width int, theme Theme) string {
	if strings.TrimSpace(m.toast) == "" || (!m.toastUntil.IsZero() && time.Now().UTC().After(m.toastUntil)) {
		return ""
	}
	palette := themePalette(theme)
	line := truncateVis(m.toast, width)
	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(palette.Priority.High)).
		Background(lipgloss.Color(palette.Base.Background)).
		Padding(0, 1).
		Render(line)
}

func (m *Model) setToast(text string) {
	m.toast = strings.TrimSpace(text)
	m.toastUntil = time.Now().UTC().Add(2 * time.Second)
}

func parseQuickSendInput(input string) (target string, body string, ok bool) {
	trimmed := strings.TrimSpace(input)
	trimmed = strings.TrimPrefix(trimmed, ":")
	trimmed = strings.TrimSpace(trimmed)
	if trimmed == "" {
		return "", "", false
	}
	parts := strings.SplitN(trimmed, " ", 2)
	if len(parts) != 2 {
		return "", "", false
	}
	target = strings.TrimSpace(parts[0])
	body = strings.TrimSpace(parts[1])
	if target == "" || body == "" {
		return "", "", false
	}
	return target, body, true
}

func normalizePriorityInput(value string) string {
	value = strings.TrimSpace(strings.ToLower(value))
	switch value {
	case fmail.PriorityLow, fmail.PriorityHigh, fmail.PriorityNormal:
		return value
	default:
		return fmail.PriorityNormal
	}
}

func parseTagCSV(csv string) []string {
	if strings.TrimSpace(csv) == "" {
		return nil
	}
	parts := strings.Split(csv, ",")
	out := make([]string, 0, len(parts))
	seen := make(map[string]struct{}, len(parts))
	for _, part := range parts {
		tag := strings.TrimSpace(strings.ToLower(part))
		if tag == "" {
			continue
		}
		if _, ok := seen[tag]; ok {
			continue
		}
		seen[tag] = struct{}{}
		out = append(out, tag)
	}
	return out
}

func (m *Model) knownTargets() []string {
	if m.provider == nil {
		return nil
	}
	seen := map[string]struct{}{}
	out := make([]string, 0, 16)

	topics, _ := m.provider.Topics()
	for _, topic := range topics {
		name := strings.TrimSpace(topic.Name)
		if name == "" {
			continue
		}
		if _, ok := seen[name]; ok {
			continue
		}
		seen[name] = struct{}{}
		out = append(out, name)
	}

	dms, _ := m.provider.DMConversations(m.selfAgent)
	for _, dm := range dms {
		target := "@" + strings.TrimSpace(dm.Agent)
		if target == "@" {
			continue
		}
		if _, ok := seen[target]; ok {
			continue
		}
		seen[target] = struct{}{}
		out = append(out, target)
	}

	sort.Strings(out)
	return out
}

func filterPrefix(values []string, prefix string) []string {
	prefix = strings.ToLower(strings.TrimSpace(prefix))
	if len(values) == 0 {
		return nil
	}
	if prefix == "" {
		return append([]string(nil), values...)
	}
	out := make([]string, 0, len(values))
	for _, value := range values {
		if strings.HasPrefix(strings.ToLower(value), prefix) {
			out = append(out, value)
		}
	}
	return out
}

func (m *Model) completeComposeTarget() {
	prefix := strings.TrimSpace(m.compose.to)
	choices := filterPrefix(m.knownTargets(), prefix)
	if len(choices) == 0 {
		return
	}
	if m.compose.toCompletionPrefix != prefix {
		m.compose.toCompletionPrefix = prefix
		m.compose.toCompletionIndex = 0
	} else {
		m.compose.toCompletionIndex = (m.compose.toCompletionIndex + 1) % len(choices)
	}
	m.compose.to = choices[m.compose.toCompletionIndex]
}

func (m *Model) completeQuickTarget() {
	input := m.quick.input
	if strings.TrimSpace(input) == "" {
		input = ":"
	}
	if !strings.HasPrefix(input, ":") {
		input = ":" + input
	}
	rest := strings.TrimPrefix(input, ":")
	if strings.Contains(rest, " ") {
		return
	}
	prefix := strings.TrimSpace(rest)
	choices := filterPrefix(m.knownTargets(), prefix)
	if len(choices) == 0 {
		return
	}
	if m.quick.completionPrefix != prefix {
		m.quick.completionPrefix = prefix
		m.quick.completionIndex = 0
	} else {
		m.quick.completionIndex = (m.quick.completionIndex + 1) % len(choices)
	}
	m.quick.input = ":" + choices[m.quick.completionIndex] + " "
}

func (m *Model) completeComposeTag() {
	target := strings.TrimSpace(m.compose.to)
	if target == "" || m.provider == nil {
		return
	}
	values := m.knownTagsForTarget(target)
	if len(values) == 0 {
		return
	}

	parts := strings.Split(m.compose.tags, ",")
	prefix := strings.TrimSpace(parts[len(parts)-1])
	choices := filterPrefix(values, prefix)
	if len(choices) == 0 {
		return
	}

	if m.compose.tagCompletionPrefix != prefix {
		m.compose.tagCompletionPrefix = prefix
		m.compose.tagCompletionIndex = 0
	} else {
		m.compose.tagCompletionIndex = (m.compose.tagCompletionIndex + 1) % len(choices)
	}

	parts[len(parts)-1] = " " + choices[m.compose.tagCompletionIndex]
	updated := strings.Join(parts, ",")
	m.compose.tags = strings.TrimLeft(updated, " ")
}

func (m *Model) knownTagsForTarget(target string) []string {
	if m.provider == nil {
		return nil
	}
	seen := map[string]struct{}{}
	out := make([]string, 0, 16)
	appendTags := func(tags []string) {
		for _, tag := range tags {
			tag = strings.TrimSpace(strings.ToLower(tag))
			if tag == "" {
				continue
			}
			if _, ok := seen[tag]; ok {
				continue
			}
			seen[tag] = struct{}{}
			out = append(out, tag)
		}
	}

	if strings.HasPrefix(target, "@") {
		msgs, _ := m.provider.DMs(strings.TrimPrefix(target, "@"), data.MessageFilter{To: "@" + m.selfAgent, Limit: 200})
		for _, msg := range msgs {
			appendTags(msg.Tags)
		}
	} else {
		msgs, _ := m.provider.Messages(target, data.MessageFilter{Limit: 200})
		for _, msg := range msgs {
			appendTags(msg.Tags)
		}
	}

	sort.Strings(out)
	return out
}

func (m *Model) cyclePriority(delta int) {
	current := normalizePriorityInput(m.compose.priority)
	idx := 1
	for i := range composePriorities {
		if composePriorities[i] == current {
			idx = i
			break
		}
	}
	next := (idx + delta + len(composePriorities)) % len(composePriorities)
	m.compose.priority = composePriorities[next]
}

func (m *Model) composeDeleteRune() {
	switch m.compose.focus {
	case composeFieldTo:
		if len(m.compose.to) == 0 {
			return
		}
		runes := []rune(m.compose.to)
		m.compose.to = string(runes[:len(runes)-1])
	case composeFieldPriority:
		if len(m.compose.priority) == 0 {
			return
		}
		runes := []rune(m.compose.priority)
		m.compose.priority = string(runes[:len(runes)-1])
	case composeFieldTags:
		if len(m.compose.tags) == 0 {
			return
		}
		runes := []rune(m.compose.tags)
		m.compose.tags = string(runes[:len(runes)-1])
	case composeFieldBody:
		if len(m.compose.body) == 0 {
			return
		}
		runes := []rune(m.compose.body)
		m.compose.body = string(runes[:len(runes)-1])
	}
	m.resetComposeCompletionState()
}

func (m *Model) resetComposeCompletionState() {
	m.compose.toCompletionPrefix = ""
	m.compose.toCompletionIndex = -1
	m.compose.tagCompletionPrefix = ""
	m.compose.tagCompletionIndex = -1
}

func (m *Model) resetQuickCompletion() {
	m.quick.completionPrefix = ""
	m.quick.completionIndex = -1
}

func (m *Model) persistDraft(save bool) {
	if m.tuiState == nil {
		return
	}
	target := strings.TrimSpace(m.compose.to)
	if target == "" {
		return
	}
	if !save {
		m.tuiState.DeleteDraft(target)
		m.tuiState.SaveSoon()
		return
	}
	m.tuiState.SetDraft(tuistate.ComposeDraft{
		Target:    target,
		To:        target,
		Priority:  normalizePriorityInput(m.compose.priority),
		Tags:      strings.TrimSpace(m.compose.tags),
		ReplyTo:   strings.TrimSpace(m.compose.replyTo),
		Body:      strings.TrimSpace(m.compose.body),
		UpdatedAt: time.Now().UTC(),
	})
	m.tuiState.SaveSoon()
}
