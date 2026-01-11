package fmail

import (
	"fmt"
	"regexp"
	"strings"
)

const (
	MaxTagsPerMessage = 10
	MaxTagLength      = 50
)

var namePattern = regexp.MustCompile(`^[a-z0-9-]+$`)

// NormalizeTopic lowercases and validates a topic name.
func NormalizeTopic(topic string) (string, error) {
	normalized := strings.ToLower(strings.TrimSpace(topic))
	if normalized == "" || !namePattern.MatchString(normalized) {
		return "", ErrInvalidTopic
	}
	return normalized, nil
}

// ValidateTopic enforces topic naming rules without modification.
func ValidateTopic(topic string) error {
	value := strings.TrimSpace(topic)
	if value == "" || value != strings.ToLower(value) || !namePattern.MatchString(value) {
		return ErrInvalidTopic
	}
	return nil
}

// NormalizeAgentName lowercases and validates an agent name.
func NormalizeAgentName(name string) (string, error) {
	normalized := strings.ToLower(strings.TrimSpace(name))
	if normalized == "" || !namePattern.MatchString(normalized) {
		return "", ErrInvalidAgent
	}
	return normalized, nil
}

// ValidateAgentName enforces agent naming rules without modification.
func ValidateAgentName(name string) error {
	value := strings.TrimSpace(name)
	if value == "" || value != strings.ToLower(value) || !namePattern.MatchString(value) {
		return ErrInvalidAgent
	}
	return nil
}

// NormalizeTarget returns the normalized target and whether it is a DM.
func NormalizeTarget(target string) (string, bool, error) {
	raw := strings.TrimSpace(target)
	if raw == "" {
		return "", false, ErrInvalidTarget
	}
	if strings.HasPrefix(raw, "@") {
		agent, err := NormalizeAgentName(strings.TrimPrefix(raw, "@"))
		if err != nil {
			return "", false, err
		}
		return "@" + agent, true, nil
	}
	topic, err := NormalizeTopic(raw)
	if err != nil {
		return "", false, err
	}
	return topic, false, nil
}

// ValidateTarget checks whether a target is a topic or direct message.
func ValidateTarget(target string) error {
	raw := strings.TrimSpace(target)
	if raw == "" {
		return ErrInvalidTarget
	}
	if strings.HasPrefix(raw, "@") {
		return ValidateAgentName(strings.TrimPrefix(raw, "@"))
	}
	if err := ValidateTopic(raw); err != nil {
		return fmt.Errorf("%w: %s", ErrInvalidTarget, raw)
	}
	return nil
}

// ValidateTag checks a single tag against naming rules.
func ValidateTag(tag string) error {
	if tag == "" {
		return ErrInvalidTag
	}
	if len(tag) > MaxTagLength {
		return fmt.Errorf("%w: exceeds %d chars", ErrInvalidTag, MaxTagLength)
	}
	if !namePattern.MatchString(tag) {
		return fmt.Errorf("%w: %s", ErrInvalidTag, tag)
	}
	return nil
}

// ValidateTags checks all tags for a message.
func ValidateTags(tags []string) error {
	if len(tags) > MaxTagsPerMessage {
		return fmt.Errorf("%w: max %d tags", ErrInvalidTag, MaxTagsPerMessage)
	}
	for _, tag := range tags {
		if err := ValidateTag(tag); err != nil {
			return err
		}
	}
	return nil
}

// NormalizeTags lowercases and validates tags. Returns deduplicated slice.
func NormalizeTags(tags []string) ([]string, error) {
	if len(tags) == 0 {
		return nil, nil
	}
	seen := make(map[string]bool)
	result := make([]string, 0, len(tags))
	for _, tag := range tags {
		normalized := strings.ToLower(strings.TrimSpace(tag))
		if normalized == "" {
			continue
		}
		if err := ValidateTag(normalized); err != nil {
			return nil, err
		}
		if !seen[normalized] {
			seen[normalized] = true
			result = append(result, normalized)
		}
	}
	if len(result) > MaxTagsPerMessage {
		return nil, fmt.Errorf("%w: max %d tags", ErrInvalidTag, MaxTagsPerMessage)
	}
	return result, nil
}
