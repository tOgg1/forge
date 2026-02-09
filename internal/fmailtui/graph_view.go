package fmailtui

import (
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
)

type graphLoadedMsg struct {
	now  time.Time
	msgs []fmail.Message
	err  error
}

type graphIncomingMsg struct {
	msg fmail.Message
}

type graphView struct {
	root     string
	self     string
	provider data.MessageProvider

	windows   []time.Duration
	windowIdx int
	windowEnd time.Time

	loading bool
	lastErr error

	all  []fmail.Message
	seen map[string]struct{}

	snap graphSnapshot

	zoom         int
	panX         int
	panY         int
	selected     int
	showDetails  bool
	topicOverlay bool
	clusters     bool

	subCh     <-chan fmail.Message
	subCancel func()
}

func newGraphView(root, self string, provider data.MessageProvider) *graphView {
	self = strings.TrimSpace(self)
	if self == "" {
		self = defaultSelfAgent
	}
	return &graphView{
		root:     root,
		self:     self,
		provider: provider,
		windows: []time.Duration{
			1 * time.Hour,
			4 * time.Hour,
			12 * time.Hour,
			24 * time.Hour,
			7 * 24 * time.Hour,
			0, // all-time
		},
		windowIdx:   1,
		seen:        make(map[string]struct{}, 1024),
		zoom:        0,
		showDetails: true,
	}
}

func (v *graphView) Init() tea.Cmd {
	v.startSubscription()
	v.loading = true
	return tea.Batch(v.loadCmd(), v.waitForMessageCmd())
}

func (v *graphView) Close() {
	if v.subCancel != nil {
		v.subCancel()
		v.subCancel = nil
	}
	v.subCh = nil
}

func (v *graphView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.subCancel = cancel
}

func (v *graphView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return graphIncomingMsg{msg: msg}
	}
}

func (v *graphView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case graphLoadedMsg:
		v.applyLoaded(typed)
		return nil
	case graphIncomingMsg:
		v.applyIncoming(typed.msg)
		return v.waitForMessageCmd()
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *graphView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "r", "ctrl+r":
		v.loading = true
		return v.loadCmd()
	case "[":
		if v.windowIdx > 0 {
			v.windowIdx--
			v.windowEnd = time.Time{}
			v.loading = true
			return v.loadCmd()
		}
	case "]":
		if v.windowIdx < len(v.windows)-1 {
			v.windowIdx++
			v.windowEnd = time.Time{}
			v.loading = true
			return v.loadCmd()
		}
	case "h":
		if v.windows[v.windowIdx] > 0 {
			v.windowEnd = v.windowEnd.Add(-v.panStep())
			v.loading = true
			return v.loadCmd()
		}
	case "l":
		if v.windows[v.windowIdx] > 0 {
			v.windowEnd = v.windowEnd.Add(v.panStep())
			v.loading = true
			return v.loadCmd()
		}
	case "tab":
		if len(v.snap.Nodes) > 0 {
			v.selected = (v.selected + 1) % len(v.snap.Nodes)
		}
	case "shift+tab":
		if len(v.snap.Nodes) > 0 {
			v.selected--
			if v.selected < 0 {
				v.selected = len(v.snap.Nodes) - 1
			}
		}
	case "enter":
		v.showDetails = !v.showDetails
	case "t":
		v.topicOverlay = !v.topicOverlay
	case "c":
		v.clusters = !v.clusters
	case "up":
		v.panY--
	case "down":
		v.panY++
	case "left":
		v.panX--
	case "right":
		v.panX++
	case "ctrl+left":
		v.panX--
	case "ctrl+right":
		v.panX++
	case "+":
		if v.zoom < 6 {
			v.zoom++
		}
	case "-":
		if v.zoom > -3 {
			v.zoom--
		}
	}
	return nil
}

func (v *graphView) panStep() time.Duration {
	d := v.windows[v.windowIdx]
	if d <= 0 {
		return 0
	}
	step := d / 6
	if step < 15*time.Minute {
		step = 15 * time.Minute
	}
	return step
}

func (v *graphView) loadCmd() tea.Cmd {
	provider := v.provider
	self := v.self
	windowIdx := v.windowIdx
	windowEnd := v.windowEnd
	windows := append([]time.Duration(nil), v.windows...)

	return func() tea.Msg {
		now := time.Now().UTC()
		if provider == nil {
			return graphLoadedMsg{now: now}
		}

		d := time.Duration(0)
		if windowIdx >= 0 && windowIdx < len(windows) {
			d = windows[windowIdx]
		}
		allTime := d == 0

		end := windowEnd
		if end.IsZero() {
			end = now
		}
		start := end.Add(-d)

		filter := data.MessageFilter{}
		if !allTime {
			filter.Since = start
			filter.Until = end
		}

		merged := make([]fmail.Message, 0, 1024)
		seen := make(map[string]struct{}, 1024)

		topics, err := provider.Topics()
		if err != nil {
			return graphLoadedMsg{now: now, err: err}
		}
		for i := range topics {
			topic := strings.TrimSpace(topics[i].Name)
			if topic == "" {
				continue
			}
			msgs, err := provider.Messages(topic, filter)
			if err != nil {
				return graphLoadedMsg{now: now, err: err}
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
					return graphLoadedMsg{now: now, err: dmErr}
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
		return graphLoadedMsg{now: now, msgs: merged}
	}
}

func (v *graphView) applyLoaded(msg graphLoadedMsg) {
	v.loading = false
	v.lastErr = msg.err
	if msg.err != nil {
		return
	}

	v.all = append(v.all[:0], msg.msgs...)
	v.seen = make(map[string]struct{}, len(v.all))
	for i := range v.all {
		v.seen[statsDedupKey(v.all[i])] = struct{}{}
	}

	v.snap = buildGraphSnapshot(v.all, graphMaxNodesDefault)
	if v.selected < 0 || v.selected >= len(v.snap.Nodes) {
		v.selected = 0
	}
}

func (v *graphView) applyIncoming(msg fmail.Message) {
	key := statsDedupKey(msg)
	if _, ok := v.seen[key]; ok {
		return
	}
	v.seen[key] = struct{}{}
	v.all = append(v.all, msg)

	// Cheap and safe: rebuild. (Message volume is low enough for this view.)
	v.snap = buildGraphSnapshot(v.all, graphMaxNodesDefault)
	if v.selected < 0 || v.selected >= len(v.snap.Nodes) {
		v.selected = 0
	}
}
