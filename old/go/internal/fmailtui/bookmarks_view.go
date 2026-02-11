package fmailtui

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	"github.com/tOgg1/forge/internal/fmailtui/state"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

type bookmarkSort int

const (
	bookmarkSortBookmarkedAt bookmarkSort = iota
	bookmarkSortMessageTime
	bookmarkSortTopic
	bookmarkSortAgent
)

type bookmarksView struct {
	root     string
	store    *fmail.Store
	provider data.MessageProvider
	state    *state.Manager

	bookmarks []state.Bookmark
	selected  int

	filterActive bool
	filterInput  string
	filterRaw    string

	editActive bool
	editInput  string

	sortMode bookmarkSort

	messageCache map[string]*fmail.Message

	statusLine string
	statusErr  bool
}

func newBookmarksView(root string, store *fmail.Store, provider data.MessageProvider, st *state.Manager) *bookmarksView {
	return &bookmarksView{
		root:         strings.TrimSpace(root),
		store:        store,
		provider:     provider,
		state:        st,
		sortMode:     bookmarkSortBookmarkedAt,
		messageCache: make(map[string]*fmail.Message, 128),
	}
}

func (v *bookmarksView) Init() tea.Cmd {
	v.reload()
	return nil
}

func (v *bookmarksView) Update(msg tea.Msg) tea.Cmd {
	switch typed := msg.(type) {
	case tea.KeyMsg:
		if v.editActive {
			return v.handleEditKey(typed)
		}
		if v.filterActive {
			return v.handleFilterKey(typed)
		}
		return v.handleKey(typed)
	}
	return nil
}

func (v *bookmarksView) MinSize() (int, int) {
	return 60, 14
}

func (v *bookmarksView) View(width, height int, theme Theme) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	palette := themePalette(theme)
	base := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Foreground)).Background(lipgloss.Color(palette.Base.Background))
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))
	accent := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true)
	errStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Bold(true)

	lines := make([]string, 0, height*2)
	lines = append(lines, accent.Render(fmt.Sprintf("Bookmarks (%d)", len(v.bookmarks))))
	lines = append(lines, muted.Render("Enter: open  e: edit note  d: delete  x: export  /: filter  s: sort  Esc: back"))

	filter := strings.TrimSpace(v.filterRaw)
	filterLine := "filter: " + filter
	if v.filterActive {
		filterLine = "filter> " + v.filterInput
	}
	lines = append(lines, muted.Render(truncateVis(filterLine, width)))

	bodyH := maxInt(0, height-len(lines)-2)
	lines = append(lines, v.renderList(width, bodyH, palette)...)

	if v.editActive {
		lines = append(lines, muted.Render(""))
		lines = append(lines, accent.Render("edit note (Enter save, Esc cancel)"))
		lines = append(lines, truncateVis("note> "+v.editInput, width))
	}

	if strings.TrimSpace(v.statusLine) != "" {
		if v.statusErr {
			lines = append(lines, errStyle.Render(truncateVis(v.statusLine, width)))
		} else {
			lines = append(lines, muted.Render(truncateVis(v.statusLine, width)))
		}
	}

	return base.Render(strings.Join(clampRenderedLines(lines, height), "\n"))
}

func (v *bookmarksView) handleKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "esc", "backspace":
		return popViewCmd()
	case "j", "down":
		v.selected = clampInt(v.selected+1, 0, maxInt(0, len(v.bookmarks)-1))
		return nil
	case "k", "up":
		v.selected = clampInt(v.selected-1, 0, maxInt(0, len(v.bookmarks)-1))
		return nil
	case "/":
		v.filterActive = true
		v.filterInput = v.filterRaw
		return nil
	case "s":
		v.sortMode = (v.sortMode + 1) % 4
		v.reload()
		return nil
	case "enter":
		if v.selected < 0 || v.selected >= len(v.bookmarks) {
			return nil
		}
		bm := v.bookmarks[v.selected]
		return tea.Batch(openThreadCmd(bm.Topic, bm.MessageID), pushViewCmd(ViewThread))
	case "e":
		if v.selected < 0 || v.selected >= len(v.bookmarks) {
			return nil
		}
		v.editActive = true
		v.editInput = strings.TrimSpace(v.bookmarks[v.selected].Note)
		return nil
	case "d":
		if v.selected < 0 || v.selected >= len(v.bookmarks) {
			return nil
		}
		if v.state == nil {
			return nil
		}
		id := strings.TrimSpace(v.bookmarks[v.selected].MessageID)
		if id == "" {
			return nil
		}
		_ = v.state.DeleteBookmark(id)
		v.state.SaveSoon()
		v.reload()
		v.statusLine = "deleted bookmark"
		v.statusErr = false
		return nil
	case "x":
		path, err := v.exportAll()
		if err != nil {
			v.statusLine = "export failed: " + err.Error()
			v.statusErr = true
			return nil
		}
		v.statusLine = "exported: " + path
		v.statusErr = false
		return nil
	}
	return nil
}

func (v *bookmarksView) handleFilterKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.filterActive = false
		return nil
	case tea.KeyEnter:
		v.filterRaw = strings.TrimSpace(v.filterInput)
		v.filterActive = false
		v.reload()
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

func (v *bookmarksView) handleEditKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.editActive = false
		v.editInput = ""
		return nil
	case tea.KeyEnter:
		if v.state == nil || v.selected < 0 || v.selected >= len(v.bookmarks) {
			v.editActive = false
			return nil
		}
		bm := v.bookmarks[v.selected]
		note := strings.TrimSpace(v.editInput)
		v.state.UpsertBookmark(strings.TrimSpace(bm.MessageID), strings.TrimSpace(bm.Topic), note)
		v.state.SaveSoon()
		v.editActive = false
		v.editInput = ""
		v.reload()
		v.statusLine = "note saved"
		v.statusErr = false
		return nil
	case tea.KeyBackspace:
		v.editInput = trimLastRune(v.editInput)
		return nil
	case tea.KeyRunes:
		v.editInput += string(msg.Runes)
		return nil
	}
	return nil
}

func (v *bookmarksView) reload() {
	v.bookmarks = nil
	if v.state != nil {
		v.bookmarks = v.state.Bookmarks()
	}
	v.applyFilter()
	v.sortBookmarks()
	v.selected = clampInt(v.selected, 0, maxInt(0, len(v.bookmarks)-1))
}

func (v *bookmarksView) applyFilter() {
	q := strings.ToLower(strings.TrimSpace(v.filterRaw))
	if q == "" || len(v.bookmarks) == 0 {
		return
	}
	filtered := make([]state.Bookmark, 0, len(v.bookmarks))
	for _, bm := range v.bookmarks {
		blob := strings.ToLower(strings.TrimSpace(bm.Note))
		if msg := v.cachedMessage(strings.TrimSpace(bm.MessageID), strings.TrimSpace(bm.Topic)); msg != nil {
			blob += "\n" + strings.ToLower(messageBodyString(msg.Body))
		}
		if strings.Contains(blob, q) {
			filtered = append(filtered, bm)
		}
	}
	v.bookmarks = filtered
}

func (v *bookmarksView) sortBookmarks() {
	if len(v.bookmarks) == 0 {
		return
	}
	mode := v.sortMode
	sort.SliceStable(v.bookmarks, func(i, j int) bool {
		a := v.bookmarks[i]
		b := v.bookmarks[j]
		switch mode {
		case bookmarkSortMessageTime:
			am := v.cachedMessage(a.MessageID, a.Topic)
			bm := v.cachedMessage(b.MessageID, b.Topic)
			at, bt := time.Time{}, time.Time{}
			if am != nil {
				at = am.Time
			}
			if bm != nil {
				bt = bm.Time
			}
			if !at.Equal(bt) {
				return at.After(bt)
			}
		case bookmarkSortTopic:
			if a.Topic != b.Topic {
				return a.Topic < b.Topic
			}
		case bookmarkSortAgent:
			am := v.cachedMessage(a.MessageID, a.Topic)
			bm := v.cachedMessage(b.MessageID, b.Topic)
			af, bf := "", ""
			if am != nil {
				af = strings.TrimSpace(am.From)
			}
			if bm != nil {
				bf = strings.TrimSpace(bm.From)
			}
			if af != bf {
				return af < bf
			}
		default:
			if !a.CreatedAt.Equal(b.CreatedAt) {
				return a.CreatedAt.After(b.CreatedAt)
			}
		}
		return a.MessageID > b.MessageID
	})
}

func (v *bookmarksView) renderList(width, height int, palette styles.Theme) []string {
	if height <= 0 {
		return nil
	}
	if len(v.bookmarks) == 0 {
		return []string{lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render("No bookmarks")}
	}
	mapper := styles.NewAgentColorMapperWithPalette(palette.AgentPalette)
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))

	lines := make([]string, 0, height*2)
	start := clampInt(v.selected-height/4, 0, maxInt(0, len(v.bookmarks)-1))
	for i := start; i < len(v.bookmarks) && len(lines) < height; i++ {
		bm := v.bookmarks[i]
		cursor := "  "
		if i == v.selected {
			cursor = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Chrome.SelectedItem)).Bold(true).Render("▸ ")
		}
		msg := v.cachedMessage(strings.TrimSpace(bm.MessageID), strings.TrimSpace(bm.Topic))
		from := "unknown"
		ts := "-"
		if msg != nil {
			from = mapper.Foreground(msg.From).Render(mapper.Plain(msg.From))
			if !msg.Time.IsZero() {
				ts = msg.Time.UTC().Format("15:04")
			}
		}

		note := strings.TrimSpace(bm.Note)
		title := note
		if title == "" && msg != nil {
			title = truncate(firstLine(msg.Body), maxInt(0, width-8))
		}
		if title == "" {
			title = "(no note)"
		}

		head := fmt.Sprintf("%s%s \u2014 %s in %s (%s)", cursor, truncate(title, maxInt(10, width-10)), from, bm.Topic, ts)
		lines = append(lines, truncateVis(head, width))
		if note != "" {
			lines = append(lines, muted.Render(truncateVis("  Note: "+note, width)))
		}
		if msg != nil {
			preview := truncate(firstLine(msg.Body), maxInt(0, width-4))
			if preview != "" {
				lines = append(lines, muted.Render(truncateVis("  "+preview, width)))
			}
		}
		lines = append(lines, "")
	}
	if len(lines) > 0 && lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	return clampRenderedLines(lines, height)
}

func (v *bookmarksView) cachedMessage(id, topic string) *fmail.Message {
	if id == "" || topic == "" {
		return nil
	}
	if msg, ok := v.messageCache[id]; ok {
		return msg
	}
	if v.store == nil {
		return nil
	}
	path := ""
	if strings.HasPrefix(topic, "@") {
		path = v.store.DMMessagePath(strings.TrimPrefix(topic, "@"), id)
	} else {
		path = v.store.TopicMessagePath(topic, id)
	}
	msg, err := v.store.ReadMessage(path)
	if err != nil || msg == nil {
		v.messageCache[id] = nil
		return nil
	}
	v.messageCache[id] = msg
	return msg
}

func (v *bookmarksView) exportAll() (string, error) {
	if v.store == nil {
		return "", fmt.Errorf("missing store")
	}
	bookmarks := v.bookmarks
	if v.state != nil {
		// Export all (ignore filter).
		bookmarks = v.state.Bookmarks()
	}
	if len(bookmarks) == 0 {
		return "", fmt.Errorf("no bookmarks")
	}
	now := time.Now().UTC()
	dir := filepath.Join(v.store.Root, "exports")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	path := filepath.Join(dir, fmt.Sprintf("bookmarks-%s.md", now.Format("20060102-150405")))
	if err := os.WriteFile(path, []byte(renderBookmarksMarkdown(now, bookmarks, v.cachedMessage)), 0o644); err != nil {
		return "", err
	}
	return path, nil
}

func renderBookmarksMarkdown(now time.Time, bookmarks []state.Bookmark, lookup func(id, topic string) *fmail.Message) string {
	var b strings.Builder
	b.WriteString("# Bookmarks - Exported ")
	b.WriteString(now.Format(time.RFC3339))
	b.WriteString("\n\n")
	for _, bm := range bookmarks {
		id := strings.TrimSpace(bm.MessageID)
		topic := strings.TrimSpace(bm.Topic)
		if id == "" || topic == "" {
			continue
		}
		msg := lookup(id, topic)
		title := strings.TrimSpace(bm.Note)
		if title == "" && msg != nil {
			title = firstLine(msg.Body)
		}
		if title == "" {
			title = id
		}
		b.WriteString("## ")
		b.WriteString(title)
		b.WriteString("\n")
		if msg != nil {
			b.WriteString("**From:** ")
			b.WriteString(strings.TrimSpace(msg.From))
			b.WriteString(" \u2192 ")
			b.WriteString(strings.TrimSpace(msg.To))
			if !msg.Time.IsZero() {
				b.WriteString(" | **Time:** ")
				b.WriteString(msg.Time.UTC().Format(time.RFC3339))
			}
			b.WriteString("\n")
		}
		if note := strings.TrimSpace(bm.Note); note != "" {
			b.WriteString("**Note:** ")
			b.WriteString(note)
			b.WriteString("\n")
		}
		b.WriteString("\n")
		if msg != nil {
			body := strings.TrimRight(messageBodyString(msg.Body), "\n")
			if body == "" {
				body = "(empty)"
			}
			for _, line := range strings.Split(body, "\n") {
				b.WriteString("> ")
				b.WriteString(line)
				b.WriteString("\n")
			}
			b.WriteString("\n")
		}
		b.WriteString("---\n\n")
	}
	return b.String()
}
