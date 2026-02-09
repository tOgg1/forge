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

	defaultDebounce = 1 * time.Second
	maxBookmarks    = 500
	bookmarkMaxAge  = 30 * 24 * time.Hour
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
	DefaultLayout string `json:"default_layout,omitempty"` // "single", "split", "dashboard", "zen"
	LiveTailAuto  bool   `json:"live_tail_auto,omitempty"`
	RelativeTime  bool   `json:"relative_time,omitempty"`
	SoundAlerts   bool   `json:"sound_alerts,omitempty"`
}

type NotificationRule struct {
	Name     string   `json:"name"`
	Topic    string   `json:"topic,omitempty"`    // glob
	From     string   `json:"from,omitempty"`     // glob
	Priority string   `json:"priority,omitempty"` // min priority
	Tags     []string `json:"tags,omitempty"`
	Enabled  bool     `json:"enabled"`
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

	return state
}

func cloneState(state TUIState) TUIState {
	out := state
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
