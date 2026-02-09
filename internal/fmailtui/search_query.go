package fmailtui

import (
	"strconv"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
)

type parsedSearch struct {
	Query data.SearchQuery
	Raw   string
}

func parseSearchInput(raw string, now time.Time) parsedSearch {
	in := strings.TrimSpace(raw)
	out := data.SearchQuery{}
	if in == "" {
		return parsedSearch{Query: out, Raw: raw}
	}

	tokens := strings.Fields(in)
	textTerms := make([]string, 0, 4)
	for _, tok := range tokens {
		if !strings.Contains(tok, ":") {
			textTerms = append(textTerms, tok)
			continue
		}
		key, val, _ := strings.Cut(tok, ":")
		key = strings.ToLower(strings.TrimSpace(key))
		val = strings.TrimSpace(val)
		switch key {
		case "from":
			out.From = val
		case "to":
			out.To = val
		case "in":
			out.In = val
		case "priority":
			out.Priority = normalizePriorityInput(val)
		case "tag":
			if val != "" {
				out.Tags = append(out.Tags, val)
			}
		case "since":
			if d, ok := parseHumanDuration(val); ok {
				out.Since = now.Add(-d)
			}
		case "until":
			if d, ok := parseHumanDuration(val); ok {
				out.Until = now.Add(-d)
			}
		case "has":
			switch strings.ToLower(val) {
			case "reply":
				out.HasReply = true
			case "bookmark":
				out.HasBookmark = true
			}
		case "is":
			if strings.EqualFold(val, "unread") {
				out.IsUnread = true
			}
		case "text":
			if val != "" {
				textTerms = append(textTerms, val)
			}
		default:
			if val != "" {
				textTerms = append(textTerms, val)
			}
		}
	}

	out.Text = strings.TrimSpace(strings.Join(textTerms, " "))
	if out.Priority == "" {
		out.Priority = ""
	}
	if len(out.Tags) > 0 {
		if normalized, err := fmail.NormalizeTags(out.Tags); err == nil {
			out.Tags = normalized
		}
	}
	return parsedSearch{Query: out, Raw: raw}
}

func parseHumanDuration(raw string) (time.Duration, bool) {
	s := strings.TrimSpace(raw)
	if s == "" {
		return 0, false
	}
	if strings.HasSuffix(s, "d") {
		n, err := strconv.Atoi(strings.TrimSuffix(s, "d"))
		if err != nil || n <= 0 {
			return 0, false
		}
		return time.Duration(n) * 24 * time.Hour, true
	}
	if strings.HasSuffix(s, "w") {
		n, err := strconv.Atoi(strings.TrimSuffix(s, "w"))
		if err != nil || n <= 0 {
			return 0, false
		}
		return time.Duration(n) * 7 * 24 * time.Hour, true
	}
	d, err := time.ParseDuration(s)
	if err != nil || d <= 0 {
		return 0, false
	}
	return d, true
}
