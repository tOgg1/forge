package fmailtui

import (
	"fmt"
	"os"
	"sort"
	"strconv"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

const (
	operatorRefreshInterval  = 2 * time.Second
	operatorPresenceInterval = 30 * time.Second
	operatorMessageLimit     = 250
	operatorActiveWindow     = 10 * time.Minute
)

type operatorTickMsg struct{}

type operatorPresenceTickMsg struct{}

type operatorLoadedMsg struct {
	now        time.Time
	target     string
	selected   int
	convs      []operatorConversation
	quick      []string
	unread     int
	agents     []fmail.AgentRecord
	messages   []fmail.Message
	replyIndex map[string]fmail.Message
	err        error
}

type operatorIncomingMsg struct {
	msg fmail.Message
}

type operatorSendResultMsg struct {
	count int
	sent  []fmail.Message
	err   error
}

type operatorConversation struct {
	Target       string
	LastActivity time.Time
	Unread       int
	LastMessage  *fmail.Message
}

type operatorView struct {
	root      string
	projectID string
	self      string
	host      string
	store     *fmail.Store
	provider  data.MessageProvider
	tuiState  *tuistate.Manager

	width  int
	height int

	sidebarCollapsed bool
	convs            []operatorConversation
	quickTargets     []string
	selected         int
	target           string
	messages         []fmail.Message
	replyIndex       map[string]fmail.Message
	scroll           int
	follow           bool
	agents           []fmail.AgentRecord
	unreadTotal      int
	lastLoaded       time.Time

	compose          string
	composePriority  string
	composeTags      []string
	composeMultiline bool
	showPalette      bool

	groups         map[string][]string
	pendingApprove string
	waitingSince   map[string]time.Time

	statusLine string
	statusErr  error

	subCh  <-chan fmail.Message
	cancel func()
}

func operatorTickCmd() tea.Cmd {
	return tea.Tick(operatorRefreshInterval, func(time.Time) tea.Msg { return operatorTickMsg{} })
}

func operatorPresenceTickCmd() tea.Cmd {
	return tea.Tick(operatorPresenceInterval, func(time.Time) tea.Msg { return operatorPresenceTickMsg{} })
}

func newOperatorView(root, projectID, self string, store *fmail.Store, provider data.MessageProvider, st *tuistate.Manager) *operatorView {
	host, _ := os.Hostname()
	v := &operatorView{
		root:            root,
		projectID:       projectID,
		self:            strings.TrimSpace(self),
		host:            strings.TrimSpace(host),
		store:           store,
		provider:        provider,
		tuiState:        st,
		follow:          true,
		composePriority: fmail.PriorityNormal,
		groups:          map[string][]string{},
		waitingSince:    map[string]time.Time{},
		replyIndex:      map[string]fmail.Message{},
	}
	if st != nil {
		v.groups = st.Groups()
		if v.groups == nil {
			v.groups = map[string][]string{}
		}
	}
	return v
}

func (v *operatorView) Init() tea.Cmd {
	v.startSubscription()
	v.touchPresence("")
	return tea.Batch(v.loadCmd(), operatorTickCmd(), operatorPresenceTickCmd(), v.waitForMessageCmd())
}

func (v *operatorView) Close() {
	if v.cancel != nil {
		v.cancel()
		v.cancel = nil
	}
	v.touchPresence("offline")
}

func (v *operatorView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case operatorTickMsg:
		return tea.Batch(v.loadCmd(), operatorTickCmd())
	case operatorPresenceTickMsg:
		v.touchPresence("")
		return operatorPresenceTickCmd()
	case operatorLoadedMsg:
		v.applyLoaded(typed)
		return nil
	case operatorIncomingMsg:
		v.handleIncoming(typed.msg)
		return v.waitForMessageCmd()
	case operatorSendResultMsg:
		if typed.err != nil {
			v.statusErr = typed.err
			v.statusLine = ""
		} else {
			v.statusErr = nil
			v.statusLine = fmt.Sprintf("sent %d message(s)", typed.count)
		}
		for _, sent := range typed.sent {
			if hasAnyTag(sent.Tags, "question") {
				v.waitingSince[sent.ID] = sent.Time
			}
		}
		return v.loadCmd()
	case tea.WindowSizeMsg:
		v.width = typed.Width
		v.height = typed.Height
		return nil
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *operatorView) View(width, height int, theme Theme) string {
	v.width = width
	v.height = height

	palette, ok := styles.Themes[string(theme)]
	if !ok {
		palette = styles.DefaultTheme
	}
	if width <= 0 || height <= 0 {
		return ""
	}

	composePanel := v.renderComposePanel(width, palette)
	quick := v.renderQuickActions(width, palette)
	ticker := v.renderStatusTicker(width, palette)

	reserved := lipgloss.Height(composePanel) + lipgloss.Height(quick) + lipgloss.Height(ticker)
	conversationHeight := maxInt(4, height-reserved)
	conversation := v.renderConversationArea(width, conversationHeight, palette)

	parts := []string{conversation, quick, ticker, composePanel}
	if v.showPalette {
		parts = append(parts, v.renderCommandPalette(width, palette))
	}
	if strings.TrimSpace(v.statusLine) != "" {
		parts = append(parts, v.renderStatusLine(width, palette, false))
	}
	if v.statusErr != nil {
		parts = append(parts, v.renderStatusLine(width, palette, true))
	}
	return lipgloss.JoinVertical(lipgloss.Left, parts...)
}

func (v *operatorView) ComposeTarget() string {
	return strings.TrimSpace(v.target)
}

func (v *operatorView) ComposeReplySeed(dmDirect bool) (composeReplySeed, bool) {
	if len(v.messages) == 0 {
		return composeReplySeed{}, false
	}
	last := v.messages[len(v.messages)-1]
	target := strings.TrimSpace(v.target)
	if dmDirect && !strings.HasPrefix(target, "@") {
		return composeReplySeed{}, false
	}
	if strings.EqualFold(strings.TrimSpace(last.From), v.self) {
		return composeReplySeed{}, false
	}
	return composeReplySeed{
		Target:     target,
		ReplyTo:    strings.TrimSpace(last.ID),
		ParentLine: firstNonEmptyLine(messageBodyString(last.Body)),
	}, true
}

func (v *operatorView) handleKey(msg tea.KeyMsg) tea.Cmd {
	if v.showPalette {
		switch msg.String() {
		case "ctrl+p", "esc":
			v.showPalette = false
			return nil
		}
	}

	switch msg.String() {
	case "ctrl+p":
		v.showPalette = !v.showPalette
		return nil
	case "ctrl+b":
		v.sidebarCollapsed = !v.sidebarCollapsed
		return nil
	case "tab":
		v.selectConversation(1)
		return v.loadCmd()
	case "shift+tab":
		v.selectConversation(-1)
		return v.loadCmd()
	case "up":
		v.scroll += 3
		v.follow = false
		return nil
	case "down":
		v.scroll -= 3
		if v.scroll <= 0 {
			v.scroll = 0
			v.follow = true
		}
		return nil
	case "pgup":
		v.scroll += 12
		v.follow = false
		return nil
	case "pgdown":
		v.scroll -= 12
		if v.scroll <= 0 {
			v.scroll = 0
			v.follow = true
		}
		return nil
	case "home":
		v.scroll = 1 << 20
		v.follow = false
		return nil
	case "end":
		v.scroll = 0
		v.follow = true
		return nil
	case "ctrl+m":
		v.composeMultiline = !v.composeMultiline
		v.statusLine = fmt.Sprintf("compose mode: %s", map[bool]string{true: "multi-line", false: "single-line"}[v.composeMultiline])
		v.statusErr = nil
		return nil
	case "ctrl+enter", "ctrl+j":
		return v.submitCompose()
	case "enter":
		if v.composeMultiline {
			v.compose += "\n"
			return nil
		}
		return v.submitCompose()
	case "backspace", "delete", "ctrl+h":
		if len(v.compose) == 0 {
			return nil
		}
		r := []rune(v.compose)
		v.compose = string(r[:len(r)-1])
		return nil
	case "ctrl+a":
		if strings.TrimSpace(v.compose) == "" {
			v.compose = "/broadcast "
		}
		return nil
	case "n":
		if strings.TrimSpace(v.compose) == "" {
			v.target = ""
			v.statusErr = nil
			v.statusLine = "new message: use /dm <agent> or /topic <topic>"
		}
		return nil
	case "y":
		if strings.TrimSpace(v.pendingApprove) != "" && strings.TrimSpace(v.compose) == "" {
			v.compose = "/approve " + strings.TrimSpace(v.pendingApprove)
			return v.submitCompose()
		}
		return nil
	case "x":
		if strings.TrimSpace(v.pendingApprove) != "" && strings.TrimSpace(v.compose) == "" {
			v.compose = "/reject " + strings.TrimSpace(v.pendingApprove) + " "
		}
		return nil
	case "esc":
		if strings.TrimSpace(v.compose) != "" {
			v.compose = ""
			return nil
		}
		return popViewCmd()
	}

	if len(msg.Runes) == 1 {
		r := msg.Runes[0]
		if r >= '1' && r <= '9' && strings.TrimSpace(v.compose) == "" {
			idx := int(r - '1')
			if idx >= 0 && idx < len(v.quickTargets) {
				v.target = v.quickTargets[idx]
				v.scroll = 0
				v.follow = true
				return v.loadCmd()
			}
		}
	}

	if msg.Type == tea.KeyRunes {
		v.compose += string(msg.Runes)
		return nil
	}
	return nil
}

func (v *operatorView) submitCompose() tea.Cmd {
	text := strings.TrimSpace(v.compose)
	if text == "" {
		v.statusErr = fmt.Errorf("message body is empty")
		return nil
	}
	if strings.HasPrefix(text, "/") {
		cmd, handled := v.handleSlashCommand(text)
		if handled {
			if v.statusErr == nil {
				v.compose = ""
			}
			return cmd
		}
	}
	target := strings.TrimSpace(v.target)
	if target == "" {
		v.statusErr = fmt.Errorf("missing target")
		return nil
	}
	req := data.SendRequest{
		From:     v.self,
		To:       target,
		Body:     text,
		Priority: normalizePriorityInput(v.composePriority),
		Tags:     append([]string(nil), v.composeTags...),
	}
	v.statusErr = nil
	v.compose = ""
	return v.sendRequests([]data.SendRequest{req})
}

func (v *operatorView) sendRequests(requests []data.SendRequest) tea.Cmd {
	if len(requests) == 0 {
		return nil
	}
	provider := v.provider
	store := v.store
	if store == nil {
		created, err := fmail.NewStore(v.root)
		if err == nil {
			store = created
		}
	}
	return func() tea.Msg {
		sent := make([]fmail.Message, 0, len(requests))
		for _, req := range requests {
			if sender, ok := provider.(providerSender); ok {
				msg, err := sender.Send(req)
				if err != nil {
					return operatorSendResultMsg{count: len(sent), sent: sent, err: err}
				}
				sent = append(sent, msg)
				continue
			}
			if store == nil {
				return operatorSendResultMsg{count: len(sent), sent: sent, err: fmt.Errorf("missing sender and store")}
			}
			msg := &fmail.Message{
				From:     strings.TrimSpace(req.From),
				To:       strings.TrimSpace(req.To),
				Body:     strings.TrimSpace(req.Body),
				ReplyTo:  strings.TrimSpace(req.ReplyTo),
				Priority: normalizePriorityInput(req.Priority),
				Tags:     append([]string(nil), req.Tags...),
			}
			if _, err := store.SaveMessage(msg); err != nil {
				return operatorSendResultMsg{count: len(sent), sent: sent, err: err}
			}
			sent = append(sent, *msg)
		}
		return operatorSendResultMsg{count: len(sent), sent: sent}
	}
}

func (v *operatorView) applyLoaded(msg operatorLoadedMsg) {
	if msg.err != nil {
		v.statusErr = msg.err
		return
	}
	v.lastLoaded = msg.now
	v.convs = msg.convs
	v.quickTargets = msg.quick
	v.unreadTotal = msg.unread
	v.agents = msg.agents
	v.selected = clampInt(msg.selected, 0, maxInt(0, len(v.convs)-1))
	v.target = strings.TrimSpace(msg.target)
	v.messages = msg.messages
	v.replyIndex = msg.replyIndex
	if len(v.messages) == 0 {
		v.scroll = 0
	}
	if v.follow {
		v.scroll = 0
	}
	v.statusErr = nil
}

func (v *operatorView) handleIncoming(msg fmail.Message) {
	target := messageTargetForSelf(v.self, msg)
	if target == "" {
		return
	}

	if !conversationExists(v.convs, target) {
		v.convs = append(v.convs, operatorConversation{Target: target, LastActivity: msg.Time})
	}
	for i := range v.convs {
		if v.convs[i].Target != target {
			continue
		}
		v.convs[i].LastActivity = msg.Time
		if target != strings.TrimSpace(v.target) {
			v.convs[i].Unread++
			v.unreadTotal++
		}
		break
	}
	sortConversations(v.convs)
	v.quickTargets = selectQuickTargets(v.convs, 9)

	if target == strings.TrimSpace(v.target) {
		v.messages = append(v.messages, msg)
		if strings.TrimSpace(msg.ID) != "" {
			v.replyIndex[strings.TrimSpace(msg.ID)] = msg
			v.markRead(target, msg.ID)
		}
		if v.follow {
			v.scroll = 0
		}
	} else if strings.HasPrefix(target, "@") && strings.EqualFold(strings.TrimPrefix(strings.TrimSpace(msg.To), "@"), v.self) {
		v.statusLine = "\aDM from @" + strings.TrimSpace(msg.From)
	}

	if hasAnyTag(msg.Tags, "question", "needs-approval") {
		v.pendingApprove = strings.TrimSpace(msg.ID)
	}
}

func (v *operatorView) loadCmd() tea.Cmd {
	provider := v.provider
	self := v.self
	selected := v.selected
	target := v.target
	state := v.tuiState

	return func() tea.Msg {
		now := time.Now().UTC()
		if provider == nil {
			return operatorLoadedMsg{now: now, err: fmt.Errorf("missing provider")}
		}
		convs, unread, err := loadOperatorConversations(provider, state, self)
		if err != nil {
			return operatorLoadedMsg{now: now, err: err}
		}
		target, selected = pickOperatorTarget(convs, target, selected)

		messages, replyIndex, err := loadOperatorMessages(provider, target, self)
		if err != nil {
			return operatorLoadedMsg{now: now, err: err}
		}
		agents, err := provider.Agents()
		if err != nil {
			return operatorLoadedMsg{now: now, err: err}
		}

		if state != nil && target != "" && len(messages) > 0 {
			last := strings.TrimSpace(messages[len(messages)-1].ID)
			if last != "" {
				state.SetReadMarker(target, last)
				state.SaveSoon()
			}
		}

		return operatorLoadedMsg{
			now:        now,
			target:     target,
			selected:   selected,
			convs:      convs,
			quick:      selectQuickTargets(convs, 9),
			unread:     unread,
			agents:     agents,
			messages:   messages,
			replyIndex: replyIndex,
		}
	}
}

func (v *operatorView) renderConversationArea(width, height int, palette styles.Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	if v.sidebarCollapsed {
		return v.renderConversationPanel(width, height, palette)
	}
	sidebarW := minInt(34, maxInt(24, width/4))
	mainW := maxInt(16, width-sidebarW-1)
	sidebar := v.renderConversationList(sidebarW, height, palette)
	main := v.renderConversationPanel(mainW, height, palette)
	return lipgloss.JoinHorizontal(lipgloss.Top, sidebar, " ", main)
}

func (v *operatorView) renderConversationList(width, height int, palette styles.Theme) string {
	title := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render("Conversations")
	lines := []string{title}
	maxRows := maxInt(1, height-1)
	if len(v.convs) == 0 {
		lines = append(lines, lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("No conversations"))
	} else {
		for i, conv := range v.convs {
			if len(lines) >= maxRows {
				break
			}
			prefix := "  "
			if i == v.selected {
				prefix = "> "
			}
			name := truncateVis(conv.Target, maxInt(8, width-14))
			badge := ""
			if conv.Unread > 0 {
				badge = fmt.Sprintf(" [%d]", conv.Unread)
			}
			stamp := ""
			if !conv.LastActivity.IsZero() {
				stamp = " " + conv.LastActivity.Format("15:04")
			}
			line := truncateVis(prefix+name+badge+stamp, maxInt(0, width-2))
			if i == v.selected {
				line = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Bold(true).Render(line)
			}
			lines = append(lines, line)
		}
	}
	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(palette.Borders.Divider)).
		Padding(0, 1).
		Width(width).
		Height(height).
		Render(clampLines(strings.Join(lines, "\n"), maxInt(0, height-2)))
}

func (v *operatorView) renderConversationPanel(width, height int, palette styles.Theme) string {
	head := "Operator Console"
	if target := strings.TrimSpace(v.target); target != "" {
		head = "Conversation with " + target
	}
	headLine := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render(head)
	bodyHeight := maxInt(1, height-2)
	bodyLines := v.renderConversationLines(width-4, bodyHeight, palette)
	content := append([]string{headLine}, bodyLines...)
	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(palette.Borders.ActivePane)).
		Padding(0, 1).
		Width(width).
		Height(height).
		Render(clampLines(strings.Join(content, "\n"), maxInt(0, height-2)))
}

func (v *operatorView) renderConversationLines(width, height int, palette styles.Theme) []string {
	if width <= 0 || height <= 0 {
		return nil
	}
	if strings.TrimSpace(v.target) == "" {
		return []string{lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("Select a conversation (Tab / 1-9) or start with /dm")}
	}
	if len(v.messages) == 0 {
		return []string{lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("No messages yet")}
	}

	lines := make([]string, 0, len(v.messages)*3)
	var prev fmail.Message
	for idx, msg := range v.messages {
		if idx == 0 || !sameMessageGroup(prev, msg) {
			stamp := msg.Time.Format("15:04")
			lines = append(lines, lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("  "+stamp))
		}

		isMine := strings.EqualFold(strings.TrimSpace(msg.From), v.self)
		author := strings.TrimSpace(msg.From)
		if isMine {
			author = "you"
		}
		head := author
		if hasAnyTag(msg.Tags, "question", "needs-approval") {
			head += " ?"
			if strings.TrimSpace(msg.ID) != "" {
				v.pendingApprove = strings.TrimSpace(msg.ID)
			}
		}
		headStyle := lipgloss.NewStyle().Bold(true)
		if isMine {
			headStyle = headStyle.Foreground(lipgloss.Color(palette.Chrome.Breadcrumb))
		} else {
			headStyle = headStyle.Foreground(lipgloss.Color(palette.Base.Foreground))
		}
		headLine := "• " + headStyle.Render(head)
		if isMine {
			headLine = lipgloss.PlaceHorizontal(maxInt(0, width), lipgloss.Right, headLine)
		}
		lines = append(lines, headLine)

		if preview := v.replyPreview(strings.TrimSpace(msg.ReplyTo)); preview != "" {
			previewLine := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("  ↪ " + truncateVis(preview, maxInt(0, width-4)))
			if isMine {
				previewLine = lipgloss.PlaceHorizontal(maxInt(0, width), lipgloss.Right, previewLine)
			}
			lines = append(lines, previewLine)
		}

		wrapped := wrapLines(strings.TrimSpace(messageBodyString(msg.Body)), maxInt(12, width-4))
		if len(wrapped) == 0 {
			wrapped = []string{""}
		}
		for _, line := range wrapped {
			line = "  " + line
			if isMine {
				line = lipgloss.PlaceHorizontal(maxInt(0, width), lipgloss.Right, line)
			}
			lines = append(lines, line)
		}
		prev = msg
	}

	if indicator := v.typingIndicator(); indicator != "" {
		lines = append(lines, lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Status.Recent)).Render(indicator))
	}
	if wait := v.waitingIndicator(); wait != "" {
		lines = append(lines, lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Render(wait))
	}

	visible := maxInt(1, height)
	maxScroll := maxInt(0, len(lines)-visible)
	v.scroll = clampInt(v.scroll, 0, maxScroll)
	start := maxInt(0, len(lines)-visible-v.scroll)
	end := minInt(len(lines), start+visible)
	window := append([]string(nil), lines[start:end]...)
	if v.scroll > 0 && len(window) > 0 {
		window[0] = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("… older …")
	}
	return window
}

func (v *operatorView) renderQuickActions(width int, palette styles.Theme) string {
	items := make([]string, 0, len(v.quickTargets))
	for i, target := range v.quickTargets {
		item := fmt.Sprintf("[%d] %s", i+1, target)
		if target == strings.TrimSpace(v.target) {
			item = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).Render(item)
		}
		items = append(items, item)
	}
	if len(items) == 0 {
		items = append(items, "No quick targets yet")
	}
	line := "quick: " + strings.Join(items, "  ")
	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Background(lipgloss.Color(palette.Chrome.Header)).
		Padding(0, 1).
		Width(maxInt(0, width)).
		Render(truncateVis(line, maxInt(0, width-2)))
}

func (v *operatorView) renderStatusTicker(width int, palette styles.Theme) string {
	if len(v.agents) == 0 {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("agents: none")
	}
	now := time.Now().UTC()
	records := append([]fmail.AgentRecord(nil), v.agents...)
	sort.SliceStable(records, func(i, j int) bool {
		iActive := now.Sub(records[i].LastSeen) <= operatorActiveWindow
		jActive := now.Sub(records[j].LastSeen) <= operatorActiveWindow
		if iActive != jActive {
			return iActive
		}
		return records[i].Name < records[j].Name
	})
	parts := make([]string, 0, minInt(len(records), 6))
	for i, rec := range records {
		if i >= 6 {
			break
		}
		dot := "○"
		if now.Sub(rec.LastSeen) <= operatorActiveWindow {
			dot = "●"
		}
		status := strings.TrimSpace(rec.Status)
		if status != "" {
			status = " " + truncate(status, 16)
		}
		parts = append(parts, dot+" "+rec.Name+status)
	}
	line := strings.Join(parts, "  ")
	if v.unreadTotal > 0 {
		line = fmt.Sprintf("[N:%d] %s", v.unreadTotal, line)
	}
	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Background(lipgloss.Color(palette.Chrome.Footer)).
		Padding(0, 1).
		Width(maxInt(0, width)).
		Render(truncateVis(line, maxInt(0, width-2)))
}

func (v *operatorView) renderComposePanel(width int, palette styles.Theme) string {
	mode := "single-line"
	if v.composeMultiline {
		mode = "multi-line"
	}
	meta := fmt.Sprintf("To: %s  Priority: %s  Tags: %s  Mode: %s", defaultIfEmpty(v.target, "(set via /dm or /topic)"), normalizePriorityInput(v.composePriority), strings.Join(v.composeTags, ","), mode)

	body := v.compose
	if body == "" {
		body = ""
	}
	if !v.composeMultiline {
		body = strings.ReplaceAll(body, "\n", " ")
	}
	cursor := "_"
	bodyLines := wrapLines(body+cursor, maxInt(8, width-6))
	if len(bodyLines) > 4 {
		bodyLines = bodyLines[len(bodyLines)-4:]
	}
	content := []string{
		truncateVis(meta, maxInt(0, width-4)),
		"> " + strings.Join(bodyLines, "\n  "),
		"Enter: send (single-line) | Ctrl+Enter: send | Ctrl+M: toggle multiline | Ctrl+P: commands",
	}
	return lipgloss.NewStyle().
		Border(lipgloss.NormalBorder()).
		BorderForeground(lipgloss.Color(palette.Borders.Divider)).
		Padding(0, 1).
		Width(width).
		Render(strings.Join(content, "\n"))
}

func (v *operatorView) renderCommandPalette(width int, palette styles.Theme) string {
	commands := []string{
		"/dm <agent> [msg]",
		"/topic <name> [msg]",
		"/broadcast <msg>",
		"/status",
		"/assign <agent> <task>",
		"/ask <agent> <question>",
		"/approve <msg-id>",
		"/reject <msg-id> <reason>",
		"/priority high|normal|low",
		"/tag <tags...>",
		"/group create <name> <agents...>",
		"/group <name> <msg>",
		"/mystatus <text>",
	}
	line := "commands: " + strings.Join(commands, "  |  ")
	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Background(lipgloss.Color(palette.Base.Border)).
		Padding(0, 1).
		Width(width).
		Render(truncateVis(line, maxInt(0, width-2)))
}

func (v *operatorView) renderStatusLine(width int, palette styles.Theme, isErr bool) string {
	text := strings.TrimSpace(v.statusLine)
	if isErr {
		text = strings.TrimSpace(v.statusErr.Error())
	}
	if text == "" {
		return ""
	}
	style := lipgloss.NewStyle().Padding(0, 1).Width(width)
	if isErr {
		style = style.Foreground(lipgloss.Color(palette.Priority.High))
	} else {
		style = style.Foreground(lipgloss.Color(palette.Status.Online))
	}
	return style.Render(truncateVis(text, maxInt(0, width-2)))
}

func (v *operatorView) waitingIndicator() string {
	if len(v.waitingSince) == 0 {
		return ""
	}
	now := time.Now().UTC()
	count := 0
	for id, asked := range v.waitingSince {
		if asked.IsZero() {
			delete(v.waitingSince, id)
			continue
		}
		if now.Sub(asked) >= 5*time.Minute {
			count++
		}
	}
	if count == 0 {
		return ""
	}
	return fmt.Sprintf("waiting for reply: %d", count)
}

func (v *operatorView) typingIndicator() string {
	target := strings.TrimSpace(v.target)
	if !strings.HasPrefix(target, "@") {
		return ""
	}
	agent := strings.TrimPrefix(target, "@")
	now := time.Now().UTC()
	for _, rec := range v.agents {
		if !strings.EqualFold(strings.TrimSpace(rec.Name), agent) {
			continue
		}
		status := strings.ToLower(strings.TrimSpace(rec.Status))
		if status == "" {
			return ""
		}
		if now.Sub(rec.LastSeen) > operatorActiveWindow {
			return ""
		}
		if strings.Contains(status, "typing") || strings.Contains(status, "working") || strings.Contains(status, "fix") {
			return fmt.Sprintf("%s is working...", agent)
		}
		return ""
	}
	return ""
}

func (v *operatorView) replyPreview(replyTo string) string {
	replyTo = strings.TrimSpace(replyTo)
	if replyTo == "" {
		return ""
	}
	msg, ok := v.replyIndex[replyTo]
	if !ok {
		return ""
	}
	return firstNonEmptyLine(messageBodyString(msg.Body))
}

func (v *operatorView) selectConversation(delta int) {
	if len(v.convs) == 0 {
		return
	}
	idx := clampInt(v.selected+delta, 0, len(v.convs)-1)
	v.selected = idx
	v.target = v.convs[idx].Target
	v.follow = true
	v.scroll = 0
}

func (v *operatorView) touchPresence(status string) {
	if v.store == nil || strings.TrimSpace(v.self) == "" {
		return
	}
	_, _ = v.store.SetAgentStatus(v.self, status, v.host)
}

func (v *operatorView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.cancel = cancel
}

func (v *operatorView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return operatorIncomingMsg{msg: msg}
	}
}

func (v *operatorView) markRead(target, id string) {
	if v.tuiState == nil || strings.TrimSpace(target) == "" || strings.TrimSpace(id) == "" {
		return
	}
	v.tuiState.SetReadMarker(target, id)
	v.tuiState.SaveSoon()
}

func loadOperatorConversations(provider data.MessageProvider, st *tuistate.Manager, self string) ([]operatorConversation, int, error) {
	topics, err := provider.Topics()
	if err != nil {
		return nil, 0, err
	}
	dms, err := provider.DMConversations(self)
	if err != nil {
		return nil, 0, err
	}

	convs := make([]operatorConversation, 0, len(topics)+len(dms))
	unreadTotal := 0
	for _, topic := range topics {
		target := strings.TrimSpace(topic.Name)
		if target == "" {
			continue
		}
		unread := 0
		if st != nil {
			marker := strings.TrimSpace(st.ReadMarker(target))
			lastID := ""
			if topic.LastMessage != nil {
				lastID = strings.TrimSpace(topic.LastMessage.ID)
			}
			if marker != "" && lastID != "" && lastID > marker {
				unread = 1
			}
		}
		convs = append(convs, operatorConversation{
			Target:       target,
			LastActivity: topic.LastActivity,
			Unread:       unread,
			LastMessage:  topic.LastMessage,
		})
		unreadTotal += unread
	}
	for _, dm := range dms {
		target := "@" + strings.TrimSpace(dm.Agent)
		if target == "@" {
			continue
		}
		unread := maxInt(0, dm.UnreadCount)
		convs = append(convs, operatorConversation{
			Target:       target,
			LastActivity: dm.LastActivity,
			Unread:       unread,
		})
		unreadTotal += unread
	}
	sortConversations(convs)
	return convs, unreadTotal, nil
}

func loadOperatorMessages(provider data.MessageProvider, target string, self string) ([]fmail.Message, map[string]fmail.Message, error) {
	target = strings.TrimSpace(target)
	if target == "" {
		return nil, map[string]fmail.Message{}, nil
	}
	var (
		msgs []fmail.Message
		err  error
	)
	if strings.HasPrefix(target, "@") {
		msgs, err = provider.DMs(strings.TrimPrefix(target, "@"), data.MessageFilter{Limit: operatorMessageLimit})
	} else {
		msgs, err = provider.Messages(target, data.MessageFilter{Limit: operatorMessageLimit})
	}
	if err != nil {
		return nil, nil, err
	}
	sort.SliceStable(msgs, func(i, j int) bool {
		if !msgs[i].Time.Equal(msgs[j].Time) {
			return msgs[i].Time.Before(msgs[j].Time)
		}
		return strings.TrimSpace(msgs[i].ID) < strings.TrimSpace(msgs[j].ID)
	})
	replies := make(map[string]fmail.Message, len(msgs))
	for _, msg := range msgs {
		if id := strings.TrimSpace(msg.ID); id != "" {
			replies[id] = msg
		}
	}
	_ = self
	return msgs, replies, nil
}

func pickOperatorTarget(convs []operatorConversation, target string, selected int) (string, int) {
	if len(convs) == 0 {
		return "", 0
	}
	target = strings.TrimSpace(target)
	if target != "" {
		for idx := range convs {
			if convs[idx].Target == target {
				return target, idx
			}
		}
	}
	selected = clampInt(selected, 0, len(convs)-1)
	return convs[selected].Target, selected
}

func messageTargetForSelf(self string, msg fmail.Message) string {
	to := strings.TrimSpace(msg.To)
	if !strings.HasPrefix(to, "@") {
		return to
	}
	peer := dmPeerForSelf(self, msg)
	if peer == "" {
		peer = strings.TrimPrefix(to, "@")
	}
	if peer == "" {
		return ""
	}
	return "@" + peer
}

func sortConversations(convs []operatorConversation) {
	sort.SliceStable(convs, func(i, j int) bool {
		if !convs[i].LastActivity.Equal(convs[j].LastActivity) {
			return convs[i].LastActivity.After(convs[j].LastActivity)
		}
		return convs[i].Target < convs[j].Target
	})
}

func selectQuickTargets(convs []operatorConversation, max int) []string {
	if len(convs) == 0 || max <= 0 {
		return nil
	}
	if len(convs) < max {
		max = len(convs)
	}
	out := make([]string, 0, max)
	for i := 0; i < max; i++ {
		out = append(out, convs[i].Target)
	}
	return out
}

func hasAnyTag(tags []string, names ...string) bool {
	if len(tags) == 0 || len(names) == 0 {
		return false
	}
	set := make(map[string]struct{}, len(names))
	for _, name := range names {
		set[strings.ToLower(strings.TrimSpace(name))] = struct{}{}
	}
	for _, tag := range tags {
		if _, ok := set[strings.ToLower(strings.TrimSpace(tag))]; ok {
			return true
		}
	}
	return false
}

func defaultIfEmpty(value, fallback string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return fallback
	}
	return value
}

func sameMessageGroup(prev, next fmail.Message) bool {
	if !strings.EqualFold(strings.TrimSpace(prev.From), strings.TrimSpace(next.From)) {
		return false
	}
	if prev.Time.IsZero() || next.Time.IsZero() {
		return false
	}
	delta := next.Time.Sub(prev.Time)
	if delta < 0 {
		delta = -delta
	}
	return delta <= 2*time.Minute
}

func conversationExists(convs []operatorConversation, target string) bool {
	for _, conv := range convs {
		if conv.Target == target {
			return true
		}
	}
	return false
}

func parsePositiveInt(value string) (int, bool) {
	n, err := strconv.Atoi(strings.TrimSpace(value))
	if err != nil || n <= 0 {
		return 0, false
	}
	return n, true
}
