package data

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestFileProviderCachesByTTL(t *testing.T) {
	root := t.TempDir()
	store, err := fmail.NewStore(root)
	require.NoError(t, err)
	require.NoError(t, store.EnsureRoot())

	_, err = store.SaveMessage(&fmail.Message{
		From: "alice",
		To:   "task",
		Body: "first",
		Time: time.Now().UTC().Add(-time.Minute),
	})
	require.NoError(t, err)

	provider, err := NewFileProvider(FileProviderConfig{
		Root:     root,
		CacheTTL: 200 * time.Millisecond,
	})
	require.NoError(t, err)

	messages, err := provider.Messages("task", MessageFilter{})
	require.NoError(t, err)
	require.Len(t, messages, 1)

	_, err = store.SaveMessage(&fmail.Message{
		From: "alice",
		To:   "task",
		Body: "second",
		Time: time.Now().UTC(),
	})
	require.NoError(t, err)

	cached, err := provider.Messages("task", MessageFilter{})
	require.NoError(t, err)
	require.Len(t, cached, 1)

	time.Sleep(300 * time.Millisecond)
	refreshed, err := provider.Messages("task", MessageFilter{})
	require.NoError(t, err)
	require.Len(t, refreshed, 2)
}

func TestFileProviderDMConversationView(t *testing.T) {
	root := t.TempDir()
	store, err := fmail.NewStore(root)
	require.NoError(t, err)
	require.NoError(t, store.EnsureRoot())

	input := []fmail.Message{
		{From: "alice", To: "@bob", Body: "a->b", Time: time.Now().UTC().Add(-4 * time.Minute)},
		{From: "bob", To: "@alice", Body: "b->a", Time: time.Now().UTC().Add(-3 * time.Minute)},
		{From: "alice", To: "@charlie", Body: "a->c", Time: time.Now().UTC().Add(-2 * time.Minute)},
		{From: "charlie", To: "@alice", Body: "c->a", Time: time.Now().UTC().Add(-time.Minute)},
	}
	for i := range input {
		_, err := store.SaveMessage(&input[i])
		require.NoError(t, err)
	}

	provider, err := NewFileProvider(FileProviderConfig{
		Root:      root,
		SelfAgent: "alice",
	})
	require.NoError(t, err)

	conversations, err := provider.DMConversations("alice")
	require.NoError(t, err)
	require.Len(t, conversations, 2)

	bob, err := provider.DMs("bob", MessageFilter{})
	require.NoError(t, err)
	require.Len(t, bob, 2)
	require.Equal(t, "@bob", bob[0].To)
	require.Equal(t, "@alice", bob[1].To)
}

func TestFileProviderTopicsAndSearch(t *testing.T) {
	root := t.TempDir()
	store, err := fmail.NewStore(root)
	require.NoError(t, err)
	require.NoError(t, store.EnsureRoot())

	_, err = store.SaveMessage(&fmail.Message{
		From: "alice",
		To:   "task",
		Body: "deploy ready",
		Time: time.Now().UTC().Add(-2 * time.Minute),
		Tags: []string{"release"},
	})
	require.NoError(t, err)
	_, err = store.SaveMessage(&fmail.Message{
		From: "bob",
		To:   "task",
		Body: "deploy done",
		Time: time.Now().UTC().Add(-time.Minute),
		Tags: []string{"release", "done"},
	})
	require.NoError(t, err)

	provider, err := NewFileProvider(FileProviderConfig{Root: root})
	require.NoError(t, err)

	topics, err := provider.Topics()
	require.NoError(t, err)
	require.Len(t, topics, 1)
	require.Equal(t, 2, topics[0].MessageCount)
	require.NotNil(t, topics[0].LastMessage)
	require.Equal(t, "bob", topics[0].LastMessage.From)

	results, err := provider.Search(SearchQuery{
		Text: "deploy",
		Tags: []string{"done"},
	})
	require.NoError(t, err)
	require.Len(t, results, 1)
	require.Equal(t, "task", results[0].Topic)
	require.Equal(t, "bob", results[0].Message.From)
}

func TestFileProviderDMsIgnoreUnrelatedCorruptDirectory(t *testing.T) {
	root := t.TempDir()
	store, err := fmail.NewStore(root)
	require.NoError(t, err)
	require.NoError(t, store.EnsureRoot())

	input := []fmail.Message{
		{From: "alice", To: "@bob", Body: "a->b 1", Time: time.Now().UTC().Add(-4 * time.Minute)},
		{From: "bob", To: "@alice", Body: "b->a 1", Time: time.Now().UTC().Add(-3 * time.Minute)},
		{From: "alice", To: "@bob", Body: "a->b 2", Time: time.Now().UTC().Add(-2 * time.Minute)},
		{From: "bob", To: "@alice", Body: "b->a 2", Time: time.Now().UTC().Add(-time.Minute)},
	}
	for i := range input {
		_, err := store.SaveMessage(&input[i])
		require.NoError(t, err)
	}

	badDir := filepath.Join(root, "dm", "charlie")
	require.NoError(t, os.MkdirAll(badDir, 0o700))
	require.NoError(t, os.WriteFile(filepath.Join(badDir, "20990101-000000-0001.json"), []byte("{bad json"), 0o600))

	provider, err := NewFileProvider(FileProviderConfig{
		Root:      root,
		SelfAgent: "alice",
	})
	require.NoError(t, err)

	msgs, err := provider.DMs("bob", MessageFilter{Limit: 3})
	require.NoError(t, err)
	require.Len(t, msgs, 3)
	require.Equal(t, "@alice", msgs[len(msgs)-1].To)
}

func TestFileProviderTopicsMetadataIndexAvoidsFullReload(t *testing.T) {
	root := t.TempDir()
	store, err := fmail.NewStore(root)
	require.NoError(t, err)
	require.NoError(t, store.EnsureRoot())

	seed := []fmail.Message{
		{From: "alice", To: "task", Body: "one", Time: time.Now().UTC().Add(-4 * time.Minute)},
		{From: "bob", To: "task", Body: "two", Time: time.Now().UTC().Add(-3 * time.Minute)},
		{From: "charlie", To: "ops", Body: "three", Time: time.Now().UTC().Add(-2 * time.Minute)},
	}
	for i := range seed {
		_, err := store.SaveMessage(&seed[i])
		require.NoError(t, err)
	}

	provider, err := NewFileProvider(FileProviderConfig{
		Root:        root,
		CacheTTL:    25 * time.Millisecond,
		MetadataTTL: 2 * time.Second,
	})
	require.NoError(t, err)

	first, err := provider.Topics()
	require.NoError(t, err)
	require.Len(t, first, 2)
	lookups1, _ := provider.messageReadStats()
	require.Greater(t, lookups1, int64(0))

	time.Sleep(50 * time.Millisecond) // expire topics cache, keep metadata cache

	second, err := provider.Topics()
	require.NoError(t, err)
	require.Len(t, second, 2)
	lookups2, _ := provider.messageReadStats()
	require.Equal(t, lookups1, lookups2)

	newID, err := store.SaveMessage(&fmail.Message{
		From: "dora",
		To:   "task",
		Body: "delta",
		Time: time.Now().UTC(),
	})
	require.NoError(t, err)
	provider.invalidateMetadataForPath(store.TopicMessagePath("task", newID))

	third, err := provider.Topics()
	require.NoError(t, err)
	task := findTopicInfo(third, "task")
	require.NotNil(t, task)
	require.Equal(t, 3, task.MessageCount)
	lookups3, _ := provider.messageReadStats()
	require.Greater(t, lookups3, lookups2)
	require.LessOrEqual(t, lookups3-lookups2, int64(2))
}

func TestFileProviderDMConversationMetadataIndexAvoidsFullReload(t *testing.T) {
	root := t.TempDir()
	store, err := fmail.NewStore(root)
	require.NoError(t, err)
	require.NoError(t, store.EnsureRoot())

	seed := []fmail.Message{
		{From: "alice", To: "@bob", Body: "a->b", Time: time.Now().UTC().Add(-4 * time.Minute)},
		{From: "bob", To: "@alice", Body: "b->a", Time: time.Now().UTC().Add(-3 * time.Minute)},
		{From: "alice", To: "@charlie", Body: "a->c", Time: time.Now().UTC().Add(-2 * time.Minute)},
	}
	for i := range seed {
		_, err := store.SaveMessage(&seed[i])
		require.NoError(t, err)
	}

	provider, err := NewFileProvider(FileProviderConfig{
		Root:        root,
		SelfAgent:   "alice",
		CacheTTL:    25 * time.Millisecond,
		MetadataTTL: 2 * time.Second,
	})
	require.NoError(t, err)

	first, err := provider.DMConversations("alice")
	require.NoError(t, err)
	require.Len(t, first, 2)
	lookups1, _ := provider.messageReadStats()
	require.Greater(t, lookups1, int64(0))

	time.Sleep(50 * time.Millisecond) // expire message cache entries used by old path

	second, err := provider.DMConversations("alice")
	require.NoError(t, err)
	require.Len(t, second, 2)
	lookups2, _ := provider.messageReadStats()
	require.Equal(t, lookups1, lookups2)

	newID, err := store.SaveMessage(&fmail.Message{
		From: "bob",
		To:   "@alice",
		Body: "b->a 2",
		Time: time.Now().UTC(),
	})
	require.NoError(t, err)
	provider.invalidateMetadataForPath(store.DMMessagePath("alice", newID))

	third, err := provider.DMConversations("alice")
	require.NoError(t, err)
	bob := findDMConversation(third, "bob")
	require.NotNil(t, bob)
	require.Equal(t, 3, bob.MessageCount)
	lookups3, _ := provider.messageReadStats()
	require.Greater(t, lookups3, lookups2)
	require.LessOrEqual(t, lookups3-lookups2, int64(2))
}

func findTopicInfo(topics []TopicInfo, topic string) *TopicInfo {
	for i := range topics {
		if topics[i].Name == topic {
			return &topics[i]
		}
	}
	return nil
}

func findDMConversation(conversations []DMConversation, agent string) *DMConversation {
	for i := range conversations {
		if conversations[i].Agent == agent {
			return &conversations[i]
		}
	}
	return nil
}
