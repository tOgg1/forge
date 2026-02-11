package fmailtui

import (
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
)

const (
	notificationMemoryLimit  = 200
	notificationPersistLimit = 50
)

type notificationActions struct {
	Highlight bool
	Bell      bool
	Flash     bool
	Badge     bool
}

type compiledNotificationRule struct {
	rule   tuistate.NotificationRule
	textRE *regexp.Regexp
}

type notificationCenter struct {
	self   string
	state  *tuistate.Manager
	rules  []tuistate.NotificationRule
	index  []compiledNotificationRule
	items  []tuistate.Notification // newest first; capped to notificationMemoryLimit
	seen   map[string]struct{}
	high   map[string]struct{}
	loaded bool
}

func newNotificationCenter(self string, st *tuistate.Manager) *notificationCenter {
	c := &notificationCenter{
		self:  strings.TrimSpace(self),
		state: st,
		seen:  make(map[string]struct{}),
		high:  make(map[string]struct{}),
	}
	c.load()
	return c
}

func (c *notificationCenter) load() {
	if c == nil || c.loaded {
		return
	}
	c.loaded = true
	rules := c.defaultRules()
	items := []tuistate.Notification(nil)
	if c.state != nil {
		snap := c.state.Snapshot()
		if len(snap.NotifyRules) > 0 {
			rules = append([]tuistate.NotificationRule(nil), snap.NotifyRules...)
		}
		if len(snap.Notifications) > 0 {
			items = append([]tuistate.Notification(nil), snap.Notifications...)
		}
	}
	c.setRulesInternal(rules)
	c.setItemsInternal(items)
	if c.state != nil && len(c.state.NotificationRules()) == 0 {
		c.persistRules()
	}
}

func (c *notificationCenter) defaultRules() []tuistate.NotificationRule {
	rules := []tuistate.NotificationRule{
		{
			Name:            "high-priority",
			Priority:        fmail.PriorityHigh,
			ActionBell:      true,
			ActionHighlight: true,
			ActionBadge:     true,
			Enabled:         true,
		},
	}
	if c == nil {
		return rules
	}
	if self := strings.TrimSpace(c.self); self != "" {
		rules = append(rules, tuistate.NotificationRule{
			Name:            "direct-messages",
			To:              "@" + self,
			ActionBadge:     true,
			ActionHighlight: true,
			Enabled:         true,
		})
	}
	return rules
}

func (c *notificationCenter) Rules() []tuistate.NotificationRule {
	if c == nil {
		return nil
	}
	return append([]tuistate.NotificationRule(nil), c.rules...)
}

func (c *notificationCenter) Notifications() []tuistate.Notification {
	if c == nil {
		return nil
	}
	return append([]tuistate.Notification(nil), c.items...)
}

func (c *notificationCenter) UnreadCount() int {
	if c == nil {
		return 0
	}
	n := 0
	for i := range c.items {
		if c.items[i].Unread && c.items[i].Badge {
			n++
		}
	}
	return n
}

func (c *notificationCenter) IsHighlighted(messageID string) bool {
	if c == nil {
		return false
	}
	_, ok := c.high[strings.TrimSpace(messageID)]
	return ok
}

func (c *notificationCenter) ProcessMessage(msg fmail.Message) (notificationActions, bool) {
	if c == nil {
		return notificationActions{}, false
	}
	id := strings.TrimSpace(msg.ID)
	if id == "" {
		return notificationActions{}, false
	}
	if _, dup := c.seen[id]; dup {
		return notificationActions{}, false
	}

	matches := make([]compiledNotificationRule, 0, 2)
	for _, rule := range c.index {
		if !rule.rule.Enabled {
			continue
		}
		if !ruleMatchesMessage(rule, msg) {
			continue
		}
		matches = append(matches, rule)
	}
	if len(matches) == 0 {
		return notificationActions{}, false
	}

	actions := notificationActions{}
	for _, match := range matches {
		actions.Highlight = actions.Highlight || match.rule.ActionHighlight
		actions.Bell = actions.Bell || match.rule.ActionBell
		actions.Flash = actions.Flash || match.rule.ActionFlash
		actions.Badge = actions.Badge || match.rule.ActionBadge
	}

	rule := matches[0].rule
	now := time.Now().UTC()
	ts := msg.Time.UTC()
	if ts.IsZero() {
		ts = now
	}
	item := tuistate.Notification{
		MessageID: id,
		Target:    strings.TrimSpace(msg.To),
		From:      strings.TrimSpace(msg.From),
		Priority:  normalizePriorityInput(msg.Priority),
		RuleName:  strings.TrimSpace(rule.Name),
		RuleLabel: notificationRuleLabel(rule),
		Preview:   notificationPreview(msg),
		Timestamp: ts,
		Unread:    true,
		Badge:     actions.Badge,
		Highlight: actions.Highlight,
	}
	c.items = append([]tuistate.Notification{item}, c.items...)
	if len(c.items) > notificationMemoryLimit {
		c.items = c.items[:notificationMemoryLimit]
	}
	c.seen[id] = struct{}{}
	if actions.Highlight {
		c.high[id] = struct{}{}
	}
	c.persistNotifications()
	return actions, true
}

func (c *notificationCenter) MarkRead(messageID string) bool {
	if c == nil {
		return false
	}
	id := strings.TrimSpace(messageID)
	if id == "" {
		return false
	}
	changed := false
	for i := range c.items {
		if strings.TrimSpace(c.items[i].MessageID) != id {
			continue
		}
		if c.items[i].Unread {
			c.items[i].Unread = false
			changed = true
		}
		break
	}
	if changed {
		c.persistNotifications()
	}
	return changed
}

func (c *notificationCenter) Dismiss(messageID string) bool {
	if c == nil {
		return false
	}
	id := strings.TrimSpace(messageID)
	if id == "" {
		return false
	}
	idx := -1
	for i := range c.items {
		if strings.TrimSpace(c.items[i].MessageID) == id {
			idx = i
			break
		}
	}
	if idx < 0 {
		return false
	}
	c.items = append(c.items[:idx], c.items[idx+1:]...)
	delete(c.seen, id)
	delete(c.high, id)
	c.persistNotifications()
	return true
}

func (c *notificationCenter) Clear() {
	if c == nil {
		return
	}
	c.items = nil
	c.seen = make(map[string]struct{})
	c.high = make(map[string]struct{})
	c.persistNotifications()
}

func (c *notificationCenter) SetRules(rules []tuistate.NotificationRule) {
	if c == nil {
		return
	}
	c.setRulesInternal(rules)
	c.persistRules()
}

func (c *notificationCenter) DeleteRuleAt(index int) bool {
	if c == nil || index < 0 || index >= len(c.rules) {
		return false
	}
	c.rules = append(c.rules[:index], c.rules[index+1:]...)
	c.rebuildRuleIndex()
	c.persistRules()
	return true
}

func (c *notificationCenter) UpsertRule(index int, rule tuistate.NotificationRule) int {
	if c == nil {
		return -1
	}
	rule.Name = strings.TrimSpace(rule.Name)
	if rule.Name == "" {
		return -1
	}
	if index >= 0 && index < len(c.rules) {
		c.rules[index] = rule
		c.rebuildRuleIndex()
		c.persistRules()
		return index
	}
	for i := range c.rules {
		if strings.EqualFold(strings.TrimSpace(c.rules[i].Name), rule.Name) {
			c.rules[i] = rule
			c.rebuildRuleIndex()
			c.persistRules()
			return i
		}
	}
	c.rules = append(c.rules, rule)
	c.rebuildRuleIndex()
	c.persistRules()
	return len(c.rules) - 1
}

func (c *notificationCenter) PreviewMatches(rule tuistate.NotificationRule, provider data.MessageProvider, limit int) (matches int, scanned int, err error) {
	if provider == nil {
		return 0, 0, nil
	}
	if limit <= 0 {
		limit = 100
	}
	compiled := compileNotificationRule(rule)
	if strings.TrimSpace(compiled.rule.Name) == "" {
		compiled.rule.Name = "preview"
	}
	results, err := provider.Search(data.SearchQuery{})
	if err != nil {
		return 0, 0, err
	}
	if len(results) > limit {
		results = results[len(results)-limit:]
	}
	scanned = len(results)
	for _, r := range results {
		if ruleMatchesMessage(compiled, r.Message) {
			matches++
		}
	}
	return matches, scanned, nil
}

func (c *notificationCenter) setRulesInternal(rules []tuistate.NotificationRule) {
	if len(rules) == 0 {
		rules = c.defaultRules()
	}
	next := make([]tuistate.NotificationRule, 0, len(rules))
	for _, rule := range rules {
		if strings.TrimSpace(rule.Name) == "" {
			continue
		}
		next = append(next, rule)
	}
	if len(next) == 0 {
		next = c.defaultRules()
	}
	c.rules = next
	c.rebuildRuleIndex()
}

func (c *notificationCenter) rebuildRuleIndex() {
	if c == nil {
		return
	}
	compiled := make([]compiledNotificationRule, 0, len(c.rules))
	for _, rule := range c.rules {
		if strings.TrimSpace(rule.Name) == "" {
			continue
		}
		compiled = append(compiled, compileNotificationRule(rule))
	}
	c.index = compiled
}

func (c *notificationCenter) setItemsInternal(items []tuistate.Notification) {
	if c == nil {
		return
	}
	if len(items) == 0 {
		c.items = nil
		c.seen = make(map[string]struct{})
		c.high = make(map[string]struct{})
		return
	}
	sort.SliceStable(items, func(i, j int) bool {
		return items[i].Timestamp.After(items[j].Timestamp)
	})
	if len(items) > notificationMemoryLimit {
		items = items[:notificationMemoryLimit]
	}
	c.items = append([]tuistate.Notification(nil), items...)
	c.seen = make(map[string]struct{}, len(c.items))
	c.high = make(map[string]struct{}, len(c.items))
	for i := range c.items {
		id := strings.TrimSpace(c.items[i].MessageID)
		if id == "" {
			continue
		}
		c.seen[id] = struct{}{}
		if c.items[i].Highlight {
			c.high[id] = struct{}{}
		}
	}
}

func (c *notificationCenter) persistRules() {
	if c == nil || c.state == nil {
		return
	}
	c.state.SetNotificationRules(c.rules)
	c.state.SaveSoon()
}

func (c *notificationCenter) persistNotifications() {
	if c == nil || c.state == nil {
		return
	}
	persist := c.items
	if len(persist) > notificationPersistLimit {
		persist = persist[:notificationPersistLimit]
	}
	c.state.SetNotifications(persist)
	c.state.SaveSoon()
}

func compileNotificationRule(rule tuistate.NotificationRule) compiledNotificationRule {
	rule.Name = strings.TrimSpace(rule.Name)
	rule.Topic = strings.TrimSpace(rule.Topic)
	rule.From = strings.TrimSpace(rule.From)
	rule.To = strings.TrimSpace(rule.To)
	rule.Priority = normalizePriorityInput(rule.Priority)
	rule.Tags = normalizeRuleTags(rule.Tags)
	rule.Text = strings.TrimSpace(rule.Text)
	if !(rule.ActionHighlight || rule.ActionBell || rule.ActionFlash || rule.ActionBadge) {
		rule.ActionBadge = true
	}
	re := (*regexp.Regexp)(nil)
	if rule.Text != "" {
		re, _ = regexp.Compile("(?i)" + rule.Text)
	}
	return compiledNotificationRule{rule: rule, textRE: re}
}

func ruleMatchesMessage(compiled compiledNotificationRule, msg fmail.Message) bool {
	rule := compiled.rule
	if !rule.Enabled {
		return false
	}
	topic := strings.TrimSpace(msg.To)
	if rule.Topic != "" {
		if strings.HasPrefix(topic, "@") {
			return false
		}
		if !globMatchRule(rule.Topic, topic) {
			return false
		}
	}
	if rule.From != "" && !globMatchRule(rule.From, msg.From) {
		return false
	}
	if rule.To != "" && !globMatchRule(rule.To, msg.To) {
		return false
	}
	if rule.Priority != "" && notificationPriorityRank(msg.Priority) < notificationPriorityRank(rule.Priority) {
		return false
	}
	if len(rule.Tags) > 0 {
		if !notificationTagMatchAny(msg.Tags, rule.Tags) {
			return false
		}
	}
	if rule.Text != "" {
		if compiled.textRE == nil {
			return false
		}
		if !compiled.textRE.MatchString(messageBodyString(msg.Body)) {
			return false
		}
	}
	return true
}

func globMatchRule(pattern, value string) bool {
	pattern = strings.ToLower(strings.TrimSpace(pattern))
	value = strings.ToLower(strings.TrimSpace(value))
	if pattern == "" {
		return true
	}
	if value == "" {
		return false
	}
	ok, err := filepath.Match(pattern, value)
	if err != nil {
		return pattern == value
	}
	return ok
}

func notificationTagMatchAny(actual []string, wanted []string) bool {
	if len(wanted) == 0 {
		return true
	}
	set := make(map[string]struct{}, len(actual))
	for _, t := range actual {
		t = strings.TrimSpace(strings.ToLower(t))
		if t == "" {
			continue
		}
		set[t] = struct{}{}
	}
	for _, want := range wanted {
		want = strings.TrimSpace(strings.ToLower(want))
		if want == "" {
			continue
		}
		if _, ok := set[want]; ok {
			return true
		}
	}
	return false
}

func normalizeRuleTags(tags []string) []string {
	if len(tags) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(tags))
	out := make([]string, 0, len(tags))
	for _, t := range tags {
		t = strings.TrimSpace(strings.ToLower(t))
		if t == "" {
			continue
		}
		if _, ok := seen[t]; ok {
			continue
		}
		seen[t] = struct{}{}
		out = append(out, t)
	}
	return out
}

func notificationPriorityRank(priority string) int {
	switch normalizePriorityInput(priority) {
	case fmail.PriorityHigh:
		return 3
	case fmail.PriorityNormal, "":
		return 2
	case fmail.PriorityLow:
		return 1
	default:
		return 0
	}
}

func notificationRuleLabel(rule tuistate.NotificationRule) string {
	if strings.EqualFold(strings.TrimSpace(rule.Name), "high-priority") {
		return "HIGH"
	}
	name := strings.TrimSpace(rule.Name)
	if name == "" {
		return "RULE"
	}
	return "RULE \"" + name + "\""
}

func notificationPreview(msg fmail.Message) string {
	preview := strings.TrimSpace(firstLine(msg.Body))
	if preview == "" {
		preview = "(no body)"
	}
	if len(preview) > 72 {
		preview = preview[:69] + "..."
	}
	return preview
}
