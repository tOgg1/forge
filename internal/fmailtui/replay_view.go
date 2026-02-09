package fmailtui

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	tuistate "github.com/tOgg1/forge/internal/fmailtui/state"
)

type replayMode int

const (
	replayModeFeed replayMode = iota
	replayModeTimeline
)

type replayLoadedMsg struct {
	now   time.Time
	msgs  []fmail.Message
	times []time.Time
	start time.Time
	end   time.Time
	err   error
}

type replayTickMsg struct{}

type replayExportedMsg struct {
	path string
	err  error
}

type replayView struct {
	root     string
	self     string
	provider data.MessageProvider
	st       *tuistate.Manager

	loading bool
	lastErr error

	now   time.Time
	start time.Time
	end   time.Time

	msgs  []fmail.Message
	times []time.Time

	idx        int
	playing    bool
	speedIdx   int
	highlight  time.Time
	mode       replayMode
	statusLine string

	pendingMark bool
	pendingJump bool
	marks       map[rune]int
}

func newReplayView(root, self string, provider data.MessageProvider, st *tuistate.Manager) *replayView {
	self = strings.TrimSpace(self)
	if self == "" {
		self = defaultSelfAgent
	}
	return &replayView{
		root:     root,
		self:     self,
		provider: provider,
		st:       st,
		marks:    make(map[rune]int),
	}
}

func (v *replayView) Init() tea.Cmd {
	v.loading = true
	return v.loadCmd()
}

func (v *replayView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case replayLoadedMsg:
		v.applyLoaded(typed)
		return nil
	case replayTickMsg:
		return v.handleTick()
	case replayExportedMsg:
		if typed.err != nil {
			v.statusLine = "export error: " + typed.err.Error()
		} else {
			v.statusLine = "exported: " + typed.path
		}
		return nil
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *replayView) handleKey(msg tea.KeyMsg) tea.Cmd {
	if v.pendingMark {
		return v.handleMarkKey(msg)
	}
	if v.pendingJump {
		return v.handleJumpKey(msg)
	}

	switch msg.String() {
	case "esc", "backspace":
		v.playing = false
		return popViewCmd()
	case " ":
		if len(v.msgs) == 0 {
			return nil
		}
		v.playing = !v.playing
		if v.playing {
			return v.scheduleNextTick()
		}
		return nil
	case "t":
		if v.mode == replayModeFeed {
			v.mode = replayModeTimeline
		} else {
			v.mode = replayModeFeed
		}
		v.persistPrefs()
		return nil
	case "1", "2", "3", "4":
		idx := int(msg.String()[0] - '1')
		if idx >= 0 && idx < len(replaySpeedPresets) {
			v.speedIdx = idx
			v.persistPrefs()
			if v.playing {
				return v.scheduleNextTick()
			}
		}
		return nil
	case "left":
		v.playing = false
		v.step(-1)
		return nil
	case "right":
		v.playing = false
		v.step(1)
		return nil
	case "shift+left":
		v.playing = false
		v.seekBy(-1 * time.Minute)
		return nil
	case "shift+right":
		v.playing = false
		v.seekBy(1 * time.Minute)
		return nil
	case "home":
		v.playing = false
		v.setIndex(0)
		return nil
	case "end":
		v.playing = false
		v.setIndex(maxInt(0, len(v.msgs)-1))
		return nil
	case "m":
		if len(v.msgs) == 0 {
			return nil
		}
		v.pendingMark = true
		v.statusLine = "mark: press letter"
		return nil
	case "'":
		if len(v.msgs) == 0 {
			return nil
		}
		v.pendingJump = true
		v.statusLine = "jump: press letter"
		return nil
	case "e":
		return v.exportCmd()
	}
	return nil
}

func (v *replayView) handleMarkKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "esc":
		v.pendingMark = false
		v.statusLine = ""
		return nil
	}
	if len(msg.Runes) != 1 {
		return nil
	}
	r := msg.Runes[0]
	if r < 'a' || r > 'z' {
		v.statusLine = "mark: use a-z"
		return nil
	}
	v.marks[r] = v.idx
	v.pendingMark = false
	v.statusLine = fmt.Sprintf("marked '%c'", r)
	return nil
}

func (v *replayView) handleJumpKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "esc":
		v.pendingJump = false
		v.statusLine = ""
		return nil
	}
	if len(msg.Runes) != 1 {
		return nil
	}
	r := msg.Runes[0]
	idx, ok := v.marks[r]
	if !ok {
		v.statusLine = fmt.Sprintf("no mark '%c'", r)
		v.pendingJump = false
		return nil
	}
	v.pendingJump = false
	v.playing = false
	v.setIndex(idx)
	v.statusLine = fmt.Sprintf("jumped '%c'", r)
	return nil
}

func (v *replayView) step(delta int) {
	if len(v.msgs) == 0 {
		return
	}
	v.setIndex(clampInt(v.idx+delta, 0, len(v.msgs)-1))
}

func (v *replayView) seekBy(delta time.Duration) {
	if len(v.times) == 0 {
		return
	}
	curr := v.times[v.idx]
	target := curr.Add(delta)
	v.setIndex(replaySeekIndexBeforeOrAt(v.times, target))
}

func (v *replayView) setIndex(idx int) {
	if len(v.msgs) == 0 {
		v.idx = 0
		return
	}
	v.idx = clampInt(idx, 0, len(v.msgs)-1)
	v.persistPrefs()
}

func (v *replayView) persistPrefs() {
	if v.st == nil || len(v.msgs) == 0 {
		return
	}
	cursorID := v.msgs[v.idx].ID
	mode := "feed"
	if v.mode == replayModeTimeline {
		mode = "timeline"
	}
	speedIdx := v.speedIdx
	v.st.UpdatePreferences(func(p *tuistate.Preferences) {
		p.ReplayCursorID = cursorID
		p.ReplaySpeedIdx = speedIdx
		p.ReplayMode = mode
	})
}

func (v *replayView) loadCmd() tea.Cmd {
	provider := v.provider
	self := v.self

	return func() tea.Msg {
		now := time.Now().UTC()
		if provider == nil {
			return replayLoadedMsg{now: now}
		}

		filter := data.MessageFilter{}
		merged := make([]fmail.Message, 0, 4096)
		seen := make(map[string]struct{}, 4096)

		topics, err := provider.Topics()
		if err != nil {
			return replayLoadedMsg{now: now, err: err}
		}
		for i := range topics {
			topic := strings.TrimSpace(topics[i].Name)
			if topic == "" {
				continue
			}
			msgs, err := provider.Messages(topic, filter)
			if err != nil {
				return replayLoadedMsg{now: now, err: err}
			}
			for _, msg := range msgs {
				key := statsDedupKey(msg)
				if _, ok := seen[key]; ok {
					continue
				}
				seen[key] = struct{}{}
				merged = append(merged, msg)
			}
		}

		convs, err := provider.DMConversations(self)
		if err == nil {
			for i := range convs {
				agent := strings.TrimSpace(convs[i].Agent)
				if agent == "" {
					continue
				}
				msgs, dmErr := provider.DMs(agent, filter)
				if dmErr != nil {
					return replayLoadedMsg{now: now, err: dmErr}
				}
				for _, msg := range msgs {
					key := statsDedupKey(msg)
					if _, ok := seen[key]; ok {
						continue
					}
					seen[key] = struct{}{}
					merged = append(merged, msg)
				}
			}
		}

		sortMessages(merged)
		times := make([]time.Time, 0, len(merged))
		var start, end time.Time
		for i := range merged {
			t := replayMessageTime(merged[i])
			times = append(times, t)
			if !t.IsZero() {
				if start.IsZero() || t.Before(start) {
					start = t
				}
				if end.IsZero() || t.After(end) {
					end = t
				}
			}
		}
		if start.IsZero() && len(merged) > 0 {
			start = now
			end = now
		}
		return replayLoadedMsg{now: now, msgs: merged, times: times, start: start, end: end}
	}
}

func (v *replayView) applyLoaded(msg replayLoadedMsg) {
	v.loading = false
	v.lastErr = msg.err
	v.now = msg.now
	if msg.err != nil {
		return
	}
	v.msgs = append(v.msgs[:0], msg.msgs...)
	v.times = append(v.times[:0], msg.times...)
	v.start = msg.start
	v.end = msg.end
	v.idx = 0

	// Restore persisted cursor/speed/mode.
	if v.st != nil && len(v.msgs) > 0 {
		prefs := v.st.Preferences()
		if prefs.ReplaySpeedIdx >= 0 && prefs.ReplaySpeedIdx < len(replaySpeedPresets) {
			v.speedIdx = prefs.ReplaySpeedIdx
		}
		if strings.TrimSpace(prefs.ReplayMode) == "timeline" {
			v.mode = replayModeTimeline
		} else {
			v.mode = replayModeFeed
		}
		if cursor := strings.TrimSpace(prefs.ReplayCursorID); cursor != "" {
			for i := range v.msgs {
				if v.msgs[i].ID == cursor {
					v.idx = i
					break
				}
			}
		}
	}
	v.persistPrefs()
}

func (v *replayView) handleTick() tea.Cmd {
	if !v.playing || len(v.msgs) == 0 {
		return nil
	}
	if v.idx >= len(v.msgs)-1 {
		v.playing = false
		return nil
	}
	v.idx++
	v.highlight = time.Now().UTC().Add(500 * time.Millisecond)
	v.persistPrefs()
	return v.scheduleNextTick()
}

func (v *replayView) scheduleNextTick() tea.Cmd {
	if !v.playing || len(v.msgs) == 0 || v.idx >= len(v.msgs)-1 {
		return nil
	}
	currT := v.times[v.idx]
	nextT := v.times[v.idx+1]
	speed := replaySpeedPresets[clampInt(v.speedIdx, 0, len(replaySpeedPresets)-1)]
	d := replayNextInterval(currT, nextT, speed)
	return tea.Tick(d, func(time.Time) tea.Msg { return replayTickMsg{} })
}

func (v *replayView) exportCmd() tea.Cmd {
	root := v.root
	start := v.start
	end := v.end
	msgs := append([]fmail.Message(nil), v.msgs...)
	times := append([]time.Time(nil), v.times...)

	return func() tea.Msg {
		if strings.TrimSpace(root) == "" {
			return replayExportedMsg{err: fmt.Errorf("root not set")}
		}
		now := time.Now().UTC()
		outDir := filepath.Join(root, ".fmail", "exports")
		if err := os.MkdirAll(outDir, 0o755); err != nil {
			return replayExportedMsg{err: err}
		}
		path := filepath.Join(outDir, fmt.Sprintf("replay-%s.md", now.Format("20060102-150405")))
		f, err := os.Create(path)
		if err != nil {
			return replayExportedMsg{err: err}
		}
		defer func() { _ = f.Close() }()

		_, _ = fmt.Fprintf(f, "# fmail replay export\n\n")
		_, _ = fmt.Fprintf(f, "time range: %s .. %s\n\n", start.Format(time.RFC3339), end.Format(time.RFC3339))
		for i := range msgs {
			t := times[i]
			if t.IsZero() {
				t = replayMessageTime(msgs[i])
			}
			line := fmt.Sprintf("%s %s -> %s", t.Format("15:04:05"), msgs[i].From, msgs[i].To)
			body := firstLine(msgs[i].Body)
			if strings.TrimSpace(body) != "" {
				line += ": " + body
			}
			_, _ = fmt.Fprintf(f, "- %s\n", line)
		}
		return replayExportedMsg{path: path}
	}
}
