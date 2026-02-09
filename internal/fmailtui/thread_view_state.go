package fmailtui

import (
	"path/filepath"
	"strings"

	"github.com/tOgg1/forge/internal/fmail"
)

func (v *threadView) tuiStatePath() string {
	if strings.TrimSpace(v.root) == "" {
		return ""
	}
	return filepath.Join(v.root, ".fmail", "tui-state.json")
}

func (v *threadView) loadState() {
	path := v.tuiStatePath()
	if path == "" {
		return
	}
	state, err := loadTUIState(path)
	if err != nil {
		return
	}

	if v.readMarkers == nil {
		v.readMarkers = make(map[string]string)
	}
	for target, marker := range state.ReadMarkers {
		target = strings.TrimSpace(target)
		marker = strings.TrimSpace(marker)
		if target == "" || marker == "" {
			continue
		}
		if prev := strings.TrimSpace(v.readMarkers[target]); prev != "" && prev >= marker {
			continue
		}
		v.readMarkers[target] = marker
	}
}

func (v *threadView) persistReadMarker(target, marker string) {
	path := v.tuiStatePath()
	if path == "" {
		return
	}
	target = strings.TrimSpace(target)
	marker = strings.TrimSpace(marker)
	if target == "" || marker == "" {
		return
	}

	state, err := loadTUIState(path)
	if err != nil {
		return
	}
	if state.ReadMarkers == nil {
		state.ReadMarkers = make(map[string]string)
	}
	if prev := strings.TrimSpace(state.ReadMarkers[target]); prev != "" && prev >= marker {
		return
	}
	state.ReadMarkers[target] = marker
	_ = saveTUIState(path, state)
}

func (v *threadView) bodyTruncation(msg fmail.Message) (bool, int) {
	id := strings.TrimSpace(msg.ID)
	if id != "" && v.expandedBodies[id] {
		return false, 0
	}
	raw := strings.ReplaceAll(messageBodyString(msg.Body), "\r\n", "\n")
	count := len(strings.Split(raw, "\n"))
	if count > threadMaxBodyLines {
		return true, count - threadMaxBodyLines
	}
	return false, 0
}

func (v *threadView) selectedID() string {
	if v.selected < 0 || v.selected >= len(v.rows) {
		return ""
	}
	return strings.TrimSpace(v.rows[v.selected].msg.ID)
}

func (v *threadView) selectedRow() *threadRow {
	if v.selected < 0 || v.selected >= len(v.rows) {
		return nil
	}
	return &v.rows[v.selected]
}

func (v *threadView) indexForID(id string) int {
	if strings.TrimSpace(id) == "" {
		return -1
	}
	if idx, ok := v.rowIndexByID[id]; ok {
		return idx
	}
	return -1
}

func (v *threadView) isAtBottom() bool {
	if len(v.rows) == 0 {
		return true
	}
	return v.selected >= len(v.rows)-1
}

func (v *threadView) isUnread(id string) bool {
	marker := strings.TrimSpace(v.readMarkers[v.topic])
	if marker == "" {
		return false
	}
	return id > marker
}

func (v *threadView) advanceReadMarker() {
	if strings.TrimSpace(v.topic) == "" {
		return
	}
	id := v.selectedID()
	if id == "" {
		return
	}
	if prev := strings.TrimSpace(v.readMarkers[v.topic]); prev == "" || id > prev {
		v.readMarkers[v.topic] = id
		v.persistReadMarker(v.topic, id)
	}
}

func (v *threadView) topicIndex(topic string) int {
	for i := range v.topics {
		if v.topics[i].Name == topic {
			return i
		}
	}
	return -1
}

func (v *threadView) participantCount(topic string) int {
	if strings.HasPrefix(strings.TrimSpace(topic), "@") {
		return 2
	}
	for i := range v.topics {
		if v.topics[i].Name == topic {
			return len(v.topics[i].Participants)
		}
	}
	return 0
}
