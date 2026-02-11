package fmailtui

import (
	"sort"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
)

func sortTopicsByActivity(topics []data.TopicInfo) []data.TopicInfo {
	out := append([]data.TopicInfo(nil), topics...)
	sort.SliceStable(out, func(i, j int) bool {
		if !out[i].LastActivity.Equal(out[j].LastActivity) {
			return out[i].LastActivity.After(out[j].LastActivity)
		}
		return out[i].Name < out[j].Name
	})
	return out
}

func topicExists(topics []data.TopicInfo, topic string) bool {
	for i := range topics {
		if topics[i].Name == topic {
			return true
		}
	}
	return false
}

func sortMessages(msgs []fmail.Message) {
	sort.SliceStable(msgs, func(i, j int) bool {
		if msgs[i].ID != msgs[j].ID {
			return msgs[i].ID < msgs[j].ID
		}
		if !msgs[i].Time.Equal(msgs[j].Time) {
			return msgs[i].Time.Before(msgs[j].Time)
		}
		return msgs[i].From < msgs[j].From
	})
}

func countNewerMessages(messages []fmail.Message, marker string) int {
	if marker == "" {
		return 0
	}
	count := 0
	for i := range messages {
		if messages[i].ID > marker {
			count++
		}
	}
	return count
}
