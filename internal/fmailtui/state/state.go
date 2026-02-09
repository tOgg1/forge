package state

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/tOgg1/forge/internal/fmailtui/data"
)

const (
	CurrentVersion = 1

	defaultDebounce  = 1 * time.Second
	maxBookmarks     = 500
	bookmarkMaxAge   = 30 * 24 * time.Hour
	maxNotifications = 50
)

type TUIState struct {
	Version       int                     `json:"version"`
	ReadMarkers   map[string]string       `json:"read_markers,omitempty"`   // topic/dm -> last-read message ID
	Bookmarks     []Bookmark              `json:"bookmarks,omitempty"`      // saved message references
	Annotations   map[string]string       `json:"annotations,omitempty"`    // message ID -> annotation text
	Drafts        map[string]ComposeDraft `json:"drafts,omitempty"`         // target -> draft payload
	Groups        map[string][]string     `json:"groups,omitempty"`         // ad-hoc compose groups
	StarredTopics []string                `json:"starred_topics,omitempty"` // pinned topic names
	SavedSearches []SavedSearch           `json:"saved_searches,omitempty"` // named search presets
	Preferences   Preferences             `json:"preferences,omitempty"`    // UI preferences
	NotifyRules   []NotificationRule      `json:"notify_rules,omitempty"`   // notification configuration
	Notifications []Notification          `json:"notifications,omitempty"`  // persisted recent notifications
	LastView      string                  `json:"last_view,omitempty"`      // last active view (for session restore)
	LastTopic     string                  `json:"last_topic,omitempty"`     // last viewed topic
}

type ComposeDraft struct {
	Target    string    `json:"target"`
	To        string    `json:"to"`
	Priority  string    `json:"priority,omitempty"`
	Tags      string    `json:"tags,omitempty"`
	ReplyTo   string    `json:"reply_to,omitempty"`
	Body      string    `json:"body,omitempty"`
	UpdatedAt time.Time `json:"updated_at,omitempty"`
}

type Bookmark struct {
	MessageID string    `json:"message_id"`
	Topic     string    `json:"topic"`                // topic name or "@agent" for DMs
	Note      string    `json:"note,omitempty"`       // optional user note
	CreatedAt time.Time `json:"created_at,omitempty"` // set on creation
}

type SavedSearch struct {
	Name  string           `json:"name"`
	Query data.SearchQuery `json:"query"`
}

type Preferences struct {
	DefaultLayout        string   `json:"default_layout,omitempty"`         // "single", "split", "dashboard", "zen"
	Theme                string   `json:"theme,omitempty"`                  // "default", "high-contrast"
	LayoutSplitRatio     float64  `json:"layout_split_ratio,omitempty"`     // 0.2..0.8
	LayoutSplitCollapsed bool     `json:"layout_split_collapsed,omitempty"` // hide left split pane
	LayoutFocus          int      `json:"layout_focus,omitempty"`           // focused pane index
	LayoutExpanded       bool     `json:"layout_expanded,omitempty"`        // focused-pane-only mode
	DashboardGrid        string   `json:"dashboard_grid,omitempty"`         // "2x2","2x1","1x2","1x3","3x1"
	DashboardViews       []string `json:"dashboard_views,omitempty"`        // up to 4 view IDs
	LiveTailAuto         bool     `json:"live_tail_auto,omitempty"`
	RelativeTime         bool     `json:"relative_time,omitempty"`
	SoundAlerts          bool     `json:"sound_alerts,omitempty"`
	// HighlightPatterns are live-tail keyword highlight regexes.
	HighlightPatterns []string `json:"highlight_patterns,omitempty"`
}

type NotificationRule struct {
	Name            string   `json:"name"`
	Topic           string   `json:"topic,omitempty"`    // glob
	From            string   `json:"from,omitempty"`     // glob
	To              string   `json:"to,omitempty"`       // glob
	Priority        string   `json:"priority,omitempty"` // min priority
	Tags            []string `json:"tags,omitempty"`     // any tag match
	Text            string   `json:"text,omitempty"`     // regex
	ActionHighlight bool     `json:"action_highlight,omitempty"`
	ActionBell      bool     `json:"action_bell,omitempty"`
	ActionFlash     bool     `json:"action_flash,omitempty"`
	ActionBadge     bool     `json:"action_badge,omitempty"`
	Enabled         bool     `json:"enabled"`
}

type Notification struct {
	MessageID  string    `json:"message_id"`
	Target     string    `json:"target,omitempty"` // topic or @agent
	From       string    `json:"from,omitempty"`
	Priority   string    `json:"priority,omitempty"`
	RuleName   string    `json:"rule_name,omitempty"`
	RuleLabel  string    `json:"rule_label,omitempty"`
	Preview    string    `json:"preview,omitempty"`
	Timestamp  time.Time `json:"timestamp,omitempty"`
	Unread     bool      `json:"unread,omitempty"`
	Badge      bool      `json:"badge,omitempty"`
	Highlight  bool      `json:"highlight,omitempty"`
	OccurredAt time.Time `json:"occurred_at,omitempty"` // backwards-compatible fallback timestamp
}

type Manager struct {
	path     string
	lockPath string

	mu        sync.Mutex
	state     TUIState
	dirty     bool
	timer     *time.Timer
	debounce  time.Duration
	lastWrite time.Time
}

func New(path string) *Manager {
	path = strings.TrimSpace(path)
	return &Manager{
		path:     path,
		lockPath: path + ".lock",
		state: TUIState{
			Version:     CurrentVersion,
			ReadMarkers: make(map[string]string),
			Annotations: make(map[string]string),
			Drafts:      make(map[string]ComposeDraft),
			Groups:      make(map[string][]string),
		},
		debounce: defaultDebounce,
	}
}

func (m *Manager) Path() string { return m.path }

func (m *Manager) Load() error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.path == "" {
		return nil
	}

	loaded, err := m.loadLocked()
	if err != nil {
		return err
	}
	m.state = loaded
	m.dirty = false
	return nil
}

func (m *Manager) Snapshot() TUIState {
	m.mu.Lock()
	defer m.mu.Unlock()
	return cloneState(m.state)
}

func (m *Manager) Preferences() Preferences {
	m.mu.Lock()
	defer m.mu.Unlock()
	return clonePreferences(m.state.Preferences)
}

func (m *Manager) UpdatePreferences(update func(*Preferences)) {
	if update == nil {
		return
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	next := clonePreferences(m.state.Preferences)
	update(&next)
	m.state.Preferences = normalizePreferences(next)
	m.markDirtyLocked()
}

func (m *Manager) Theme() string {
	m.mu.Lock()
	defer m.mu.Unlock()
	return strings.TrimSpace(m.state.Preferences.Theme)
}

func (m *Manager) SetTheme(theme string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	theme = strings.TrimSpace(theme)
	if theme == "" {
		return
	}
	if strings.EqualFold(m.state.Preferences.Theme, theme) {
		return
	}
	m.state.Preferences.Theme = theme
	m.markDirtyLocked()
}

func (m *Manager) HighlightPatterns() []string {
	m.mu.Lock()
	defer m.mu.Unlock()
	if len(m.state.Preferences.HighlightPatterns) == 0 {
		return nil
	}
	return append([]string(nil), m.state.Preferences.HighlightPatterns...)
}

func (m *Manager) SetHighlightPatterns(patterns []string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	normalized := normalizeStringList(patterns)
	if len(normalized) == 0 {
		m.state.Preferences.HighlightPatterns = nil
		m.markDirtyLocked()
		return
	}
	m.state.Preferences.HighlightPatterns = normalized
	m.markDirtyLocked()
}

func (m *Manager) ReadMarker(target string) string {
	m.mu.Lock()
	defer m.mu.Unlock()
	target = strings.TrimSpace(target)
	if target == "" {
		return ""
	}
	return strings.TrimSpace(m.state.ReadMarkers[target])
}

func (m *Manager) SetReadMarker(target, marker string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	target = strings.TrimSpace(target)
	marker = strings.TrimSpace(marker)
	if target == "" || marker == "" {
		return
	}
	if m.state.ReadMarkers == nil {
		m.state.ReadMarkers = make(map[string]string)
	}
	if prev := strings.TrimSpace(m.state.ReadMarkers[target]); prev != "" && prev >= marker {
		return
	}
	m.state.ReadMarkers[target] = marker
	m.markDirtyLocked()
}

func (m *Manager) SetReadMarkers(markers map[string]string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.state.ReadMarkers == nil {
		m.state.ReadMarkers = make(map[string]string)
	}
	for k := range m.state.ReadMarkers {
		delete(m.state.ReadMarkers, k)
	}
	for target, marker := range markers {
		target = strings.TrimSpace(target)
		marker = strings.TrimSpace(marker)
		if target == "" || marker == "" {
			continue
		}
		m.state.ReadMarkers[target] = marker
	}
	m.markDirtyLocked()
}

func (m *Manager) Bookmarks() []Bookmark {
	m.mu.Lock()
	defer m.mu.Unlock()
	if len(m.state.Bookmarks) == 0 {
		return nil
	}
	return append([]Bookmark(nil), m.state.Bookmarks...)
}

func (m *Manager) IsBookmarked(messageID string) bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	messageID = strings.TrimSpace(messageID)
	if messageID == "" || len(m.state.Bookmarks) == 0 {
		return false
	}
	for _, bm := range m.state.Bookmarks {
		if strings.TrimSpace(bm.MessageID) == messageID {
			return true
		}
	}
	return false
}

func (m *Manager) BookmarkNote(messageID string) string {
	m.mu.Lock()
	defer m.mu.Unlock()
	messageID = strings.TrimSpace(messageID)
	if messageID == "" || len(m.state.Bookmarks) == 0 {
		return ""
	}
	for _, bm := range m.state.Bookmarks {
		if strings.TrimSpace(bm.MessageID) == messageID {
			return strings.TrimSpace(bm.Note)
		}
	}
	return ""
}

// ToggleBookmark toggles a bookmark for a message.
// Returns true when the bookmark was added, false when removed/no-op.
func (m *Manager) ToggleBookmark(messageID, topic string) bool {
	m.mu.Lock()
	defer m.mu.Unlock()

	messageID = strings.TrimSpace(messageID)
	topic = strings.TrimSpace(topic)
	if messageID == "" || topic == "" {
		return false
	}

	filtered := make([]Bookmark, 0, len(m.state.Bookmarks))
	removed := false
	for _, bm := range m.state.Bookmarks {
		if strings.TrimSpace(bm.MessageID) == messageID {
			removed = true
			continue
		}
		filtered = append(filtered, bm)
	}
	if removed {
		m.state.Bookmarks = filtered
		m.markDirtyLocked()
		return false
	}

	next := Bookmark{
		MessageID: messageID,
		Topic:     topic,
		CreatedAt: time.Now().UTC(),
	}
	m.state.Bookmarks = append([]Bookmark{next}, filtered...)
	m.markDirtyLocked()
	return true
}

// UpsertBookmark adds a bookmark or updates its note (and topic if needed).
func (m *Manager) UpsertBookmark(messageID, topic, note string) {
	m.mu.Lock()
	defer m.mu.Unlock()

	messageID = strings.TrimSpace(messageID)
	topic = strings.TrimSpace(topic)
	note = strings.TrimSpace(note)
	if messageID == "" || topic == "" {
		return
	}

	next := make([]Bookmark, 0, len(m.state.Bookmarks)+1)
	updated := false
	for _, bm := range m.state.Bookmarks {
		if strings.TrimSpace(bm.MessageID) != messageID {
			next = append(next, bm)
			continue
		}
		bm.MessageID = messageID
		bm.Topic = topic
		bm.Note = note
		if bm.CreatedAt.IsZero() {
			bm.CreatedAt = time.Now().UTC()
		}
		next = append(next, bm)
		updated = true
	}
	if !updated {
		next = append([]Bookmark{{
			MessageID: messageID,
			Topic:     topic,
			Note:      note,
			CreatedAt: time.Now().UTC(),
		}}, next...)
	}

	m.state.Bookmarks = next
	m.markDirtyLocked()
}

// DeleteBookmark removes a bookmark by message ID.
func (m *Manager) DeleteBookmark(messageID string) bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	messageID = strings.TrimSpace(messageID)
	if messageID == "" || len(m.state.Bookmarks) == 0 {
		return false
	}
	next := make([]Bookmark, 0, len(m.state.Bookmarks))
	removed := false
	for _, bm := range m.state.Bookmarks {
		if strings.TrimSpace(bm.MessageID) == messageID {
			removed = true
			continue
		}
		next = append(next, bm)
	}
	if !removed {
		return false
	}
	m.state.Bookmarks = next
	m.markDirtyLocked()
	return true
}

func (m *Manager) Annotation(messageID string) string {
	m.mu.Lock()
	defer m.mu.Unlock()
	messageID = strings.TrimSpace(messageID)
	if messageID == "" || len(m.state.Annotations) == 0 {
		return ""
	}
	return strings.TrimSpace(m.state.Annotations[messageID])
}

// SetAnnotation adds or updates an annotation. Empty note deletes.
func (m *Manager) SetAnnotation(messageID, note string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	messageID = strings.TrimSpace(messageID)
	note = strings.TrimSpace(note)
	if messageID == "" {
		return
	}
	if m.state.Annotations == nil {
		m.state.Annotations = make(map[string]string)
	}
	if note == "" {
		if _, ok := m.state.Annotations[messageID]; !ok {
			return
		}
		delete(m.state.Annotations, messageID)
		m.markDirtyLocked()
		return
	}
	if strings.TrimSpace(m.state.Annotations[messageID]) == note {
		return
	}
	m.state.Annotations[messageID] = note
	m.markDirtyLocked()
}

func (m *Manager) Draft(target string) (ComposeDraft, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	target = strings.TrimSpace(target)
	if target == "" || len(m.state.Drafts) == 0 {
		return ComposeDraft{}, false
	}
	draft, ok := m.state.Drafts[target]
	if !ok {
		return ComposeDraft{}, false
	}
	return draft, true
}

func (m *Manager) SetDraft(draft ComposeDraft) {
	m.mu.Lock()
	defer m.mu.Unlock()
	target := strings.TrimSpace(draft.Target)
	if target == "" {
		return
	}
	if m.state.Drafts == nil {
		m.state.Drafts = make(map[string]ComposeDraft)
	}
	draft.Target = target
	if draft.UpdatedAt.IsZero() {
		draft.UpdatedAt = time.Now().UTC()
	}
	m.state.Drafts[target] = draft
	m.markDirtyLocked()
}

func (m *Manager) DeleteDraft(target string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	target = strings.TrimSpace(target)
	if target == "" || len(m.state.Drafts) == 0 {
		return
	}
	if _, ok := m.state.Drafts[target]; !ok {
		return
	}
	delete(m.state.Drafts, target)
	m.markDirtyLocked()
}

func (m *Manager) Groups() map[string][]string {
	m.mu.Lock()
	defer m.mu.Unlock()
	return cloneGroups(m.state.Groups)
}

func (m *Manager) SetGroup(name string, members []string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	name = strings.TrimSpace(name)
	if name == "" {
		return
	}
	normalized := normalizeGroupMembers(members)
	if m.state.Groups == nil {
		m.state.Groups = make(map[string][]string)
	}
	if len(normalized) == 0 {
		delete(m.state.Groups, name)
		m.markDirtyLocked()
		return
	}
	m.state.Groups[name] = normalized
	m.markDirtyLocked()
}

func (m *Manager) StarredTopics() []string {
	m.mu.Lock()
	defer m.mu.Unlock()
	return append([]string(nil), m.state.StarredTopics...)
}

func (m *Manager) SavedSearches() []SavedSearch {
	m.mu.Lock()
	defer m.mu.Unlock()
	return append([]SavedSearch(nil), m.state.SavedSearches...)
}

func (m *Manager) UpsertSavedSearch(name string, query data.SearchQuery) {
	m.mu.Lock()
	defer m.mu.Unlock()
	name = strings.TrimSpace(name)
	if name == "" {
		return
	}

	next := make([]SavedSearch, 0, len(m.state.SavedSearches)+1)
	for _, ss := range m.state.SavedSearches {
		if strings.EqualFold(strings.TrimSpace(ss.Name), name) {
			continue
		}
		if strings.TrimSpace(ss.Name) == "" {
			continue
		}
		next = append(next, ss)
	}
	next = append(next, SavedSearch{Name: name, Query: query})
	sort.SliceStable(next, func(i, j int) bool {
		return strings.ToLower(next[i].Name) < strings.ToLower(next[j].Name)
	})
	m.state.SavedSearches = next
	m.markDirtyLocked()
}

func (m *Manager) DeleteSavedSearch(name string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	name = strings.TrimSpace(name)
	if name == "" || len(m.state.SavedSearches) == 0 {
		return
	}
	next := make([]SavedSearch, 0, len(m.state.SavedSearches))
	for _, ss := range m.state.SavedSearches {
		if strings.EqualFold(strings.TrimSpace(ss.Name), name) {
			continue
		}
		next = append(next, ss)
	}
	if len(next) == len(m.state.SavedSearches) {
		return
	}
	m.state.SavedSearches = next
	m.markDirtyLocked()
}

func (m *Manager) SetStarredTopics(topics []string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	normalized := make([]string, 0, len(topics))
	seen := make(map[string]struct{}, len(topics))
	for _, t := range topics {
		t = strings.TrimSpace(t)
		if t == "" {
			continue
		}
		if _, ok := seen[t]; ok {
			continue
		}
		seen[t] = struct{}{}
		normalized = append(normalized, t)
	}
	sort.Strings(normalized)
	m.state.StarredTopics = normalized
	m.markDirtyLocked()
}

func (m *Manager) NotificationRules() []NotificationRule {
	m.mu.Lock()
	defer m.mu.Unlock()
	if len(m.state.NotifyRules) == 0 {
		return nil
	}
	return append([]NotificationRule(nil), m.state.NotifyRules...)
}

func (m *Manager) SetNotificationRules(rules []NotificationRule) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if len(rules) == 0 {
		m.state.NotifyRules = nil
		m.markDirtyLocked()
		return
	}
	next := make([]NotificationRule, 0, len(rules))
	for _, rule := range rules {
		normalized, ok := normalizeNotificationRule(rule)
		if !ok {
			continue
		}
		next = append(next, normalized)
	}
	m.state.NotifyRules = next
	m.markDirtyLocked()
}

func (m *Manager) Notifications() []Notification {
	m.mu.Lock()
	defer m.mu.Unlock()
	if len(m.state.Notifications) == 0 {
		return nil
	}
	return append([]Notification(nil), m.state.Notifications...)
}

func (m *Manager) SetNotifications(notifications []Notification) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if len(notifications) == 0 {
		m.state.Notifications = nil
		m.markDirtyLocked()
		return
	}
	next := make([]Notification, 0, len(notifications))
	for _, item := range notifications {
		normalized, ok := normalizeNotification(item)
		if !ok {
			continue
		}
		next = append(next, normalized)
		if len(next) == maxNotifications {
			break
		}
	}
	m.state.Notifications = next
	m.markDirtyLocked()
}

func (m *Manager) SaveSoon() {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.markDirtyLocked()
}

func (m *Manager) Close() error {
	m.mu.Lock()
	if m.timer != nil {
		m.timer.Stop()
		m.timer = nil
	}
	needsSave := m.dirty
	m.mu.Unlock()
	if !needsSave {
		return nil
	}
	return m.SaveNow()
}

func (m *Manager) SaveNow() error {
	m.mu.Lock()
	if m.path == "" {
		m.mu.Unlock()
		return nil
	}
	state := cloneState(m.state)
	m.dirty = false
	m.mu.Unlock()

	state.Version = CurrentVersion
	state = normalizeState(state, time.Now().UTC())

	if err := withFileLock(m.lockPath, func() error {
		return writeAtomicJSON(m.path, state)
	}); err != nil {
		m.mu.Lock()
		m.dirty = true
		m.mu.Unlock()
		return err
	}

	m.mu.Lock()
	m.lastWrite = time.Now().UTC()
	m.mu.Unlock()
	return nil
}

func (m *Manager) markDirtyLocked() {
	m.dirty = true
	if m.path == "" {
		return
	}
	if m.timer == nil {
		m.timer = time.AfterFunc(m.debounce, func() {
			_ = m.SaveNow()
		})
		return
	}
	_ = m.timer.Reset(m.debounce)
}

func (m *Manager) loadLocked() (TUIState, error) {
	var out TUIState
	if err := withFileLock(m.lockPath, func() error {
		payload, err := os.ReadFile(m.path)
		if err != nil {
			if errors.Is(err, os.ErrNotExist) {
				out = TUIState{Version: CurrentVersion}
				return nil
			}
			return err
		}
		if len(payload) == 0 {
			out = TUIState{Version: CurrentVersion}
			return nil
		}

		// First attempt: current schema.
		if err := json.Unmarshal(payload, &out); err == nil && out.Version > 0 {
			return nil
		}

		// Legacy schema: no version, only read_markers/starred_topics.
		var legacy struct {
			ReadMarkers   map[string]string `json:"read_markers,omitempty"`
			StarredTopics []string          `json:"starred_topics,omitempty"`
		}
		if err := json.Unmarshal(payload, &legacy); err != nil {
			return err
		}
		out = TUIState{
			Version:       CurrentVersion,
			ReadMarkers:   legacy.ReadMarkers,
			StarredTopics: legacy.StarredTopics,
		}
		return nil
	}); err != nil {
		return TUIState{}, err
	}

	if out.Version <= 0 {
		out.Version = CurrentVersion
	}
	if out.ReadMarkers == nil {
		out.ReadMarkers = make(map[string]string)
	}
	if out.Annotations == nil {
		out.Annotations = make(map[string]string)
	}
	if out.Drafts == nil {
		out.Drafts = make(map[string]ComposeDraft)
	}
	if out.Groups == nil {
		out.Groups = make(map[string][]string)
	}
	return out, nil
}

func withFileLock(lockPath string, fn func() error) error {
	if strings.TrimSpace(lockPath) == "" {
		return fn()
	}
	if err := os.MkdirAll(filepath.Dir(lockPath), 0o755); err != nil {
		return err
	}
	f, err := os.OpenFile(lockPath, os.O_CREATE|os.O_RDWR, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX); err != nil {
		return fmt.Errorf("lock %s: %w", lockPath, err)
	}
	defer func() {
		_ = syscall.Flock(int(f.Fd()), syscall.LOCK_UN)
	}()
	return fn()
}

func writeAtomicJSON(path string, state TUIState) error {
	payload, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, payload, 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

func normalizeState(state TUIState, now time.Time) TUIState {
	// Normalize maps.
	if state.ReadMarkers == nil {
		state.ReadMarkers = make(map[string]string)
	}
	if state.Annotations == nil {
		state.Annotations = make(map[string]string)
	}
	if state.Drafts == nil {
		state.Drafts = make(map[string]ComposeDraft)
	}
	if state.Groups == nil {
		state.Groups = make(map[string][]string)
	}

	// Prune + cap bookmarks.
	if len(state.Bookmarks) > 0 {
		pruned := make([]Bookmark, 0, len(state.Bookmarks))
		for _, bm := range state.Bookmarks {
			if bm.MessageID == "" || bm.Topic == "" {
				continue
			}
			bm.MessageID = strings.TrimSpace(bm.MessageID)
			bm.Topic = strings.TrimSpace(bm.Topic)
			bm.Note = strings.TrimSpace(bm.Note)
			if bm.MessageID == "" || bm.Topic == "" {
				continue
			}
			if !bm.CreatedAt.IsZero() && now.Sub(bm.CreatedAt) > bookmarkMaxAge {
				continue
			}
			pruned = append(pruned, bm)
		}
		sort.SliceStable(pruned, func(i, j int) bool {
			return pruned[i].CreatedAt.After(pruned[j].CreatedAt)
		})
		if len(pruned) > maxBookmarks {
			pruned = pruned[:maxBookmarks]
		}
		state.Bookmarks = pruned
	}

	// StarredTopics: de-dupe + sort.
	if len(state.StarredTopics) > 0 {
		seen := make(map[string]struct{}, len(state.StarredTopics))
		out := make([]string, 0, len(state.StarredTopics))
		for _, t := range state.StarredTopics {
			t = strings.TrimSpace(t)
			if t == "" {
				continue
			}
			if _, ok := seen[t]; ok {
				continue
			}
			seen[t] = struct{}{}
			out = append(out, t)
		}
		sort.Strings(out)
		state.StarredTopics = out
	}
	if len(state.Groups) > 0 {
		normalized := make(map[string][]string, len(state.Groups))
		for name, members := range state.Groups {
			trimmed := strings.TrimSpace(name)
			if trimmed == "" {
				continue
			}
			clean := normalizeGroupMembers(members)
			if len(clean) == 0 {
				continue
			}
			normalized[trimmed] = clean
		}
		state.Groups = normalized
	}
	if len(state.NotifyRules) > 0 {
		normalized := make([]NotificationRule, 0, len(state.NotifyRules))
		for _, rule := range state.NotifyRules {
			norm, ok := normalizeNotificationRule(rule)
			if !ok {
				continue
			}
			normalized = append(normalized, norm)
		}
		state.NotifyRules = normalized
	}
	if len(state.Notifications) > 0 {
		normalized := make([]Notification, 0, len(state.Notifications))
		for _, item := range state.Notifications {
			norm, ok := normalizeNotification(item)
			if !ok {
				continue
			}
			normalized = append(normalized, norm)
		}
		sort.SliceStable(normalized, func(i, j int) bool {
			return notificationTimestamp(normalized[i]).After(notificationTimestamp(normalized[j]))
		})
		if len(normalized) > maxNotifications {
			normalized = normalized[:maxNotifications]
		}
		state.Notifications = normalized
	}
	state.Preferences = normalizePreferences(state.Preferences)

	return state
}

func cloneState(state TUIState) TUIState {
	out := state
	out.Preferences = clonePreferences(state.Preferences)
	if state.ReadMarkers != nil {
		out.ReadMarkers = make(map[string]string, len(state.ReadMarkers))
		for k, v := range state.ReadMarkers {
			out.ReadMarkers[k] = v
		}
	}
	if state.Annotations != nil {
		out.Annotations = make(map[string]string, len(state.Annotations))
		for k, v := range state.Annotations {
			out.Annotations[k] = v
		}
	}
	if state.Drafts != nil {
		out.Drafts = make(map[string]ComposeDraft, len(state.Drafts))
		for k, v := range state.Drafts {
			out.Drafts[k] = v
		}
	}
	if state.Groups != nil {
		out.Groups = cloneGroups(state.Groups)
	}
	if len(state.Bookmarks) > 0 {
		out.Bookmarks = append([]Bookmark(nil), state.Bookmarks...)
	}
	if len(state.StarredTopics) > 0 {
		out.StarredTopics = append([]string(nil), state.StarredTopics...)
	}
	if len(state.SavedSearches) > 0 {
		out.SavedSearches = append([]SavedSearch(nil), state.SavedSearches...)
	}
	if len(state.NotifyRules) > 0 {
		out.NotifyRules = append([]NotificationRule(nil), state.NotifyRules...)
	}
	if len(state.Notifications) > 0 {
		out.Notifications = append([]Notification(nil), state.Notifications...)
	}
	return out
}

func cloneGroups(src map[string][]string) map[string][]string {
	if len(src) == 0 {
		return nil
	}
	dst := make(map[string][]string, len(src))
	for name, members := range src {
		if strings.TrimSpace(name) == "" {
			continue
		}
		if len(members) == 0 {
			continue
		}
		dst[name] = append([]string(nil), members...)
	}
	if len(dst) == 0 {
		return nil
	}
	return dst
}

func normalizeGroupMembers(members []string) []string {
	if len(members) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(members))
	out := make([]string, 0, len(members))
	for _, member := range members {
		member = strings.TrimSpace(member)
		if member == "" {
			continue
		}
		if !strings.HasPrefix(member, "@") {
			member = "@" + member
		}
		if _, ok := seen[member]; ok {
			continue
		}
		seen[member] = struct{}{}
		out = append(out, member)
	}
	sort.Strings(out)
	return out
}

func clonePreferences(p Preferences) Preferences {
	out := p
	if len(p.DashboardViews) > 0 {
		out.DashboardViews = append([]string(nil), p.DashboardViews...)
	}
	if len(p.HighlightPatterns) > 0 {
		out.HighlightPatterns = append([]string(nil), p.HighlightPatterns...)
	}
	return out
}

func normalizePreferences(p Preferences) Preferences {
	p.DefaultLayout = strings.TrimSpace(strings.ToLower(p.DefaultLayout))
	switch p.DefaultLayout {
	case "", "single", "split", "dashboard", "zen":
	default:
		p.DefaultLayout = ""
	}
	p.Theme = strings.TrimSpace(strings.ToLower(p.Theme))
	switch p.Theme {
	case "", "default", "high-contrast":
	default:
		p.Theme = ""
	}
	if p.LayoutSplitRatio != 0 {
		if p.LayoutSplitRatio < 0.2 {
			p.LayoutSplitRatio = 0.2
		}
		if p.LayoutSplitRatio > 0.8 {
			p.LayoutSplitRatio = 0.8
		}
	}
	if p.LayoutFocus < 0 {
		p.LayoutFocus = 0
	}
	p.DashboardGrid = strings.TrimSpace(strings.ToLower(p.DashboardGrid))
	switch p.DashboardGrid {
	case "", "2x2", "2x1", "1x2", "1x3", "3x1":
	default:
		p.DashboardGrid = "2x2"
	}
	if len(p.DashboardViews) > 0 {
		out := make([]string, 0, minInt(len(p.DashboardViews), 4))
		for _, id := range p.DashboardViews {
			id = strings.TrimSpace(id)
			if id == "" {
				continue
			}
			out = append(out, id)
			if len(out) == 4 {
				break
			}
		}
		p.DashboardViews = out
	}
	if len(p.HighlightPatterns) > 0 {
		p.HighlightPatterns = normalizeStringList(p.HighlightPatterns)
	}
	return p
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func normalizeStringList(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(values))
	out := make([]string, 0, len(values))
	for _, v := range values {
		v = strings.TrimSpace(v)
		if v == "" {
			continue
		}
		if _, ok := seen[v]; ok {
			continue
		}
		seen[v] = struct{}{}
		out = append(out, v)
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

func normalizeNotificationRule(rule NotificationRule) (NotificationRule, bool) {
	rule.Name = strings.TrimSpace(rule.Name)
	if rule.Name == "" {
		return NotificationRule{}, false
	}
	rule.Topic = strings.TrimSpace(rule.Topic)
	rule.From = strings.TrimSpace(rule.From)
	rule.To = strings.TrimSpace(rule.To)
	rule.Priority = strings.TrimSpace(strings.ToLower(rule.Priority))
	switch rule.Priority {
	case "", "low", "normal", "high":
	default:
		rule.Priority = ""
	}
	rule.Tags = normalizeStringList(rule.Tags)
	rule.Text = strings.TrimSpace(rule.Text)
	if !(rule.ActionHighlight || rule.ActionBell || rule.ActionFlash || rule.ActionBadge) {
		rule.ActionBadge = true
	}
	return rule, true
}

func normalizeNotification(item Notification) (Notification, bool) {
	item.MessageID = strings.TrimSpace(item.MessageID)
	if item.MessageID == "" {
		return Notification{}, false
	}
	item.Target = strings.TrimSpace(item.Target)
	item.From = strings.TrimSpace(item.From)
	item.Priority = strings.TrimSpace(strings.ToLower(item.Priority))
	switch item.Priority {
	case "", "low", "normal", "high":
	default:
		item.Priority = ""
	}
	item.RuleName = strings.TrimSpace(item.RuleName)
	item.RuleLabel = strings.TrimSpace(item.RuleLabel)
	item.Preview = strings.TrimSpace(item.Preview)
	if item.Timestamp.IsZero() && !item.OccurredAt.IsZero() {
		item.Timestamp = item.OccurredAt
	}
	if item.Timestamp.IsZero() {
		if parsed, ok := parseMessageIDTimestamp(item.MessageID); ok {
			item.Timestamp = parsed
		} else {
			item.Timestamp = time.Now().UTC()
		}
	}
	item.Timestamp = item.Timestamp.UTC()
	return item, true
}

func notificationTimestamp(item Notification) time.Time {
	if !item.Timestamp.IsZero() {
		return item.Timestamp
	}
	return item.OccurredAt
}

func parseMessageIDTimestamp(id string) (time.Time, bool) {
	id = strings.TrimSpace(id)
	const prefixLen = len("20060102-150405")
	if len(id) < prefixLen {
		return time.Time{}, false
	}
	parsed, err := time.Parse("20060102-150405", id[:prefixLen])
	if err != nil {
		return time.Time{}, false
	}
	return parsed.UTC(), true
}
