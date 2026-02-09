package fmailtui

import "strings"

func hasAllTags(actual []string, required []string) bool {
	if len(required) == 0 {
		return true
	}
	if len(actual) == 0 {
		return false
	}
	actualSet := make(map[string]struct{}, len(actual))
	for _, tag := range actual {
		tag = strings.TrimSpace(strings.ToLower(tag))
		if tag == "" {
			continue
		}
		actualSet[tag] = struct{}{}
	}
	for _, want := range required {
		want = strings.TrimSpace(strings.ToLower(want))
		if want == "" {
			continue
		}
		if _, ok := actualSet[want]; !ok {
			return false
		}
	}
	return true
}
