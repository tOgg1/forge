package fmailtui

import (
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestReplayMessageTime_ParsesIDWhenTimeMissing(t *testing.T) {
	msg := fmail.Message{ID: "20260209-153001-0001", From: "a", To: "task", Body: "x"}
	got := replayMessageTime(msg)
	if got.IsZero() {
		t.Fatalf("got zero time")
	}
	if got.UTC().Format("20060102-150405") != "20260209-153001" {
		t.Fatalf("got %s", got.UTC().Format("20060102-150405"))
	}
}

func TestReplaySeekIndexBeforeOrAt(t *testing.T) {
	base := time.Date(2026, 2, 9, 15, 0, 0, 0, time.UTC)
	times := []time.Time{
		base.Add(0),
		base.Add(1 * time.Minute),
		base.Add(2 * time.Minute),
	}
	if got := replaySeekIndexBeforeOrAt(times, base.Add(90*time.Second)); got != 1 {
		t.Fatalf("got %d, want 1", got)
	}
	if got := replaySeekIndexBeforeOrAt(times, base.Add(-1*time.Minute)); got != 0 {
		t.Fatalf("got %d, want 0", got)
	}
	if got := replaySeekIndexBeforeOrAt(times, base.Add(10*time.Minute)); got != 2 {
		t.Fatalf("got %d, want 2", got)
	}
}

func TestReplayNextInterval_ClampsLargeGaps(t *testing.T) {
	curr := time.Date(2026, 2, 9, 15, 0, 0, 0, time.UTC)
	next := curr.Add(10 * time.Minute)
	got := replayNextInterval(curr, next, 1)
	if got > 200*time.Millisecond {
		t.Fatalf("got %s, want <= 200ms", got)
	}
}
