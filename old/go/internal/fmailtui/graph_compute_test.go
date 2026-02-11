package fmailtui

import (
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestBuildGraphSnapshot_TopicBroadcastEdges(t *testing.T) {
	now := time.Now().UTC()
	msgs := []fmail.Message{
		{ID: "1", From: "alice", To: "task", Time: now, Body: "a"},
		{ID: "2", From: "bob", To: "task", Time: now.Add(1 * time.Second), Body: "b"},
	}

	snap := buildGraphSnapshot(msgs, 12)
	if snap.Messages != 2 {
		t.Fatalf("Messages=%d, want 2", snap.Messages)
	}
	if len(snap.Nodes) != 2 {
		t.Fatalf("Nodes=%d, want 2", len(snap.Nodes))
	}
	if len(snap.Edges) != 2 {
		t.Fatalf("Edges=%d, want 2", len(snap.Edges))
	}

	assertEdgeCount(t, snap, "alice", "bob", 1)
	assertEdgeCount(t, snap, "bob", "alice", 1)
}

func TestBuildGraphSnapshot_TopicCountsScaleWithMessages(t *testing.T) {
	now := time.Now().UTC()
	msgs := []fmail.Message{
		{ID: "1", From: "alice", To: "task", Time: now, Body: "a1"},
		{ID: "2", From: "alice", To: "task", Time: now.Add(1 * time.Second), Body: "a2"},
		{ID: "3", From: "alice", To: "task", Time: now.Add(2 * time.Second), Body: "a3"},
		{ID: "4", From: "bob", To: "task", Time: now.Add(3 * time.Second), Body: "b"},
	}

	snap := buildGraphSnapshot(msgs, 12)
	assertEdgeCount(t, snap, "alice", "bob", 3)
	assertEdgeCount(t, snap, "bob", "alice", 1)
}

func TestBuildGraphSnapshot_DMsAreDirected(t *testing.T) {
	now := time.Now().UTC()
	msgs := []fmail.Message{
		{ID: "1", From: "alice", To: "@bob", Time: now, Body: "a"},
		{ID: "2", From: "bob", To: "@alice", Time: now.Add(1 * time.Second), Body: "b"},
		{ID: "3", From: "bob", To: "@alice", Time: now.Add(2 * time.Second), Body: "b2"},
	}

	snap := buildGraphSnapshot(msgs, 12)
	assertEdgeCount(t, snap, "alice", "bob", 1)
	assertEdgeCount(t, snap, "bob", "alice", 2)
}

func TestBuildGraphSnapshot_CollapsesToOthersNode(t *testing.T) {
	now := time.Now().UTC()
	msgs := make([]fmail.Message, 0, 40)
	for i := 0; i < 20; i++ {
		from := "a" + string(rune('a'+i))
		to := "@z"
		msgs = append(msgs, fmail.Message{
			ID:   fmail.GenerateMessageID(now.Add(time.Duration(i) * time.Second)),
			From: from,
			To:   to,
			Time: now.Add(time.Duration(i) * time.Second),
			Body: "x",
		})
	}

	snap := buildGraphSnapshot(msgs, 4)
	if len(snap.Nodes) > 4 {
		t.Fatalf("Nodes=%d, want <= 4", len(snap.Nodes))
	}
	foundOthers := false
	for i := range snap.Nodes {
		if snap.Nodes[i].Name == "others" {
			foundOthers = true
			break
		}
	}
	if !foundOthers {
		t.Fatalf("expected others node")
	}
}

func assertEdgeCount(t *testing.T, snap graphSnapshot, from, to string, want int) {
	t.Helper()
	for _, e := range snap.Edges {
		if e.From == from && e.To == to {
			if e.Count != want {
				t.Fatalf("edge %s->%s=%d, want %d", from, to, e.Count, want)
			}
			return
		}
	}
	t.Fatalf("missing edge %s->%s", from, to)
}
