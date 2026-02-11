package threading

import (
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestBuildThreads_BasicChain(t *testing.T) {
	base := time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC)
	msgs := []fmail.Message{
		{ID: "20260209-080000-0001", From: "alice", To: "task", Time: base, Body: "root"},
		{ID: "20260209-080001-0001", From: "bob", To: "task", Time: base.Add(1 * time.Second), Body: "r1", ReplyTo: "20260209-080000-0001"},
		{ID: "20260209-080002-0001", From: "alice", To: "task", Time: base.Add(2 * time.Second), Body: "r2", ReplyTo: "20260209-080001-0001"},
	}

	threads := BuildThreads(msgs)
	if len(threads) != 1 {
		t.Fatalf("expected 1 thread, got %d", len(threads))
	}
	th := threads[0]
	if th.Root == nil || th.Root.ID != "20260209-080000-0001" {
		t.Fatalf("unexpected root: %#v", th.Root)
	}
	if th.Depth != 2 {
		t.Fatalf("expected depth 2, got %d", th.Depth)
	}
	if len(th.Messages) != 3 {
		t.Fatalf("expected 3 nodes, got %d", len(th.Messages))
	}

	flat := FlattenThread(th)
	if len(flat) != 3 {
		t.Fatalf("expected flat 3, got %d", len(flat))
	}
	if flat[0].Message.ID != "20260209-080000-0001" || flat[1].Message.ID != "20260209-080001-0001" || flat[2].Message.ID != "20260209-080002-0001" {
		t.Fatalf("unexpected flatten order: %s %s %s", flat[0].Message.ID, flat[1].Message.ID, flat[2].Message.ID)
	}
}

func TestBuildThreads_MissingParentBecomesRoot(t *testing.T) {
	msgs := []fmail.Message{
		{ID: "20260209-080000-0001", From: "alice", To: "task", Time: time.Now().UTC(), Body: "orphan", ReplyTo: "missing"},
	}
	threads := BuildThreads(msgs)
	if len(threads) != 1 {
		t.Fatalf("expected 1 thread, got %d", len(threads))
	}
	if threads[0].Root == nil || threads[0].Root.ID != msgs[0].ID {
		t.Fatalf("unexpected root: %#v", threads[0].Root)
	}
}

func TestBuildThreads_CycleBreaksDeterministically(t *testing.T) {
	base := time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC)
	// A replies to B, B replies to A -> cycle.
	msgs := []fmail.Message{
		{ID: "20260209-080000-0001", From: "alice", To: "task", Time: base, Body: "a", ReplyTo: "20260209-080001-0001"},
		{ID: "20260209-080001-0001", From: "bob", To: "task", Time: base.Add(1 * time.Second), Body: "b", ReplyTo: "20260209-080000-0001"},
	}

	threads := BuildThreads(msgs)
	if len(threads) != 1 {
		t.Fatalf("expected 1 thread, got %d", len(threads))
	}
	// Linking is chronological: A links to B, B cycle link is dropped => root is B.
	if threads[0].Root == nil || threads[0].Root.ID != "20260209-080001-0001" {
		t.Fatalf("unexpected root: %#v", threads[0].Root)
	}
	if threads[0].Depth != 1 {
		t.Fatalf("expected depth 1, got %d", threads[0].Depth)
	}
}

func TestBuildThreads_SelfReplyIgnored(t *testing.T) {
	msgs := []fmail.Message{
		{ID: "20260209-080000-0001", From: "alice", To: "task", Time: time.Now().UTC(), Body: "x", ReplyTo: "20260209-080000-0001"},
	}
	threads := BuildThreads(msgs)
	if len(threads) != 1 {
		t.Fatalf("expected 1 thread, got %d", len(threads))
	}
	if threads[0].Root == nil || threads[0].Root.ID != "20260209-080000-0001" {
		t.Fatalf("unexpected root: %#v", threads[0].Root)
	}
}

func TestBuildThreads_DepthClamped(t *testing.T) {
	base := time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC)
	msgs := make([]fmail.Message, 0, 16)
	var prevID string
	for i := 0; i < 16; i++ {
		id := fmail.GenerateMessageID(base.Add(time.Duration(i) * time.Second))
		msg := fmail.Message{ID: id, From: "a", To: "task", Time: base.Add(time.Duration(i) * time.Second), Body: "x"}
		if prevID != "" {
			msg.ReplyTo = prevID
		}
		msgs = append(msgs, msg)
		prevID = id
	}

	threads := BuildThreads(msgs)
	if len(threads) != 1 {
		t.Fatalf("expected 1 thread, got %d", len(threads))
	}
	if threads[0].Depth != maxDisplayDepth {
		t.Fatalf("expected depth clamped to %d, got %d", maxDisplayDepth, threads[0].Depth)
	}
}

func TestBuildThread_FindsRoot(t *testing.T) {
	base := time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC)
	msgs := []fmail.Message{
		{ID: "20260209-080000-0001", From: "alice", To: "task", Time: base, Body: "root"},
		{ID: "20260209-080001-0001", From: "bob", To: "task", Time: base.Add(1 * time.Second), Body: "r1", ReplyTo: "20260209-080000-0001"},
	}

	th := BuildThread(msgs, "20260209-080001-0001")
	if th == nil || th.Root == nil {
		t.Fatalf("expected thread")
	}
	if th.Root.ID != "20260209-080000-0001" {
		t.Fatalf("expected root, got %s", th.Root.ID)
	}
}

func TestSummarizeThread_FirstLine(t *testing.T) {
	base := time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC)
	msgs := []fmail.Message{
		{ID: "20260209-080000-0001", From: "alice", To: "task", Time: base, Body: "hello\nworld"},
	}
	th := BuildThread(msgs, "20260209-080000-0001")
	sum := SummarizeThread(th)
	if sum.Title != "hello" {
		t.Fatalf("expected title 'hello', got %q", sum.Title)
	}
	if sum.MessageCount != 1 {
		t.Fatalf("expected message count 1, got %d", sum.MessageCount)
	}
	if sum.ParticipantCount != 1 {
		t.Fatalf("expected participant count 1, got %d", sum.ParticipantCount)
	}
	if sum.LastActivity.IsZero() {
		t.Fatalf("expected last activity set")
	}
}

func TestIsCrossTargetReply(t *testing.T) {
	base := time.Date(2026, 2, 9, 8, 0, 0, 0, time.UTC)
	msgs := []fmail.Message{
		{ID: "20260209-080000-0001", From: "alice", To: "task", Time: base, Body: "root"},
		{ID: "20260209-080001-0001", From: "bob", To: "build", Time: base.Add(1 * time.Second), Body: "reply", ReplyTo: "20260209-080000-0001"},
	}
	th := BuildThreads(msgs)[0]
	nodes := FlattenThread(th)
	if len(nodes) != 2 {
		t.Fatalf("expected 2 nodes, got %d", len(nodes))
	}
	if IsCrossTargetReply(nodes[1]) != true {
		t.Fatalf("expected cross-target reply")
	}
}
