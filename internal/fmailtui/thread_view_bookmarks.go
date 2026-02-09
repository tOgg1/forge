package fmailtui

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

func (v *threadView) toggleBookmark() {
	if v == nil || v.state == nil {
		return
	}
	id := v.selectedID()
	if id == "" {
		return
	}
	topic := strings.TrimSpace(v.topic)
	if topic == "" {
		return
	}

	if v.bookmarkedIDs == nil {
		v.bookmarkedIDs = make(map[string]bool)
	}

	if v.bookmarkedIDs[id] {
		if v.bookmarkConfirmID != id {
			v.bookmarkConfirmID = id
			v.statusLine = "press b again to remove bookmark"
			v.statusErr = false
			return
		}
		v.state.ToggleBookmark(id, topic)
		v.state.SaveSoon()
		delete(v.bookmarkedIDs, id)
		v.bookmarkConfirmID = ""
		v.statusLine = "bookmark removed"
		v.statusErr = false
		return
	}

	v.state.ToggleBookmark(id, topic)
	v.state.SaveSoon()
	v.bookmarkedIDs[id] = true
	v.bookmarkConfirmID = ""
	v.statusLine = "bookmarked"
	v.statusErr = false
}

func (v *threadView) openBookmarkNoteEditor() {
	if v == nil || v.state == nil {
		return
	}
	id := v.selectedID()
	if id == "" || strings.TrimSpace(v.topic) == "" {
		return
	}
	v.editActive = true
	v.editKind = "bookmark-note"
	v.editTargetID = id
	v.editInput = v.state.BookmarkNote(id)
	v.statusLine = ""
	v.statusErr = false
}

func (v *threadView) openAnnotationEditor() {
	if v == nil || v.state == nil {
		return
	}
	id := v.selectedID()
	if id == "" {
		return
	}
	v.editActive = true
	v.editKind = "annotation"
	v.editTargetID = id
	v.editInput = v.state.Annotation(id)
	v.statusLine = ""
	v.statusErr = false
}

func (v *threadView) handleEditKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.Type {
	case tea.KeyEsc:
		v.editActive = false
		v.editKind = ""
		v.editTargetID = ""
		v.editInput = ""
		return nil
	case tea.KeyEnter:
		v.saveEdit()
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

func (v *threadView) saveEdit() {
	if v == nil || v.state == nil {
		return
	}
	id := strings.TrimSpace(v.editTargetID)
	kind := v.editKind
	input := strings.TrimSpace(v.editInput)

	v.editActive = false
	v.editKind = ""
	v.editTargetID = ""
	v.editInput = ""

	if id == "" {
		return
	}

	switch kind {
	case "bookmark-note":
		topic := strings.TrimSpace(v.topic)
		if topic == "" {
			return
		}
		v.state.UpsertBookmark(id, topic, input)
		v.state.SaveSoon()
		if v.bookmarkedIDs == nil {
			v.bookmarkedIDs = make(map[string]bool)
		}
		v.bookmarkedIDs[id] = true
		v.statusLine = "bookmark updated"
		v.statusErr = false
	case "annotation":
		v.state.SetAnnotation(id, input)
		v.state.SaveSoon()
		if v.annotations == nil {
			v.annotations = make(map[string]string)
		}
		if input == "" {
			delete(v.annotations, id)
			v.statusLine = "annotation cleared"
		} else {
			v.annotations[id] = input
			v.statusLine = "annotation saved"
		}
		v.statusErr = false
	}
}

func (v *threadView) renderEditPrompt(width int, palette styles.Theme) string {
	if width <= 0 {
		return ""
	}
	title := "edit"
	prompt := "input"
	switch v.editKind {
	case "bookmark-note":
		title = "Bookmark note"
		prompt = "note"
	case "annotation":
		title = "Annotation"
		prompt = "note"
	}

	accent := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Accent)).Bold(true)
	muted := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted))

	lines := []string{
		accent.Render(title) + muted.Render(" (Enter save, Esc cancel)"),
		truncateVis(prompt+"> "+v.editInput, width),
		"",
		"",
	}
	return strings.Join(lines[:4], "\n")
}

func (v *threadView) exportThreadCmd() tea.Cmd {
	topic := strings.TrimSpace(v.topic)
	msgs := append([]fmail.Message(nil), v.allMsgs...)
	root := strings.TrimSpace(v.root)
	return func() tea.Msg {
		if topic == "" || len(msgs) == 0 || root == "" {
			return threadExportResultMsg{err: fmt.Errorf("nothing to export")}
		}
		store, err := fmail.NewStore(root)
		if err != nil {
			return threadExportResultMsg{err: err}
		}
		dir := filepath.Join(store.Root, "exports")
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return threadExportResultMsg{err: err}
		}
		now := time.Now().UTC()
		name := fmt.Sprintf("thread-%s-%s.md", sanitizeFilenamePart(topic), now.Format("20060102-150405"))
		path := filepath.Join(dir, name)
		if err := os.WriteFile(path, []byte(renderThreadMarkdown(topic, now, msgs)), 0o644); err != nil {
			return threadExportResultMsg{err: err}
		}
		return threadExportResultMsg{path: path}
	}
}

func renderThreadMarkdown(topic string, now time.Time, msgs []fmail.Message) string {
	var b strings.Builder
	b.WriteString("# Thread - Exported ")
	b.WriteString(now.Format(time.RFC3339))
	b.WriteString("\n\n")
	b.WriteString("Topic: ")
	b.WriteString(topic)
	b.WriteString("\n\n")
	sortMessages(msgs)
	for _, msg := range msgs {
		if strings.TrimSpace(msg.ID) == "" {
			continue
		}
		b.WriteString("## ")
		b.WriteString(msg.ID)
		b.WriteString("\n")
		b.WriteString("**From:** ")
		b.WriteString(strings.TrimSpace(msg.From))
		b.WriteString(" \u2192 ")
		b.WriteString(strings.TrimSpace(msg.To))
		if !msg.Time.IsZero() {
			b.WriteString(" | **Time:** ")
			b.WriteString(msg.Time.UTC().Format(time.RFC3339))
		}
		b.WriteString("\n\n")
		body := strings.TrimRight(messageBodyString(msg.Body), "\n")
		if body == "" {
			body = "(empty)"
		}
		for _, line := range strings.Split(body, "\n") {
			b.WriteString("> ")
			b.WriteString(line)
			b.WriteString("\n")
		}
		b.WriteString("\n---\n\n")
	}
	return b.String()
}

func sanitizeFilenamePart(s string) string {
	s = strings.TrimSpace(s)
	if s == "" {
		return "untitled"
	}
	var b strings.Builder
	for _, r := range s {
		switch {
		case r >= 'a' && r <= 'z':
			b.WriteRune(r)
		case r >= 'A' && r <= 'Z':
			b.WriteRune(r + ('a' - 'A'))
		case r >= '0' && r <= '9':
			b.WriteRune(r)
		default:
			b.WriteByte('-')
		}
	}
	out := strings.Trim(b.String(), "-")
	out = strings.ReplaceAll(out, "--", "-")
	if out == "" {
		return "untitled"
	}
	return out
}
