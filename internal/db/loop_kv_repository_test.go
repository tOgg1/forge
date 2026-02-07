package db

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"
	"github.com/tOgg1/forge/internal/models"
)

func TestLoopKVRepository_SetGetListDelete(t *testing.T) {
	database, err := OpenInMemory()
	require.NoError(t, err)
	defer database.Close()
	require.NoError(t, database.Migrate(context.Background()))

	loopRepo := NewLoopRepository(database)
	loopEntry := testLoop(t, loopRepo)

	repo := NewLoopKVRepository(database)

	require.NoError(t, repo.Set(context.Background(), loopEntry.ID, "blocked_on", "waiting for reply"))
	require.NoError(t, repo.Set(context.Background(), loopEntry.ID, "blocked_on", "still waiting"))

	got, err := repo.Get(context.Background(), loopEntry.ID, "blocked_on")
	require.NoError(t, err)
	require.Equal(t, "blocked_on", got.Key)
	require.Equal(t, "still waiting", got.Value)

	items, err := repo.ListByLoop(context.Background(), loopEntry.ID)
	require.NoError(t, err)
	require.Len(t, items, 1)

	require.NoError(t, repo.Delete(context.Background(), loopEntry.ID, "blocked_on"))
	_, err = repo.Get(context.Background(), loopEntry.ID, "blocked_on")
	require.ErrorIs(t, err, ErrLoopKVNotFound)
}

func testLoop(t *testing.T, repo *LoopRepository) *models.Loop {
	t.Helper()
	loopEntry := &models.Loop{
		Name:            "loop-test",
		RepoPath:        t.TempDir(),
		BasePromptMsg:   "base",
		IntervalSeconds: 1,
		State:           models.LoopStateStopped,
	}
	require.NoError(t, repo.Create(context.Background(), loopEntry))
	return loopEntry
}
