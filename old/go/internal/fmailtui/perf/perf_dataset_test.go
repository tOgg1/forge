//go:build perf

package perf

import (
	"fmt"
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

type datasetConfig struct {
	topics        int
	topicMessages int

	dmPeers        int
	dmMessagesEach int // each direction

	agents int
}

func writeSyntheticMailbox(t testing.TB, projectRoot string, cfg datasetConfig) {
	t.Helper()

	start := time.Date(2026, 2, 9, 0, 0, 0, 0, time.UTC)
	var seq int
	nextID := func(now time.Time) string {
		seq++
		return fmt.Sprintf("%s-%04d", now.UTC().Format("20060102-150405"), seq%10000)
	}

	store, err := fmail.NewStore(projectRoot, fmail.WithNow(func() time.Time { return start }), fmail.WithIDGenerator(nextID))
	if err != nil {
		t.Fatalf("new store: %v", err)
	}
	if err := store.EnsureRoot(); err != nil {
		t.Fatalf("ensure root: %v", err)
	}

	agents := make([]string, 0, cfg.agents)
	for i := 0; i < cfg.agents; i++ {
		agents = append(agents, fmt.Sprintf("agent-%03d", i+1))
	}
	if len(agents) == 0 {
		agents = []string{"agent-001"}
	}

	// Topics
	now := start
	for i := 0; i < cfg.topics; i++ {
		topic := fmt.Sprintf("topic-%03d", i+1)
		for j := 0; j < cfg.topicMessages; j++ {
			from := agents[(i+j)%len(agents)]
			body := fmt.Sprintf("topic msg %d/%d in %s", j+1, cfg.topicMessages, topic)
			if i == 0 && j == 0 {
				body += " needle"
			}
			msg := &fmail.Message{
				ID:       storeID(nextID(now)),
				From:     from,
				To:       topic,
				Time:     now,
				Body:     body,
				Priority: fmail.PriorityNormal,
			}
			if _, err := store.SaveMessageExact(msg); err != nil {
				t.Fatalf("save topic message: %v", err)
			}
			now = now.Add(1 * time.Second)
		}
	}

	// DMs: viewer <-> peer. Messages split across @peer and @viewer directories.
	viewer := "viewer"
	for p := 0; p < cfg.dmPeers; p++ {
		peer := fmt.Sprintf("peer-%03d", p+1)
		for j := 0; j < cfg.dmMessagesEach; j++ {
			// viewer -> peer (written to dm/<peer>)
			msg := &fmail.Message{
				ID:       storeID(nextID(now)),
				From:     viewer,
				To:       "@"+peer,
				Time:     now,
				Body:     fmt.Sprintf("dm to %s %d needle", peer, j+1),
				Priority: fmail.PriorityNormal,
			}
			if _, err := store.SaveMessageExact(msg); err != nil {
				t.Fatalf("save dm message: %v", err)
			}
			now = now.Add(1 * time.Second)

			// peer -> viewer (written to dm/<viewer>)
			reply := &fmail.Message{
				ID:       storeID(nextID(now)),
				From:     peer,
				To:       "@"+viewer,
				Time:     now,
				Body:     fmt.Sprintf("dm from %s %d", peer, j+1),
				Priority: fmail.PriorityNormal,
			}
			if _, err := store.SaveMessageExact(reply); err != nil {
				t.Fatalf("save dm reply: %v", err)
			}
			now = now.Add(1 * time.Second)
		}
	}
}

func storeID(id string) string {
	// Keep ID format consistent with fmail.GenerateMessageID: YYYYMMDD-HHMMSS-####.
	return id
}
