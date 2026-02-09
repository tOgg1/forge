package fmailtui

import (
	"fmt"
	"path/filepath"
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
	timelineRefreshInterval = 2 * time.Second
	timelineGapThreshold    = 5 * time.Minute
	timelineMaxLanes        = 8
	timelineInitialPageSize = 200
	timelinePageSize        = 200
	timelineLoadThreshold   = 6
)

var timelineZoomLevels = []time.Duration{
	time.Minute,
	5 * time.Minute,
	15 * time.Minute,
	time.Hour,
	4 * time.Hour,
	24 * time.Hour,
}

type timelineMode int

const (
	timelineModeChronological timelineMode = iota
	timelineModeSwimlane
)

type timelineAgentOrder int

const (
	timelineOrderFirstSeen timelineAgentOrder = iota
	timelineOrderAlphabetical
)

type timelineTickMsg struct{}

type timelineLoadedMsg struct {
	now               time.Time
	messages          []fmail.Message
	topicParticipants map[string][]string
	hasOlder          bool
	mode              timelineLoadMode
	err               error
}

type timelineLoadMode int

const (
	timelineLoadReplace timelineLoadMode = iota
	timelineLoadOlder
)

type timelineIncomingMsg struct {
	msg fmail.Message
}

type timelineFilter struct {
	From        string
	To          string
	In          string
	Priority    string
	Tags        []string
	Text        string
	Since       time.Duration
	Until       time.Duration
	HasReply    bool
	HasBookmark bool
}

type timelineItem struct {
	msg              fmail.Message
	parentVisible    bool
	targetDisplay    string
	topicColorLookup string
	timestamp        time.Time
}

type timelineView struct {
	root     string
	self     string
	provider data.MessageProvider
	state    *tuistate.Manager

	now               time.Time
	lastErr           error
	all               []fmail.Message
	visible           []timelineItem
	repliedByParentID map[string]struct{}
	bookmarkedIDs     map[string]struct{}
	topicParticipants map[string][]string

	mode      timelineMode
	agentSort timelineAgentOrder
	zoomIdx   int
	windowEnd time.Time
	hasOlder  bool

	selected   int
	selectedID string
	top        int

	filter       timelineFilter
	filterRaw    string
	filterActive bool
	filterInput  string

	jumpActive bool
	jumpInput  string

	noteActive      bool
	noteInput       string
	noteTargetID    string
	noteTargetTopic string

	detailOpen bool
	laneOffset int

	subCh     <-chan fmail.Message
	subCancel func()
	loading   bool
}

var _ composeContextView = (*timelineView)(nil)

func newTimelineView(root, self string, provider data.MessageProvider, st *tuistate.Manager) *timelineView {
	self = strings.TrimSpace(self)
	if self == "" {
		self = defaultSelfAgent
	}
	return &timelineView{
		root:              root,
		self:              self,
		provider:          provider,
		state:             st,
		mode:              timelineModeChronological,
		agentSort:         timelineOrderFirstSeen,
		zoomIdx:           1,
		repliedByParentID: make(map[string]struct{}),
		bookmarkedIDs:     make(map[string]struct{}),
		topicParticipants: make(map[string][]string),
	}
}

func timelineTickCmd() tea.Cmd {
	return tea.Tick(timelineRefreshInterval, func(time.Time) tea.Msg { return timelineTickMsg{} })
}

func (v *timelineView) Init() tea.Cmd {
	v.startSubscription()
	v.loading = true
	return tea.Batch(v.loadWindowCmd(data.MessageFilter{Limit: timelineInitialPageSize}, timelineLoadReplace), timelineTickCmd(), v.waitForMessageCmd())
}

func (v *timelineView) Close() {
	if v.subCancel != nil {
		v.subCancel()
		v.subCancel = nil
	}
	v.subCh = nil
}

func (v *timelineView) ComposeTarget() string {
	msg, ok := v.selectedMessage()
	if !ok {
		return ""
	}
	return strings.TrimSpace(msg.To)
}

func (v *timelineView) ComposeReplySeed(dmDirect bool) (composeReplySeed, bool) {
	msg, ok := v.selectedMessage()
	if !ok {
		return composeReplySeed{}, false
	}
	target := strings.TrimSpace(msg.To)
	if dmDirect {
		target = "@" + strings.TrimSpace(msg.From)
	}
	return composeReplySeed{
		Target:     target,
		ReplyTo:    strings.TrimSpace(msg.ID),
		ParentLine: firstNonEmptyLine(messageBodyString(msg.Body)),
	}, true
}

func (v *timelineView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case timelineTickMsg:
		v.now = time.Now().UTC()
		return timelineTickCmd()
	case timelineLoadedMsg:
		v.applyLoaded(typed)
		return nil
	case timelineIncomingMsg:
		v.applyIncoming(typed.msg)
		return v.waitForMessageCmd()
	case tea.KeyMsg:
		return v.handleKey(typed)
	}
	return nil
}

func (v *timelineView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}

	palette := themePalette(theme)
	base := lipgloss.NewStyle().
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Background(lipgloss.Color(palette.Base.Background))
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))

	if v.now.IsZero() {
		v.now = time.Now().UTC()
	}
	v.rebuildVisible()

	windowStart, windowEnd := v.windowBounds()
	modeLabel := "TIMELINE"
	if v.mode == timelineModeSwimlane {
		modeLabel = "SEQUENCE"
	}
	filterLabel := "none"
	if strings.TrimSpace(v.filterRaw) != "" {
		filterLabel = v.filterRaw
	}
	olderLabel := "end"
	if v.loading {
		olderLabel = "loading"
	} else if v.hasOlder {
		olderLabel = "more"
	}
	head := fmt.Sprintf("%s  %s - %s  zoom:%s  filter:%s  older:%s  %d/%d",
		modeLabel,
		windowStart.Format("15:04"),
		windowEnd.Format("15:04"),
		timelineZoomLabel(v.zoomWindow()),
		filterLabel,
		olderLabel,
		len(v.visible),
		len(v.all),
	)

	lines := make([]string, 0, 4)
	lines = append(lines, lipgloss.NewStyle().Bold(true).Render(truncateVis(head, maxInt(0, width))))
	if v.filterActive {
		lines = append(lines, muted.Render("f filter: ")+v.filterInput)
	}
	if v.jumpActive {
		lines = append(lines, muted.Render("t jump (time/date): ")+v.jumpInput)
	}
	if v.noteActive {
		lines = append(lines, muted.Render("B bookmark note: ")+v.noteInput)
	}

	contentHeight := height - len(lines)
	if contentHeight < 0 {
		contentHeight = 0
	}

	var body string
	if v.mode == timelineModeSwimlane {
		body = strings.Join(v.renderSwimlane(width, contentHeight, palette), "\n")
	} else {
		body = strings.Join(v.renderChronological(width, contentHeight, palette), "\n")
	}
	lines = append(lines, body)

	content := lipgloss.JoinVertical(lipgloss.Left, lines...)
	if v.detailOpen {
		content = v.renderDetailOverlay(content, width, height, palette)
	}
	if v.lastErr != nil {
		errLine := lipgloss.NewStyle().
			Foreground(lipgloss.Color(palette.Priority.High)).
			Render("data error: " + truncate(v.lastErr.Error(), maxInt(0, width-2)))
		content = lipgloss.JoinVertical(lipgloss.Left, content, errLine)
	}
	return base.Render(content)
}

func (v *timelineView) wantsKey(key string) bool {
	if v.noteActive {
		switch key {
		case "ctrl+c":
			return false
		default:
			return true
		}
	}

	switch key {
	case "n", "t", "o", "a", "h", "l", "left", "right", "+", "=", "-", "_", "1", "2", "3", "[", "]", "b", "B":
		return true
	case "esc":
		return v.filterActive || v.jumpActive || v.detailOpen || v.noteActive
	default:
		return false
	}
}

func (v *timelineView) handleKey(msg tea.KeyMsg) tea.Cmd {
	if v.filterActive {
		return v.handleFilterKey(msg)
	}
	if v.jumpActive {
		return v.handleJumpKey(msg)
	}
	if v.noteActive {
		return v.handleNoteKey(msg)
	}
	if v.detailOpen {
		switch msg.String() {
		case "enter", "d":
			v.detailOpen = false
			return nil
		case "o":
			return v.openSelectedInThreadCmd()
		case "b":
			v.toggleSelectedBookmark()
			return nil
		case "B":
			v.openSelectedBookmarkNote()
			return nil
		case "esc":
			v.detailOpen = false
			return nil
		}
	}

	switch msg.String() {
	case "esc", "backspace":
		return popViewCmd()
	case "j", "down":
		v.moveSelection(1)
		return nil
	case "k", "up":
		v.moveSelection(-1)
		return v.maybeLoadOlderCmd()
	case "pgdown", "ctrl+d":
		v.moveSelection(maxInt(1, v.pageStep()))
		return nil
	case "pgup", "ctrl+u":
		v.moveSelection(-maxInt(1, v.pageStep()))
		return v.maybeLoadOlderCmd()
	case "g", "home":
		v.selected = 0
		v.rememberSelection()
		return v.maybeLoadOlderCmd()
	case "G", "end":
		if len(v.visible) > 0 {
			v.selected = len(v.visible) - 1
			v.rememberSelection()
		}
		return nil
	case "s":
		if v.mode == timelineModeChronological {
			v.mode = timelineModeSwimlane
		} else {
			v.mode = timelineModeChronological
		}
		v.top = 0
		v.laneOffset = 0
		return nil
	case "a":
		if v.agentSort == timelineOrderFirstSeen {
			v.agentSort = timelineOrderAlphabetical
		} else {
			v.agentSort = timelineOrderFirstSeen
		}
		return nil
	case "[":
		v.laneOffset = maxInt(0, v.laneOffset-1)
		return nil
	case "]":
		v.laneOffset++
		return nil
	case "left", "h":
		v.panWindow(-1)
		return nil
	case "right", "l":
		v.panWindow(1)
		return nil
	case "+", "=":
		v.zoomIn()
		return nil
	case "-", "_":
		v.zoomOut()
		return nil
	case "n":
		v.jumpNow()
		return nil
	case "t":
		v.jumpActive = true
		v.jumpInput = ""
		return nil
	case "f":
		v.filterActive = true
		v.filterInput = v.filterRaw
		return nil
	case "c":
		v.filterRaw = ""
		v.filter = timelineFilter{}
		v.rebuildVisible()
		return nil
	case "enter":
		if len(v.visible) > 0 {
			v.detailOpen = true
		}
		return nil
	case "o":
		return v.openSelectedInThreadCmd()
	case "b":
		v.toggleSelectedBookmark()
		return nil
	case "B":
		v.openSelectedBookmarkNote()
		return nil
	case "1":
		v.applyQuickFilter("to", v.selectedTarget())
		return nil
	case "2":
		v.applyQuickFilter("from", v.selectedSender())
		return nil
	case "3":
		v.applyQuickFilter("priority", fmail.PriorityHigh)
		return nil
	}
	return nil
}

func (v *timelineView) handleFilterKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.filterActive = false
		return nil
	case tea.KeyEnter:
		v.filterRaw = strings.TrimSpace(v.filterInput)
		v.filter = parseTimelineFilter(v.filterRaw)
		v.filterActive = false
		v.rebuildVisible()
		return nil
	case tea.KeyBackspace:
		v.filterInput = trimLastRune(v.filterInput)
		return nil
	case tea.KeyRunes:
		v.filterInput += string(msg.Runes)
		return nil
	}
	return nil
}

func (v *timelineView) handleJumpKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.jumpActive = false
		return nil
	case tea.KeyEnter:
		parsed, ok := parseTimelineJump(v.jumpInput, v.now)
		if ok {
			v.windowEnd = parsed.UTC()
			v.rebuildVisible()
		}
		v.jumpActive = false
		return nil
	case tea.KeyBackspace:
		v.jumpInput = trimLastRune(v.jumpInput)
		return nil
	case tea.KeyRunes:
		v.jumpInput += string(msg.Runes)
		return nil
	}
	return nil
}

func (v *timelineView) openSelectedBookmarkNote() {
	msg, ok := v.selectedMessage()
	if !ok || v.state == nil {
		return
	}
	id := strings.TrimSpace(msg.ID)
	if id == "" {
		return
	}
	target := strings.TrimSpace(msg.To)
	if target == "" {
		return
	}
	v.noteActive = true
	v.noteTargetID = id
	v.noteTargetTopic = target
	v.noteInput = v.state.BookmarkNote(id)
}

func (v *timelineView) handleNoteKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.noteActive = false
		v.noteInput = ""
		v.noteTargetID = ""
		v.noteTargetTopic = ""
		return nil
	case tea.KeyEnter:
		if v.state != nil && strings.TrimSpace(v.noteTargetID) != "" && strings.TrimSpace(v.noteTargetTopic) != "" {
			v.state.UpsertBookmark(v.noteTargetID, v.noteTargetTopic, v.noteInput)
			v.state.SaveSoon()
			v.refreshBookmarks()
		}
		v.noteActive = false
		v.noteInput = ""
		v.noteTargetID = ""
		v.noteTargetTopic = ""
		return nil
	case tea.KeyBackspace, tea.KeyDelete:
		v.noteInput = trimLastRune(v.noteInput)
		return nil
	case tea.KeyRunes:
		v.noteInput += string(msg.Runes)
		return nil
	}
	return nil
}

func (v *timelineView) openSelectedInThreadCmd() tea.Cmd {
	msg, ok := v.selectedMessage()
	if !ok {
		return nil
	}
	target := strings.TrimSpace(msg.To)
	if target == "" {
		return nil
	}
	return tea.Batch(openThreadCmd(target, strings.TrimSpace(msg.ID)), pushViewCmd(ViewThread))
}

func (v *timelineView) toggleSelectedBookmark() {
	msg, ok := v.selectedMessage()
	if !ok || v.state == nil {
		return
	}
	v.state.ToggleBookmark(strings.TrimSpace(msg.ID), strings.TrimSpace(msg.To))
	v.state.SaveSoon()
	v.refreshBookmarks()
}

func (v *timelineView) applyQuickFilter(key, value string) {
	value = strings.TrimSpace(value)
	if value == "" {
		return
	}
	v.filterRaw = key + ":" + value
	v.filter = parseTimelineFilter(v.filterRaw)
	v.rebuildVisible()
}

func (v *timelineView) selectedSender() string {
	msg, ok := v.selectedMessage()
	if !ok {
		return ""
	}
	return strings.TrimSpace(msg.From)
}

func (v *timelineView) selectedTarget() string {
	msg, ok := v.selectedMessage()
	if !ok {
		return ""
	}
	return strings.TrimSpace(msg.To)
}

func (v *timelineView) selectedMessage() (fmail.Message, bool) {
	if len(v.visible) == 0 {
		return fmail.Message{}, false
	}
	if v.selected < 0 || v.selected >= len(v.visible) {
		return fmail.Message{}, false
	}
	return v.visible[v.selected].msg, true
}

func (v *timelineView) pageStep() int {
	return 8
}

func (v *timelineView) moveSelection(delta int) {
	if len(v.visible) == 0 || delta == 0 {
		return
	}
	v.selected = clampInt(v.selected+delta, 0, len(v.visible)-1)
	v.rememberSelection()
}

func (v *timelineView) maybeLoadOlderCmd() tea.Cmd {
	if v.provider == nil || v.loading || !v.hasOlder || len(v.visible) == 0 {
		return nil
	}
	if v.selected > timelineLoadThreshold {
		return nil
	}
	oldest := messageTimestamp(v.all[0])
	if oldest.IsZero() {
		return nil
	}
	v.loading = true
	return v.loadWindowCmd(data.MessageFilter{
		Until: oldest.Add(-time.Nanosecond),
		Limit: timelinePageSize,
	}, timelineLoadOlder)
}

func (v *timelineView) rememberSelection() {
	if len(v.visible) == 0 || v.selected < 0 || v.selected >= len(v.visible) {
		v.selectedID = ""
		return
	}
	v.selectedID = strings.TrimSpace(v.visible[v.selected].msg.ID)
}

func (v *timelineView) zoomWindow() time.Duration {
	if len(timelineZoomLevels) == 0 {
		return 5 * time.Minute
	}
	idx := clampInt(v.zoomIdx, 0, len(timelineZoomLevels)-1)
	return timelineZoomLevels[idx]
}

func (v *timelineView) zoomIn() {
	if v.zoomIdx > 0 {
		v.zoomIdx--
		v.rebuildVisible()
	}
}

func (v *timelineView) zoomOut() {
	if v.zoomIdx < len(timelineZoomLevels)-1 {
		v.zoomIdx++
		v.rebuildVisible()
	}
}

func (v *timelineView) panWindow(direction int) {
	if direction == 0 {
		return
	}
	window := v.zoomWindow()
	shift := window / 4
	if shift < time.Second {
		shift = time.Second
	}
	if v.windowEnd.IsZero() {
		v.windowEnd = v.latestTime(time.Now().UTC())
	}
	v.windowEnd = v.windowEnd.Add(time.Duration(direction) * shift)
	latest := v.latestTime(v.now)
	if !latest.IsZero() && v.windowEnd.After(latest) {
		v.windowEnd = latest
	}
	v.rebuildVisible()
}

func (v *timelineView) jumpNow() {
	now := time.Now().UTC()
	latest := v.latestTime(now)
	if latest.After(now) {
		v.windowEnd = latest
	} else {
		v.windowEnd = now
	}
	v.rebuildVisible()
}

func (v *timelineView) windowBounds() (time.Time, time.Time) {
	end := v.windowEnd
	if end.IsZero() {
		end = v.latestTime(v.now)
	}
	if end.IsZero() {
		end = time.Now().UTC()
	}
	start := end.Add(-v.zoomWindow())
	return start, end
}

func (v *timelineView) startSubscription() {
	if v.provider == nil || v.subCh != nil {
		return
	}
	ch, cancel := v.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	v.subCh = ch
	v.subCancel = cancel
}

func (v *timelineView) waitForMessageCmd() tea.Cmd {
	if v.subCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-v.subCh
		if !ok {
			return nil
		}
		return timelineIncomingMsg{msg: msg}
	}
}

func (v *timelineView) loadWindowCmd(base data.MessageFilter, mode timelineLoadMode) tea.Cmd {
	provider := v.provider
	self := v.self
	return func() tea.Msg {
		now := time.Now().UTC()
		if provider == nil {
			return timelineLoadedMsg{now: now, mode: mode}
		}

		merged := make([]fmail.Message, 0, 1024)
		seen := make(map[string]struct{}, 1024)
		participants := make(map[string]map[string]struct{})
		hasOlder := false

		topics, err := provider.Topics()
		if err != nil {
			return timelineLoadedMsg{now: now, mode: mode, err: err}
		}
		for i := range topics {
			topic := strings.TrimSpace(topics[i].Name)
			if topic == "" {
				continue
			}

			opts := base
			msgs, err := provider.Messages(topic, opts)
			if err != nil {
				return timelineLoadedMsg{now: now, mode: mode, err: err}
			}
			if opts.Limit > 0 && len(msgs) >= opts.Limit {
				hasOlder = true
			}

			bucket := participants[topic]
			if bucket == nil {
				bucket = make(map[string]struct{}, 8)
				participants[topic] = bucket
			}
			for _, msg := range msgs {
				key := timelineDedupKey(msg)
				if _, ok := seen[key]; ok {
					continue
				}
				seen[key] = struct{}{}
				merged = append(merged, msg)
				if from := strings.TrimSpace(msg.From); from != "" {
					bucket[from] = struct{}{}
				}
			}
		}

		convs, err := provider.DMConversations(self)
		if err == nil {
			for i := range convs {
				agent := strings.TrimSpace(convs[i].Agent)
				if agent == "" {
					continue
				}

				opts := base
				msgs, dmErr := provider.DMs(agent, opts)
				if dmErr != nil {
					return timelineLoadedMsg{now: now, mode: mode, err: dmErr}
				}
				if opts.Limit > 0 && len(msgs) >= opts.Limit {
					hasOlder = true
				}
				for _, msg := range msgs {
					key := timelineDedupKey(msg)
					if _, ok := seen[key]; ok {
						continue
					}
					seen[key] = struct{}{}
					merged = append(merged, msg)
				}
			}
		}

		sortMessages(merged)
		return timelineLoadedMsg{
			now:               now,
			messages:          merged,
			topicParticipants: flattenParticipants(participants),
			hasOlder:          hasOlder,
			mode:              mode,
		}
	}
}

func (v *timelineView) applyLoaded(msg timelineLoadedMsg) {
	v.loading = false
	v.now = msg.now
	v.lastErr = msg.err
	if msg.err != nil {
		return
	}

	keepSelection := v.selectedID
	prevLatest := v.latestTime(v.now)
	followTail := msg.mode == timelineLoadReplace && (v.windowEnd.IsZero() || (!prevLatest.IsZero() && !v.windowEnd.Before(prevLatest.Add(-2*time.Second))))

	if msg.mode == timelineLoadReplace {
		v.all = append(v.all[:0], msg.messages...)
		v.topicParticipants = msg.topicParticipants
	} else {
		v.all = mergeTimelineMessages(v.all, msg.messages)
		v.topicParticipants = mergeTopicParticipants(v.topicParticipants, msg.topicParticipants)
	}
	v.hasOlder = msg.hasOlder

	v.rebuildReplyIndex()
	v.refreshBookmarks()

	if followTail {
		v.windowEnd = v.latestTime(v.now)
	}
	v.rebuildVisible()
	if keepSelection != "" {
		v.restoreSelectionByID(keepSelection)
	}
}

func (v *timelineView) applyIncoming(msg fmail.Message) {
	v.now = time.Now().UTC()
	key := timelineDedupKey(msg)
	for i := range v.all {
		if timelineDedupKey(v.all[i]) == key {
			return
		}
	}

	prevLatest := v.latestTime(v.now)
	followTail := v.windowEnd.IsZero() || (!prevLatest.IsZero() && !v.windowEnd.Before(prevLatest.Add(-2*time.Second)))
	v.all = append(v.all, msg)
	sortMessages(v.all)
	v.rebuildReplyIndex()
	v.refreshBookmarks()
	v.updateParticipantsFromMessage(msg)
	if followTail {
		v.windowEnd = v.latestTime(v.now)
	}
	v.rebuildVisible()
}

func (v *timelineView) rebuildReplyIndex() {
	v.repliedByParentID = make(map[string]struct{}, len(v.all))
	for i := range v.all {
		replyTo := strings.TrimSpace(v.all[i].ReplyTo)
		if replyTo == "" {
			continue
		}
		v.repliedByParentID[replyTo] = struct{}{}
	}
}

func (v *timelineView) refreshBookmarks() {
	v.bookmarkedIDs = make(map[string]struct{}, 64)
	if v.state == nil {
		return
	}
	snap := v.state.Snapshot()
	for _, bm := range snap.Bookmarks {
		id := strings.TrimSpace(bm.MessageID)
		if id == "" {
			continue
		}
		v.bookmarkedIDs[id] = struct{}{}
	}
}

func (v *timelineView) updateParticipantsFromMessage(msg fmail.Message) {
	target := strings.TrimSpace(msg.To)
	if target == "" || strings.HasPrefix(target, "@") {
		return
	}
	if v.topicParticipants == nil {
		v.topicParticipants = make(map[string][]string)
	}
	part := append([]string(nil), v.topicParticipants[target]...)
	part = append(part, strings.TrimSpace(msg.From))
	v.topicParticipants[target] = dedupeSorted(part)
}

func (v *timelineView) rebuildVisible() {
	windowStart, windowEnd := v.windowBounds()
	now := v.now
	if now.IsZero() {
		now = time.Now().UTC()
	}

	items := make([]timelineItem, 0, len(v.all))
	visibleIDs := make(map[string]struct{}, len(v.all))
	for i := range v.all {
		msg := v.all[i]
		ts := messageTimestamp(msg)
		if ts.Before(windowStart) || ts.After(windowEnd) {
			continue
		}
		if !v.filter.matches(msg, now, v.repliedByParentID, v.bookmarkedIDs) {
			continue
		}
		item := timelineItem{
			msg:              msg,
			targetDisplay:    timelineTargetLabel(msg.To),
			topicColorLookup: timelineTopicColorKey(msg.To),
			timestamp:        ts,
		}
		items = append(items, item)
		if id := strings.TrimSpace(msg.ID); id != "" {
			visibleIDs[id] = struct{}{}
		}
	}
	for i := range items {
		replyTo := strings.TrimSpace(items[i].msg.ReplyTo)
		if replyTo == "" {
			continue
		}
		_, items[i].parentVisible = visibleIDs[replyTo]
	}

	v.visible = items
	if len(v.visible) == 0 {
		v.selected = 0
		v.top = 0
		v.selectedID = ""
		return
	}

	if v.selectedID != "" {
		v.restoreSelectionByID(v.selectedID)
	} else if v.selected < 0 || v.selected >= len(v.visible) {
		v.selected = len(v.visible) - 1
	}

	v.selected = clampInt(v.selected, 0, len(v.visible)-1)
	v.rememberSelection()
	v.top = clampInt(v.top, 0, len(v.visible)-1)
}

func (v *timelineView) restoreSelectionByID(id string) {
	id = strings.TrimSpace(id)
	if id == "" || len(v.visible) == 0 {
		return
	}
	for i := range v.visible {
		if strings.TrimSpace(v.visible[i].msg.ID) == id {
			v.selected = i
			v.selectedID = id
			return
		}
	}
}

func (v *timelineView) latestTime(fallback time.Time) time.Time {
	for i := len(v.all) - 1; i >= 0; i-- {
		ts := messageTimestamp(v.all[i])
		if !ts.IsZero() {
			return ts
		}
	}
	return fallback
}

func (v *timelineView) renderChronological(width, height int, palette styles.Theme) []string {
	if height <= 0 {
		return nil
	}
	if len(v.visible) == 0 {
		return clampRenderedLines([]string{lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("no messages in selected window")}, height)
	}

	agentColors := styles.NewAgentColorMapper()
	topicColors := styles.NewAgentColorMapper()
	selectedStyle := lipgloss.NewStyle().Background(lipgloss.Color(palette.Chrome.SelectedItem))
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))

	lines := make([]string, 0, height+4)
	v.ensureViewport(height / 2)
	start := clampInt(v.top, 0, len(v.visible)-1)
	var prev time.Time
	for i := start; i < len(v.visible) && len(lines) < height; i++ {
		item := v.visible[i]
		if !prev.IsZero() {
			gap := item.timestamp.Sub(prev)
			if gap > timelineGapThreshold && len(lines) < height {
				lines = append(lines, muted.Render(fmt.Sprintf("        ── %s gap ──", timelineGapLabel(gap))))
			}
		}
		prev = item.timestamp

		replyMark := " "
		if strings.TrimSpace(item.msg.ReplyTo) != "" {
			if item.parentVisible {
				replyMark = "│"
			} else {
				replyMark = "╎"
			}
		}
		head := fmt.Sprintf("%s %s %s -> %s",
			item.timestamp.Format("15:04:05"),
			replyMark,
			agentColors.Foreground(item.msg.From).Render(agentColors.Plain(item.msg.From)),
			item.targetDisplay,
		)
		if _, ok := v.bookmarkedIDs[strings.TrimSpace(item.msg.ID)]; ok {
			head += " ★"
		}
		head = truncateVis(head, maxInt(0, width-3))

		body := firstNonEmptyLine(messageBodyString(item.msg.Body))
		if strings.TrimSpace(body) == "" {
			body = "(empty)"
		}
		bodyLine := "         " + truncateVis(body, maxInt(0, width-11))

		border := topicColors.Foreground(item.topicColorLookup).Render("▌")
		hline := border + " " + head
		bline := border + " " + bodyLine
		if i == v.selected {
			hline = selectedStyle.Render(truncateVis(hline, width))
			bline = selectedStyle.Render(truncateVis(bline, width))
		}
		lines = append(lines, truncateVis(hline, width))
		if len(lines) < height {
			lines = append(lines, truncateVis(bline, width))
		}
	}
	return clampRenderedLines(lines, height)
}

func (v *timelineView) renderSwimlane(width, height int, palette styles.Theme) []string {
	if height <= 0 {
		return nil
	}
	if len(v.visible) == 0 {
		return clampRenderedLines([]string{lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("no messages in selected window")}, height)
	}

	agents := v.activeAgentsInWindow()
	if len(agents) == 0 {
		return clampRenderedLines([]string{lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("no active agents")}, height)
	}

	if v.laneOffset >= len(agents) {
		v.laneOffset = maxInt(0, len(agents)-1)
	}
	lanes := agents[v.laneOffset:]
	if len(lanes) > timelineMaxLanes {
		lanes = lanes[:timelineMaxLanes]
	}

	lines := make([]string, 0, height+2)
	lines = append(lines, v.renderSwimlaneHeader(width, lanes))
	lines = append(lines, v.renderSwimlaneGuides(width, lanes))

	contentRows := height - len(lines)
	if contentRows <= 0 {
		return clampRenderedLines(lines, height)
	}

	v.ensureViewport(contentRows)
	start := clampInt(v.top, 0, len(v.visible)-1)
	for i := start; i < len(v.visible) && len(lines) < height; i++ {
		row := v.renderSwimlaneRow(width, lanes, v.visible[i], i == v.selected, palette)
		lines = append(lines, row)
	}
	return clampRenderedLines(lines, height)
}

func (v *timelineView) renderSwimlaneHeader(width int, lanes []string) string {
	if width <= 0 {
		return ""
	}
	row := make([]rune, width)
	for i := range row {
		row[i] = ' '
	}
	writeRunes(row, 0, "time  ")

	startX := 6
	laneW := maxInt(6, (width-startX)/maxInt(1, len(lanes)))
	for i := range lanes {
		center := laneCenter(i, startX, laneW)
		name := truncateVis(lanes[i], maxInt(4, laneW-1))
		writeCentered(row, center, name)
	}
	return string(row)
}

func (v *timelineView) renderSwimlaneGuides(width int, lanes []string) string {
	if width <= 0 {
		return ""
	}
	row := make([]rune, width)
	for i := range row {
		row[i] = ' '
	}
	startX := 6
	laneW := maxInt(6, (width-startX)/maxInt(1, len(lanes)))
	for i := range lanes {
		center := laneCenter(i, startX, laneW)
		if center >= 0 && center < len(row) {
			row[center] = '│'
		}
	}
	return string(row)
}

func (v *timelineView) renderSwimlaneRow(width int, lanes []string, item timelineItem, selected bool, palette styles.Theme) string {
	if width <= 0 {
		return ""
	}
	row := make([]rune, width)
	for i := range row {
		row[i] = ' '
	}
	writeRunes(row, 0, item.timestamp.Format("15:04 "))

	startX := 6
	laneW := maxInt(6, (width-startX)/maxInt(1, len(lanes)))
	lanePos := make(map[string]int, len(lanes))
	for i, name := range lanes {
		center := laneCenter(i, startX, laneW)
		lanePos[name] = center
		if center >= 0 && center < len(row) {
			row[center] = '│'
		}
	}

	sender := strings.TrimSpace(item.msg.From)
	fromX, hasSender := lanePos[sender]
	recipients := v.swimlaneRecipients(item.msg)
	rendered := false
	for _, recipient := range recipients {
		toX, ok := lanePos[recipient]
		if !ok || !hasSender {
			continue
		}
		rendered = true
		drawArrow(row, fromX, toX, strings.HasPrefix(strings.TrimSpace(item.msg.To), "@"))
	}
	if !rendered && hasSender {
		if fromX >= 0 && fromX < len(row) {
			row[fromX] = '•'
		}
	}

	label := timelineArrowLabel(item.msg.To)
	writeCentered(row, maxInt(startX, minInt(width-1, startX+(laneW*len(lanes))/2)), label)

	line := string(row)
	line = styles.NewAgentColorMapper().Foreground(item.topicColorLookup).Render(line)
	if selected {
		line = lipgloss.NewStyle().Background(lipgloss.Color(palette.Chrome.SelectedItem)).Render(truncateVis(line, width))
	}
	return truncateVis(line, width)
}

func (v *timelineView) ensureViewport(maxRows int) {
	if len(v.visible) == 0 {
		v.top = 0
		return
	}
	if maxRows <= 0 {
		maxRows = 1
	}
	if v.selected < v.top {
		v.top = v.selected
	}
	if v.selected >= v.top+maxRows {
		v.top = v.selected - maxRows + 1
	}
	v.top = clampInt(v.top, 0, maxInt(0, len(v.visible)-1))
}

func (v *timelineView) renderDetailOverlay(base string, width, height int, palette styles.Theme) string {
	msg, ok := v.selectedMessage()
	if !ok {
		return base
	}
	body := messageBodyString(msg.Body)
	if strings.TrimSpace(body) == "" {
		body = "(empty)"
	}
	lines := []string{
		lipgloss.NewStyle().Bold(true).Render("Message " + shortID(msg.ID)),
		fmt.Sprintf("From: %s", strings.TrimSpace(msg.From)),
		fmt.Sprintf("To: %s", strings.TrimSpace(msg.To)),
		fmt.Sprintf("Time: %s", messageTimestamp(msg).Format(time.RFC3339)),
	}
	if p := strings.TrimSpace(msg.Priority); p != "" {
		lines = append(lines, "Priority: "+p)
	}
	if r := strings.TrimSpace(msg.ReplyTo); r != "" {
		lines = append(lines, "Reply-To: "+r)
	}
	if len(msg.Tags) > 0 {
		lines = append(lines, "Tags: "+strings.Join(msg.Tags, ", "))
	}
	if _, ok := v.bookmarkedIDs[strings.TrimSpace(msg.ID)]; ok {
		lines = append(lines, "Bookmark: yes")
	}
	lines = append(lines, "", truncateVis(body, maxInt(20, width-14)), "", "Enter close  o open thread  b bookmark  r reply")
	panel := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(palette.Base.Border)).
		Background(lipgloss.Color(palette.Base.Background)).
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Padding(1, 2).
		Width(minInt(maxInt(50, width-10), 100))
	return lipgloss.Place(width, height, lipgloss.Center, lipgloss.Center, panel.Render(strings.Join(lines, "\n")))
}

func (v *timelineView) activeAgentsInWindow() []string {
	set := make(map[string]struct{}, 16)
	firstSeen := make([]string, 0, 16)
	for _, item := range v.visible {
		from := strings.TrimSpace(item.msg.From)
		if from != "" {
			if _, ok := set[from]; !ok {
				set[from] = struct{}{}
				firstSeen = append(firstSeen, from)
			}
		}
		for _, recipient := range v.swimlaneRecipients(item.msg) {
			if recipient == "" {
				continue
			}
			if _, ok := set[recipient]; ok {
				continue
			}
			set[recipient] = struct{}{}
			firstSeen = append(firstSeen, recipient)
		}
	}

	if v.agentSort == timelineOrderAlphabetical {
		out := append([]string(nil), firstSeen...)
		sort.Strings(out)
		return out
	}
	return firstSeen
}

func (v *timelineView) swimlaneRecipients(msg fmail.Message) []string {
	target := strings.TrimSpace(msg.To)
	if strings.HasPrefix(target, "@") {
		return []string{strings.TrimPrefix(target, "@")}
	}
	participants := append([]string(nil), v.topicParticipants[target]...)
	if len(participants) == 0 {
		return nil
	}
	sender := strings.TrimSpace(msg.From)
	out := make([]string, 0, len(participants))
	for _, p := range participants {
		p = strings.TrimSpace(p)
		if p == "" || strings.EqualFold(p, sender) {
			continue
		}
		out = append(out, p)
	}
	return out
}

func (f timelineFilter) matches(msg fmail.Message, now time.Time, repliedByParentID map[string]struct{}, bookmarks map[string]struct{}) bool {
	ts := messageTimestamp(msg)
	if f.Since > 0 && ts.Before(now.Add(-f.Since)) {
		return false
	}
	if f.Until > 0 && ts.After(now.Add(-f.Until)) {
		return false
	}
	if f.From != "" && !timelineMatch(f.From, msg.From) {
		return false
	}
	if f.To != "" && !timelineMatch(f.To, msg.To) {
		return false
	}
	if f.In != "" {
		to := strings.TrimSpace(msg.To)
		if strings.HasPrefix(to, "@") || !timelineMatch(f.In, to) {
			return false
		}
	}
	if f.Priority != "" && !strings.EqualFold(strings.TrimSpace(msg.Priority), f.Priority) {
		return false
	}
	if len(f.Tags) > 0 {
		if !hasAllTags(msg.Tags, f.Tags) {
			return false
		}
	}
	if f.Text != "" {
		if !strings.Contains(strings.ToLower(messageBodyString(msg.Body)), strings.ToLower(f.Text)) {
			return false
		}
	}
	if f.HasReply {
		if _, ok := repliedByParentID[strings.TrimSpace(msg.ID)]; !ok {
			return false
		}
	}
	if f.HasBookmark {
		if _, ok := bookmarks[strings.TrimSpace(msg.ID)]; !ok {
			return false
		}
	}
	return true
}

func parseTimelineFilter(input string) timelineFilter {
	input = strings.TrimSpace(input)
	if input == "" {
		return timelineFilter{}
	}
	out := timelineFilter{}
	textBits := make([]string, 0, 2)
	for _, token := range strings.Fields(input) {
		key, value, hasValue := strings.Cut(token, ":")
		if !hasValue {
			textBits = append(textBits, token)
			continue
		}
		key = strings.ToLower(strings.TrimSpace(key))
		value = strings.TrimSpace(value)
		switch key {
		case "from":
			out.From = value
		case "to":
			out.To = value
		case "in", "topic":
			out.In = value
		case "priority":
			out.Priority = strings.ToLower(value)
		case "tag":
			if value != "" {
				out.Tags = append(out.Tags, strings.ToLower(value))
			}
		case "text":
			if value != "" {
				textBits = append(textBits, value)
			}
		case "since":
			if d, ok := parseTimelineDuration(value); ok {
				out.Since = d
			}
		case "until":
			if d, ok := parseTimelineDuration(value); ok {
				out.Until = d
			}
		case "has":
			switch strings.ToLower(value) {
			case "reply":
				out.HasReply = true
			case "bookmark":
				out.HasBookmark = true
			}
		default:
			textBits = append(textBits, token)
		}
	}
	out.Text = strings.TrimSpace(strings.Join(textBits, " "))
	return out
}

func parseTimelineDuration(raw string) (time.Duration, bool) {
	raw = strings.TrimSpace(strings.ToLower(raw))
	if raw == "" {
		return 0, false
	}
	if dur, err := time.ParseDuration(raw); err == nil && dur > 0 {
		return dur, true
	}
	unit := raw[len(raw)-1]
	numStr := raw[:len(raw)-1]
	n, err := strconv.Atoi(numStr)
	if err != nil || n <= 0 {
		return 0, false
	}
	switch unit {
	case 'm':
		return time.Duration(n) * time.Minute, true
	case 'h':
		return time.Duration(n) * time.Hour, true
	case 'd':
		return time.Duration(n) * 24 * time.Hour, true
	case 'w':
		return time.Duration(n) * 7 * 24 * time.Hour, true
	default:
		return 0, false
	}
}

func parseTimelineJump(raw string, now time.Time) (time.Time, bool) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return time.Time{}, false
	}
	if strings.HasPrefix(raw, "-") || strings.HasPrefix(raw, "+") {
		if d, ok := parseTimelineDuration(strings.TrimPrefix(strings.TrimPrefix(raw, "-"), "+")); ok {
			if strings.HasPrefix(raw, "-") {
				return now.Add(-d), true
			}
			return now.Add(d), true
		}
	}

	layouts := []string{
		time.RFC3339,
		"2006-01-02 15:04:05",
		"2006-01-02 15:04",
		"2006-01-02",
		"15:04:05",
		"15:04",
	}
	for _, layout := range layouts {
		if parsed, err := time.ParseInLocation(layout, raw, time.UTC); err == nil {
			if layout == "15:04:05" || layout == "15:04" {
				return time.Date(now.Year(), now.Month(), now.Day(), parsed.Hour(), parsed.Minute(), parsed.Second(), 0, time.UTC), true
			}
			if layout == "2006-01-02" {
				return time.Date(parsed.Year(), parsed.Month(), parsed.Day(), now.Hour(), now.Minute(), now.Second(), 0, time.UTC), true
			}
			return parsed.UTC(), true
		}
	}
	return time.Time{}, false
}

func timelineMatch(pattern, value string) bool {
	pattern = strings.TrimSpace(pattern)
	value = strings.TrimSpace(value)
	if pattern == "" {
		return true
	}
	if strings.ContainsAny(pattern, "*?[]") {
		ok, err := filepath.Match(strings.ToLower(pattern), strings.ToLower(value))
		return err == nil && ok
	}
	return strings.EqualFold(pattern, value)
}

func timelineZoomLabel(window time.Duration) string {
	switch window {
	case time.Minute:
		return "1m"
	case 5 * time.Minute:
		return "5m"
	case 15 * time.Minute:
		return "15m"
	case time.Hour:
		return "1h"
	case 4 * time.Hour:
		return "4h"
	case 24 * time.Hour:
		return "1d"
	default:
		return window.String()
	}
}

func timelineGapLabel(gap time.Duration) string {
	if gap < time.Minute {
		return fmt.Sprintf("%ds", int(gap.Seconds()))
	}
	if gap < time.Hour {
		return fmt.Sprintf("%dm", int(gap.Minutes()))
	}
	return fmt.Sprintf("%dh%02dm", int(gap.Hours()), int(gap.Minutes())%60)
}

func timelineArrowLabel(target string) string {
	target = strings.TrimSpace(target)
	if strings.HasPrefix(target, "@") {
		return "DM"
	}
	return truncate(target, 8)
}

func timelineTargetLabel(target string) string {
	target = strings.TrimSpace(target)
	if target == "" {
		return "(unknown)"
	}
	return target
}

func timelineTopicColorKey(target string) string {
	target = strings.TrimSpace(target)
	if strings.HasPrefix(target, "@") {
		return "dm"
	}
	if target == "" {
		return "unknown"
	}
	return target
}

func timelineDedupKey(msg fmail.Message) string {
	return strings.TrimSpace(msg.ID) + "|" + strings.TrimSpace(msg.From) + "|" + strings.TrimSpace(msg.To)
}

func messageTimestamp(msg fmail.Message) time.Time {
	if !msg.Time.IsZero() {
		return msg.Time.UTC()
	}
	if len(msg.ID) >= len("20060102-150405") {
		if parsed, err := time.Parse("20060102-150405", msg.ID[:len("20060102-150405")]); err == nil {
			return parsed.UTC()
		}
	}
	return time.Time{}
}

func flattenParticipants(in map[string]map[string]struct{}) map[string][]string {
	out := make(map[string][]string, len(in))
	for topic, bucket := range in {
		names := make([]string, 0, len(bucket))
		for name := range bucket {
			name = strings.TrimSpace(name)
			if name == "" {
				continue
			}
			names = append(names, name)
		}
		sort.Strings(names)
		out[topic] = names
	}
	return out
}

func mergeTimelineMessages(existing []fmail.Message, incoming []fmail.Message) []fmail.Message {
	if len(incoming) == 0 {
		return existing
	}
	seen := make(map[string]struct{}, len(existing)+len(incoming))
	merged := make([]fmail.Message, 0, len(existing)+len(incoming))
	for i := range existing {
		msg := existing[i]
		key := timelineDedupKey(msg)
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		merged = append(merged, msg)
	}
	for i := range incoming {
		msg := incoming[i]
		key := timelineDedupKey(msg)
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		merged = append(merged, msg)
	}
	sortMessages(merged)
	return merged
}

func mergeTopicParticipants(left map[string][]string, right map[string][]string) map[string][]string {
	if len(left) == 0 && len(right) == 0 {
		return map[string][]string{}
	}
	out := make(map[string][]string, len(left)+len(right))
	for topic, participants := range left {
		out[topic] = append([]string(nil), participants...)
	}
	for topic, participants := range right {
		out[topic] = dedupeSorted(append(out[topic], participants...))
	}
	return out
}

func dedupeSorted(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	out := make([]string, 0, len(values))
	seen := make(map[string]struct{}, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		out = append(out, value)
	}
	sort.Strings(out)
	return out
}

func drawArrow(row []rune, fromX, toX int, dashed bool) {
	if len(row) == 0 {
		return
	}
	leftToRight := fromX < toX
	lineRune := '─'
	if dashed {
		lineRune = '┄'
	}
	if fromX == toX {
		if fromX >= 0 && fromX < len(row) {
			row[fromX] = '•'
		}
		return
	}
	if fromX > toX {
		fromX, toX = toX, fromX
	}
	fromX = clampInt(fromX, 0, len(row)-1)
	toX = clampInt(toX, 0, len(row)-1)
	for x := fromX + 1; x < toX; x++ {
		row[x] = lineRune
	}
	if leftToRight {
		row[toX] = '→'
	} else {
		row[toX] = '←'
	}
}

func laneCenter(index, startX, laneWidth int) int {
	return startX + (index * laneWidth) + laneWidth/2
}

func writeRunes(dst []rune, start int, value string) {
	if start >= len(dst) {
		return
	}
	for _, r := range value {
		if start >= len(dst) {
			break
		}
		dst[start] = r
		start++
	}
}

func writeCentered(dst []rune, center int, value string) {
	if len(dst) == 0 {
		return
	}
	trimmed := []rune(strings.TrimSpace(value))
	if len(trimmed) == 0 {
		return
	}
	start := center - len(trimmed)/2
	if start < 0 {
		start = 0
	}
	if start >= len(dst) {
		return
	}
	for _, r := range trimmed {
		if start >= len(dst) {
			break
		}
		dst[start] = r
		start++
	}
}

func trimLastRune(s string) string {
	if s == "" {
		return s
	}
	runes := []rune(s)
	return string(runes[:len(runes)-1])
}
