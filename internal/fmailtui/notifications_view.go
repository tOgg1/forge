package fmailtui

import (
	"fmt"
	"strconv"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

type notificationsFocus int

const (
	notificationFocusItems notificationsFocus = iota
	notificationFocusRules
)

type notificationsView struct {
	self     string
	provider data.MessageProvider
	center   *notificationCenter

	focus   notificationsFocus
	itemIdx int
	ruleIdx int

	editActive bool
	editIndex  int // -1 = create new
	editInput  string
	statusLine string
	statusErr  bool
}

func newNotificationsView(self string, provider data.MessageProvider, center *notificationCenter) *notificationsView {
	return &notificationsView{
		self:      strings.TrimSpace(self),
		provider:  provider,
		center:    center,
		editIndex: -1,
	}
}

func (v *notificationsView) Init() tea.Cmd {
	v.clampSelection()
	return nil
}

func (v *notificationsView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case tea.KeyMsg:
		if v.editActive {
			return v.handleEditKey(typed)
		}
		return v.handleListKey(typed)
	}
	return nil
}

func (v *notificationsView) MinSize() (int, int) {
	return 68, 18
}

func (v *notificationsView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	palette := themePalette(theme)
	base := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Background(lipgloss.Color(palette.Base.Background))
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	accent := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true)
	errStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Bold(true)

	lines := []string{
		accent.Render(fmt.Sprintf("Notifications (%d unread)", v.unreadCount())),
		muted.Render("Enter open  x dismiss  c clear  Tab section  n/e/d rule edit  Space toggle rule"),
	}
	if v.focus == notificationFocusItems {
		lines = append(lines, muted.Render("focus: notifications"))
	} else {
		lines = append(lines, muted.Render("focus: rules"))
	}

	noteLines := v.renderNotificationLines(maxInt(0, width-2), maxInt(5, height/2), palette)
	ruleLines := v.renderRuleLines(maxInt(0, width-2), maxInt(4, height-len(lines)-len(noteLines)-4), palette)
	lines = append(lines, noteLines...)
	lines = append(lines, muted.Render(strings.Repeat("-", maxInt(0, width-2))))
	lines = append(lines, ruleLines...)

	if v.editActive {
		lines = append(lines, "")
		lines = append(lines, accent.Render("rule editor (Enter save, Ctrl+T test, Esc cancel)"))
		lines = append(lines, truncateVis("rule> "+v.editInput, maxInt(0, width-2)))
	}
	if strings.TrimSpace(v.statusLine) != "" {
		if v.statusErr {
			lines = append(lines, errStyle.Render(truncateVis(v.statusLine, maxInt(0, width-2))))
		} else {
			lines = append(lines, muted.Render(truncateVis(v.statusLine, maxInt(0, width-2))))
		}
	}
	return base.Render(joinClampedLines(lines, height))
}

func (v *notificationsView) handleListKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "esc", "backspace":
		return popViewCmd()
	case "tab":
		if v.focus == notificationFocusItems {
			v.focus = notificationFocusRules
		} else {
			v.focus = notificationFocusItems
		}
		v.statusLine = ""
		v.statusErr = false
		v.clampSelection()
		return nil
	case "j", "down":
		if v.focus == notificationFocusItems {
			v.itemIdx++
		} else {
			v.ruleIdx++
		}
		v.clampSelection()
		return nil
	case "k", "up":
		if v.focus == notificationFocusItems {
			v.itemIdx--
		} else {
			v.ruleIdx--
		}
		v.clampSelection()
		return nil
	case "enter":
		item, ok := v.selectedNotification()
		if !ok {
			return nil
		}
		if v.center != nil {
			v.center.MarkRead(item.MessageID)
		}
		if strings.TrimSpace(item.Target) == "" {
			return nil
		}
		return tea.Batch(openThreadCmd(item.Target, item.MessageID), pushViewCmd(ViewThread))
	case "x":
		item, ok := v.selectedNotification()
		if !ok {
			return nil
		}
		if v.center != nil && v.center.Dismiss(item.MessageID) {
			v.statusLine = "dismissed " + item.MessageID
			v.statusErr = false
		}
		v.clampSelection()
		return nil
	case "c":
		if v.center != nil {
			v.center.Clear()
			v.statusLine = "notifications cleared"
			v.statusErr = false
		}
		v.clampSelection()
		return nil
	case "n":
		v.startRuleEdit(-1)
		return nil
	case "e":
		if v.focus != notificationFocusRules {
			return nil
		}
		v.startRuleEdit(v.ruleIdx)
		return nil
	case "d":
		if v.focus != notificationFocusRules {
			return nil
		}
		if v.center != nil && v.center.DeleteRuleAt(v.ruleIdx) {
			v.statusLine = "rule deleted"
			v.statusErr = false
			v.clampSelection()
		}
		return nil
	case " ":
		if v.focus != notificationFocusRules {
			return nil
		}
		rule, ok := v.selectedRule()
		if !ok || v.center == nil {
			return nil
		}
		rule.Enabled = !rule.Enabled
		v.center.UpsertRule(v.ruleIdx, rule)
		v.statusLine = "rule toggled"
		v.statusErr = false
		return nil
	}
	return nil
}

func (v *notificationsView) handleEditKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.editActive = false
		v.statusLine = "edit canceled"
		v.statusErr = false
		return nil
	case tea.KeyEnter:
		rule, err := parseNotificationRuleSpec(v.editInput)
		if err != nil {
			v.statusLine = "invalid rule: " + err.Error()
			v.statusErr = true
			return nil
		}
		if v.center != nil {
			v.ruleIdx = v.center.UpsertRule(v.editIndex, rule)
		}
		v.editActive = false
		v.statusLine = "rule saved"
		v.statusErr = false
		v.clampSelection()
		return nil
	case tea.KeyBackspace:
		if len(v.editInput) > 0 {
			v.editInput = v.editInput[:len(v.editInput)-1]
		}
		return nil
	case tea.KeyCtrlU:
		v.editInput = ""
		return nil
	case tea.KeyCtrlT:
		if v.center == nil {
			return nil
		}
		rule, err := parseNotificationRuleSpec(v.editInput)
		if err != nil {
			v.statusLine = "invalid rule: " + err.Error()
			v.statusErr = true
			return nil
		}
		matches, scanned, err := v.center.PreviewMatches(rule, v.provider, 100)
		if err != nil {
			v.statusLine = "test failed: " + err.Error()
			v.statusErr = true
			return nil
		}
		v.statusLine = fmt.Sprintf("rule test: %d matches in last %d", matches, scanned)
		v.statusErr = false
		return nil
	case tea.KeyRunes:
		v.editInput += string(msg.Runes)
		return nil
	}
	return nil
}

func (v *notificationsView) startRuleEdit(index int) {
	v.editActive = true
	v.editIndex = index
	v.statusLine = ""
	v.statusErr = false
	rule := defaultEditableRule(v.self)
	if existing, ok := v.ruleAt(index); ok {
		rule = existing
	}
	v.editInput = formatNotificationRuleSpec(rule)
}

func (v *notificationsView) unreadCount() int {
	if v.center == nil {
		return 0
	}
	return v.center.UnreadCount()
}

func (v *notificationsView) notifications() []tuistate.Notification {
	if v.center == nil {
		return nil
	}
	return v.center.Notifications()
}

func (v *notificationsView) rules() []tuistate.NotificationRule {
	if v.center == nil {
		return nil
	}
	return v.center.Rules()
}

func (v *notificationsView) clampSelection() {
	notes := v.notifications()
	if len(notes) == 0 {
		v.itemIdx = 0
	} else {
		v.itemIdx = clampInt(v.itemIdx, 0, len(notes)-1)
	}
	rules := v.rules()
	if len(rules) == 0 {
		v.ruleIdx = 0
	} else {
		v.ruleIdx = clampInt(v.ruleIdx, 0, len(rules)-1)
	}
}

func (v *notificationsView) selectedNotification() (tuistate.Notification, bool) {
	notes := v.notifications()
	if len(notes) == 0 {
		return tuistate.Notification{}, false
	}
	idx := clampInt(v.itemIdx, 0, len(notes)-1)
	return notes[idx], true
}

func (v *notificationsView) selectedRule() (tuistate.NotificationRule, bool) {
	return v.ruleAt(v.ruleIdx)
}

func (v *notificationsView) ruleAt(index int) (tuistate.NotificationRule, bool) {
	rules := v.rules()
	if len(rules) == 0 {
		return tuistate.NotificationRule{}, false
	}
	idx := clampInt(index, 0, len(rules)-1)
	return rules[idx], true
}

func (v *notificationsView) renderNotificationLines(width, maxLines int, palette styles.Theme) []string {
	if maxLines <= 0 {
		return nil
	}
	lines := make([]string, 0, maxLines)
	notes := v.notifications()
	if len(notes) == 0 {
		return []string{"  (no notifications)"}
	}
	mapper := styles.NewAgentColorMapperWithPalette(palette.AgentPalette)
	start := 0
	if len(notes) > maxLines/2 {
		start = clampInt(v.itemIdx-(maxLines/4), 0, maxInt(0, len(notes)-1))
	}
	for i := start; i < len(notes) && len(lines) < maxLines; i++ {
		item := notes[i]
		cursor := "  "
		if v.focus == notificationFocusItems && i == v.itemIdx {
			cursor = "> "
		}
		mark := "o"
		if item.Unread {
			mark = "*"
		}
		ts := item.Timestamp
		if ts.IsZero() {
			ts = time.Now().UTC()
		}
		header := fmt.Sprintf("%s%s %s %s - %s -> %s", cursor, mark, ts.Format("15:04"), item.RuleLabel, mapper.Plain(item.From), strings.TrimSpace(item.Target))
		lines = append(lines, truncateVis(header, width))
		preview := "   " + strings.TrimSpace(item.Preview)
		lines = append(lines, truncateVis(preview, width))
	}
	return lines
}

func (v *notificationsView) renderRuleLines(width, maxLines int, palette styles.Theme) []string {
	if maxLines <= 0 {
		return nil
	}
	rules := v.rules()
	active := 0
	for _, rule := range rules {
		if rule.Enabled {
			active++
		}
	}
	lines := []string{fmt.Sprintf("Rules (%d active)", active)}
	if len(rules) == 0 {
		return append(lines, "  (none)")
	}
	for i, rule := range rules {
		if len(lines) >= maxLines {
			break
		}
		cursor := "  "
		if v.focus == notificationFocusRules && i == v.ruleIdx {
			cursor = "> "
		}
		enabled := "x"
		if rule.Enabled {
			enabled = "v"
		}
		line := fmt.Sprintf("%s%s \"%s\" - %s -> %s", cursor, enabled, rule.Name, renderRuleConditions(rule), renderRuleActions(rule))
		lines = append(lines, truncateVis(line, width))
	}
	if len(lines) < maxLines {
		lines = append(lines, lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("rule format: name=... topic=... from=... to=... priority=... tags=a,b text=... actions=badge,bell,flash,highlight enabled=true|false"))
	}
	return lines
}

func renderRuleConditions(rule tuistate.NotificationRule) string {
	parts := make([]string, 0, 6)
	if rule.Topic != "" {
		parts = append(parts, "topic:"+rule.Topic)
	}
	if rule.From != "" {
		parts = append(parts, "from:"+rule.From)
	}
	if rule.To != "" {
		parts = append(parts, "to:"+rule.To)
	}
	if p := normalizePriorityInput(rule.Priority); p != "" {
		parts = append(parts, "priority:"+p)
	}
	if len(rule.Tags) > 0 {
		parts = append(parts, "tags:"+strings.Join(rule.Tags, ","))
	}
	if rule.Text != "" {
		parts = append(parts, "text:"+rule.Text)
	}
	if len(parts) == 0 {
		return "(no conditions)"
	}
	return strings.Join(parts, " ")
}

func renderRuleActions(rule tuistate.NotificationRule) string {
	actions := make([]string, 0, 4)
	if rule.ActionHighlight {
		actions = append(actions, "highlight")
	}
	if rule.ActionBell {
		actions = append(actions, "bell")
	}
	if rule.ActionFlash {
		actions = append(actions, "flash")
	}
	if rule.ActionBadge {
		actions = append(actions, "badge")
	}
	if len(actions) == 0 {
		return "badge"
	}
	return strings.Join(actions, "+")
}

func defaultEditableRule(self string) tuistate.NotificationRule {
	rule := tuistate.NotificationRule{
		Name:        "new-rule",
		Enabled:     true,
		ActionBadge: true,
	}
	if strings.TrimSpace(self) != "" {
		rule.To = "@" + strings.TrimSpace(self)
	}
	return rule
}

func parseNotificationRuleSpec(input string) (tuistate.NotificationRule, error) {
	tokens := strings.Fields(strings.TrimSpace(input))
	if len(tokens) == 0 {
		return tuistate.NotificationRule{}, fmt.Errorf("empty input")
	}
	rule := tuistate.NotificationRule{Enabled: true, ActionBadge: true}
	actionsSet := false
	for _, tok := range tokens {
		key, val, ok := strings.Cut(tok, "=")
		if !ok {
			continue
		}
		key = strings.ToLower(strings.TrimSpace(key))
		val = strings.TrimSpace(val)
		switch key {
		case "name":
			rule.Name = val
		case "topic":
			rule.Topic = val
		case "from":
			rule.From = val
		case "to":
			rule.To = val
		case "priority":
			rule.Priority = normalizePriorityInput(val)
		case "tags":
			rule.Tags = splitCSVLike(val)
		case "text":
			rule.Text = val
		case "actions":
			actionsSet = true
			rule.ActionBadge = false
			rule.ActionBell = false
			rule.ActionFlash = false
			rule.ActionHighlight = false
			for _, action := range splitCSVLike(val) {
				switch strings.ToLower(strings.TrimSpace(action)) {
				case "highlight":
					rule.ActionHighlight = true
				case "bell":
					rule.ActionBell = true
				case "flash":
					rule.ActionFlash = true
				case "badge":
					rule.ActionBadge = true
				}
			}
		case "enabled":
			if val == "" {
				rule.Enabled = false
				continue
			}
			b, err := strconv.ParseBool(strings.ToLower(val))
			if err != nil {
				return tuistate.NotificationRule{}, fmt.Errorf("enabled must be true/false")
			}
			rule.Enabled = b
		}
	}
	if strings.TrimSpace(rule.Name) == "" {
		return tuistate.NotificationRule{}, fmt.Errorf("name required (name=<rule-name>)")
	}
	if actionsSet && !(rule.ActionBadge || rule.ActionBell || rule.ActionFlash || rule.ActionHighlight) {
		rule.ActionBadge = true
	}
	return rule, nil
}

func formatNotificationRuleSpec(rule tuistate.NotificationRule) string {
	parts := make([]string, 0, 10)
	parts = append(parts, "name="+strings.TrimSpace(rule.Name))
	if rule.Topic != "" {
		parts = append(parts, "topic="+rule.Topic)
	}
	if rule.From != "" {
		parts = append(parts, "from="+rule.From)
	}
	if rule.To != "" {
		parts = append(parts, "to="+rule.To)
	}
	if p := normalizePriorityInput(rule.Priority); p != "" {
		parts = append(parts, "priority="+p)
	}
	if len(rule.Tags) > 0 {
		parts = append(parts, "tags="+strings.Join(rule.Tags, ","))
	}
	if rule.Text != "" {
		parts = append(parts, "text="+rule.Text)
	}
	actions := make([]string, 0, 4)
	if rule.ActionBadge {
		actions = append(actions, "badge")
	}
	if rule.ActionBell {
		actions = append(actions, "bell")
	}
	if rule.ActionFlash {
		actions = append(actions, "flash")
	}
	if rule.ActionHighlight {
		actions = append(actions, "highlight")
	}
	if len(actions) == 0 {
		actions = append(actions, "badge")
	}
	parts = append(parts, "actions="+strings.Join(actions, ","))
	parts = append(parts, fmt.Sprintf("enabled=%t", rule.Enabled))
	return strings.Join(parts, " ")
}

func joinClampedLines(lines []string, maxLines int) string {
	if maxLines <= 0 {
		return ""
	}
	if len(lines) > maxLines {
		lines = lines[len(lines)-maxLines:]
	}
	return strings.Join(lines, "\n")
}
