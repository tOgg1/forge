package data

import (
	"fmt"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

func normalizeSendRequest(req SendRequest, fallbackAgent string) (fmail.Message, error) {
	from := strings.TrimSpace(req.From)
	if from == "" {
		from = strings.TrimSpace(fallbackAgent)
	}
	if from == "" {
		from = defaultForgedAgent
	}
	normalizedFrom, err := fmail.NormalizeAgentName(from)
	if err != nil {
		return fmail.Message{}, err
	}

	to := strings.TrimSpace(req.To)
	if to == "" {
		return fmail.Message{}, fmt.Errorf("missing target")
	}
	if _, _, err := fmail.NormalizeTarget(to); err != nil {
		return fmail.Message{}, err
	}

	body := strings.TrimSpace(req.Body)
	if body == "" {
		return fmail.Message{}, fmt.Errorf("missing body")
	}

	priority := strings.TrimSpace(strings.ToLower(req.Priority))
	if priority == "" {
		priority = fmail.PriorityNormal
	}
	if err := fmail.ValidatePriority(priority); err != nil {
		return fmail.Message{}, err
	}

	tags := normalizeSendTags(req.Tags)
	if len(tags) > 0 {
		if err := fmail.ValidateTags(tags); err != nil {
			return fmail.Message{}, err
		}
	}

	msgTime := req.Time
	if msgTime.IsZero() {
		msgTime = time.Now().UTC()
	}

	return fmail.Message{
		From:     normalizedFrom,
		To:       to,
		Time:     msgTime,
		Body:     body,
		ReplyTo:  strings.TrimSpace(req.ReplyTo),
		Priority: priority,
		Tags:     tags,
	}, nil
}

func normalizeSendTags(tags []string) []string {
	if len(tags) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(tags))
	out := make([]string, 0, len(tags))
	for _, tag := range tags {
		tag = strings.TrimSpace(strings.ToLower(tag))
		if tag == "" {
			continue
		}
		if _, ok := seen[tag]; ok {
			continue
		}
		seen[tag] = struct{}{}
		out = append(out, tag)
	}
	return out
}
