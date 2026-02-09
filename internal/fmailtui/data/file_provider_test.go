package data

import (
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
