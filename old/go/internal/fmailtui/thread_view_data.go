package fmailtui

import (
	"fmt"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
)

func (v *threadView) applyLoaded(msg threadLoadedMsg) {
	v.now = msg.now
	v.lastErr = msg.err
	if msg.err != nil {
		return
	}

	prevTopic := v.topic
	prevAnchor := v.selectedID()
	wasAtBottom := v.isAtBottom()
	prevNewest := v.newestID
	prevTotal := v.total

	v.topics = sortTopicsByActivity(msg.topics)
	if strings.TrimSpace(msg.topic) != "" {
		v.topic = msg.topic
	} else if v.topic == "" && len(v.topics) > 0 {
		v.topic = v.topics[0].Name
	}

	v.allMsgs = append([]fmail.Message(nil), msg.msgs...)
	if !v.initialized || v.topic != prevTopic {
		v.limit = maxInt(threadPageSize, v.limit)
		v.total = msg.total
		v.pendingNew = 0
		v.initialized = true
		prevAnchor = ""
	}
	v.total = msg.total

	if len(v.allMsgs) > 0 {
		v.newestID = v.allMsgs[len(v.allMsgs)-1].ID
	} else {
		v.newestID = ""
	}

	// Local-only init: on first entry/topic switch, treat current tail as read.
	if strings.TrimSpace(v.topic) != "" && strings.TrimSpace(v.readMarkers[v.topic]) == "" && v.newestID != "" {
		v.readMarkers[v.topic] = v.newestID
		v.persistReadMarker(v.topic, v.newestID)
	}

	if prevTopic == v.topic && !wasAtBottom {
		if v.total > 0 && prevTotal > 0 && v.total > prevTotal {
			v.pendingNew += v.total - prevTotal
		} else if prevNewest != "" && v.newestID != "" && v.newestID > prevNewest {
			// Fallback for providers that don't populate total reliably.
			v.pendingNew += countNewerMessages(v.allMsgs, prevNewest)
		}
	}

	preferBottom := wasAtBottom && prevTopic == v.topic
	v.rebuildRows(prevAnchor, preferBottom)
	if preferBottom {
		v.pendingNew = 0
	}
	v.ensureVisible()
	v.advanceReadMarker()
}

func (v *threadView) loadCmd() tea.Cmd {
	if v.provider == nil {
		return func() tea.Msg {
			return threadLoadedMsg{now: time.Now().UTC(), err: fmt.Errorf("missing provider")}
		}
	}
	currentTopic := strings.TrimSpace(v.topic)
	limit := v.limit
	if limit <= 0 {
		limit = threadPageSize
	}
	return func() tea.Msg {
		now := time.Now().UTC()
		topics, err := v.provider.Topics()
		if err != nil {
			return threadLoadedMsg{now: now, err: err}
		}
		sortedTopics := sortTopicsByActivity(topics)

		topic := currentTopic
		if topic == "" && len(sortedTopics) > 0 {
			topic = sortedTopics[0].Name
		}
		if topic != "" && !strings.HasPrefix(topic, "@") && !topicExists(sortedTopics, topic) && len(sortedTopics) > 0 {
			topic = sortedTopics[0].Name
		}

		msgs := []fmail.Message{}
		total := 0
		if topic != "" {
			if strings.HasPrefix(topic, "@") {
				msgs, err = v.provider.DMs(strings.TrimPrefix(topic, "@"), data.MessageFilter{Limit: limit})
				if err != nil {
					return threadLoadedMsg{now: now, topics: sortedTopics, topic: topic, err: err}
				}
				total = len(msgs)
			} else {
				for i := range sortedTopics {
					if sortedTopics[i].Name == topic {
						total = sortedTopics[i].MessageCount
						break
					}
				}
				opts := data.MessageFilter{}
				if total > 1000 {
					opts.Limit = limit
				}
				msgs, err = v.provider.Messages(topic, opts)
				if err != nil {
					return threadLoadedMsg{now: now, topics: sortedTopics, topic: topic, err: err}
				}
			}
		}

		return threadLoadedMsg{now: now, topics: sortedTopics, topic: topic, msgs: msgs, total: total}
	}
}
