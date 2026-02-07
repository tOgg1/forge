package db

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"
	"github.com/tOgg1/forge/internal/models"
)

func TestLoopWorkStateRepository_SetCurrentClearsPrevious(t *testing.T) {
	database, err := OpenInMemory()
	require.NoError(t, err)
	defer database.Close()
	require.NoError(t, database.Migrate(context.Background()))

	loopRepo := NewLoopRepository(database)
	loopEntry := &models.Loop{
		Name:            "loop-work-test",
		RepoPath:        t.TempDir(),
		BasePromptMsg:   "base",
		IntervalSeconds: 1,
		State:           models.LoopStateStopped,
	}
	require.NoError(t, loopRepo.Create(context.Background(), loopEntry))

	repo := NewLoopWorkStateRepository(database)

	s1 := &models.LoopWorkState{LoopID: loopEntry.ID, AgentID: "a", TaskID: "sv-1", Status: "blocked", Detail: "waiting", LoopIteration: 3}
	require.NoError(t, repo.SetCurrent(context.Background(), s1))

	cur, err := repo.GetCurrent(context.Background(), loopEntry.ID)
	require.NoError(t, err)
	require.Equal(t, "sv-1", cur.TaskID)
	require.True(t, cur.IsCurrent)

	s2 := &models.LoopWorkState{LoopID: loopEntry.ID, AgentID: "a", TaskID: "sv-2", Status: "in_progress", LoopIteration: 4}
	require.NoError(t, repo.SetCurrent(context.Background(), s2))

	cur, err = repo.GetCurrent(context.Background(), loopEntry.ID)
	require.NoError(t, err)
	require.Equal(t, "sv-2", cur.TaskID)

	items, err := repo.ListByLoop(context.Background(), loopEntry.ID, 0)
	require.NoError(t, err)
	require.Len(t, items, 2)
	require.True(t, items[0].IsCurrent)
	require.False(t, items[1].IsCurrent)
}
