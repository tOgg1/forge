package fmailtui

import (
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestParseHumanDuration(t *testing.T) {
	t.Parallel()
	cases := []struct {
		in   string
		want time.Duration
		ok   bool
	}{
		{"", 0, false},
		{"0m", 0, false},
		{"15m", 15 * time.Minute, true},
		{"1h", 1 * time.Hour, true},
		{"2d", 48 * time.Hour, true},
		{"1w", 7 * 24 * time.Hour, true},
	}
	for _, tc := range cases {
		got, ok := parseHumanDuration(tc.in)
		if ok != tc.ok {
			t.Fatalf("parseHumanDuration(%q) ok=%v want %v", tc.in, ok, tc.ok)
		}
		if ok && got != tc.want {
			t.Fatalf("parseHumanDuration(%q)=%s want %s", tc.in, got, tc.want)
		}
	}
}

func TestParseSearchInput(t *testing.T) {
	t.Parallel()
	now := time.Date(2026, 2, 9, 10, 0, 0, 0, time.UTC)

	parsed := parseSearchInput("from:coder-* tag:AUTH since:1h has:reply has:bookmark is:unread token refresh", now)
	q := parsed.Query
	if q.From != "coder-*" {
		t.Fatalf("From=%q", q.From)
	}
	if len(q.Tags) != 1 || q.Tags[0] != "auth" {
		t.Fatalf("Tags=%v", q.Tags)
	}
	if q.Text != "token refresh" {
		t.Fatalf("Text=%q", q.Text)
	}
	if q.Since.IsZero() || !q.Since.Equal(now.Add(-1*time.Hour)) {
		t.Fatalf("Since=%s", q.Since)
	}
	if !q.HasReply || !q.HasBookmark || !q.IsUnread {
		t.Fatalf("flags: hasReply=%v hasBookmark=%v isUnread=%v", q.HasReply, q.HasBookmark, q.IsUnread)
	}

	// Priority normalization.
	parsed = parseSearchInput("priority:HIGH", now)
	if parsed.Query.Priority != fmail.PriorityHigh {
		t.Fatalf("Priority=%q", parsed.Query.Priority)
	}
}
