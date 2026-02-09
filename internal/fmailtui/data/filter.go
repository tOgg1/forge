package data

import (
	"encoding/json"
	"slices"
	"sort"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

func applyMessageFilter(messages []fmail.Message, opts MessageFilter) []fmail.Message {
	if len(messages) == 0 {
		return nil
	}

	filtered := make([]fmail.Message, 0, len(messages))
	for i := range messages {
		msg := messages[i]
		if !messageMatchesFilter(&msg, opts) {
			continue
		}
		filtered = append(filtered, cloneMessage(msg))
	}

	if opts.Limit > 0 && len(filtered) > opts.Limit {
		filtered = filtered[len(filtered)-opts.Limit:]
	}
	return filtered
}

func messageMatchesFilter(message *fmail.Message, opts MessageFilter) bool {
	if message == nil {
		return false
	}
	if !opts.Since.IsZero() && message.Time.Before(opts.Since) {
		return false
	}
	if !opts.Until.IsZero() && message.Time.After(opts.Until) {
		return false
	}
	if from := strings.TrimSpace(opts.From); from != "" && !strings.EqualFold(message.From, from) {
		return false
	}
	if priority := strings.TrimSpace(opts.Priority); priority != "" && !strings.EqualFold(message.Priority, priority) {
		return false
	}
	if to := strings.TrimSpace(opts.To); to != "" && !strings.EqualFold(message.To, to) {
		return false
	}
	if len(opts.Tags) > 0 && !containsAllTags(message.Tags, opts.Tags) {
		return false
	}
	return true
}

func searchMatches(message *fmail.Message, query SearchQuery) (bool, int, int) {
	if message == nil {
		return false, -1, 0
	}
	if !query.Since.IsZero() && message.Time.Before(query.Since) {
		return false, -1, 0
	}
	if !query.Until.IsZero() && message.Time.After(query.Until) {
		return false, -1, 0
	}
	if from := strings.TrimSpace(query.From); from != "" && !strings.EqualFold(message.From, from) {
		return false, -1, 0
	}
	if to := strings.TrimSpace(query.To); to != "" && !strings.EqualFold(message.To, to) {
		return false, -1, 0
	}
	if priority := strings.TrimSpace(query.Priority); priority != "" && !strings.EqualFold(message.Priority, priority) {
		return false, -1, 0
	}
	if len(query.Tags) > 0 && !containsAllTags(message.Tags, query.Tags) {
		return false, -1, 0
	}

	text := strings.TrimSpace(query.Text)
	if text == "" {
		return true, -1, 0
	}

	body := messageBodyString(*message)
	bodyLower := strings.ToLower(body)
	textLower := strings.ToLower(text)
	idx := strings.Index(bodyLower, textLower)
	if idx < 0 {
		return false, -1, 0
	}
	return true, idx, len(text)
}

func containsAllTags(actual []string, required []string) bool {
	if len(required) == 0 {
		return true
	}
	if len(actual) == 0 {
		return false
	}
	actualSet := make(map[string]struct{}, len(actual))
	for _, tag := range actual {
		trimmed := strings.TrimSpace(strings.ToLower(tag))
		if trimmed == "" {
			continue
		}
		actualSet[trimmed] = struct{}{}
	}
	for _, tag := range required {
		trimmed := strings.TrimSpace(strings.ToLower(tag))
		if trimmed == "" {
			continue
		}
		if _, ok := actualSet[trimmed]; !ok {
			return false
		}
	}
	return true
}

func messageBodyString(message fmail.Message) string {
	switch value := message.Body.(type) {
	case string:
		return value
	case json.RawMessage:
		return string(value)
	default:
		data, err := json.Marshal(value)
		if err != nil {
			return ""
		}
		return string(data)
	}
}

func cloneMessage(message fmail.Message) fmail.Message {
	cloned := message
	if len(message.Tags) > 0 {
		cloned.Tags = append([]string(nil), message.Tags...)
	}
	return cloned
}

func cloneMessages(messages []fmail.Message) []fmail.Message {
	if len(messages) == 0 {
		return nil
	}
	cloned := make([]fmail.Message, len(messages))
	for i := range messages {
		cloned[i] = cloneMessage(messages[i])
	}
	return cloned
}

func cloneTopics(topics []TopicInfo) []TopicInfo {
	if len(topics) == 0 {
		return nil
	}
	cloned := make([]TopicInfo, len(topics))
	for i := range topics {
		cloned[i] = topics[i]
		if len(topics[i].Participants) > 0 {
			cloned[i].Participants = append([]string(nil), topics[i].Participants...)
		}
		if topics[i].LastMessage != nil {
			msg := cloneMessage(*topics[i].LastMessage)
			cloned[i].LastMessage = &msg
		}
	}
	return cloned
}

func cloneAgentRecords(records []fmail.AgentRecord) []fmail.AgentRecord {
	if len(records) == 0 {
		return nil
	}
	cloned := make([]fmail.AgentRecord, len(records))
	copy(cloned, records)
	return cloned
}

func sortMessagesByID(messages []fmail.Message) {
	sort.SliceStable(messages, func(i, j int) bool {
		if messages[i].ID != messages[j].ID {
			return messages[i].ID < messages[j].ID
		}
		if !messages[i].Time.Equal(messages[j].Time) {
			return messages[i].Time.Before(messages[j].Time)
		}
		return messages[i].From < messages[j].From
	})
}

func latestActivity(message fmail.Message) time.Time {
	if !message.Time.IsZero() {
		return message.Time.UTC()
	}
	if len(message.ID) >= len("20060102-150405") {
		prefix := message.ID[:len("20060102-150405")]
		if parsed, err := time.Parse("20060102-150405", prefix); err == nil {
			return parsed.UTC()
		}
	}
	return time.Time{}
}

func participantsFromMessages(messages []fmail.Message) []string {
	if len(messages) == 0 {
		return nil
	}
	set := make(map[string]struct{}, len(messages))
	for i := range messages {
		if name := strings.TrimSpace(messages[i].From); name != "" {
			set[name] = struct{}{}
		}
	}
	participants := make([]string, 0, len(set))
	for name := range set {
		participants = append(participants, name)
	}
	slices.Sort(participants)
	return participants
}

func idLowerBound(since time.Time) string {
	if since.IsZero() {
		return ""
	}
	return since.UTC().Format("20060102-150405")
}

func idUpperBound(until time.Time) string {
	if until.IsZero() {
		return ""
	}
	return until.UTC().Format("20060102-150405") + "-9999"
}
