package cli

import (
	"context"
	"testing"

	forgedv1 "github.com/tOgg1/forge/gen/forged/v1"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

func TestReconcileLoopLivenessMarksStaleRunningLoop(t *testing.T) {
	repo := t.TempDir()
	cleanup := withTempConfig(t, repo)
	defer cleanup()

	withWorkingDir(t, repo, func() {
		database, err := openDatabase()
		if err != nil {
			t.Fatalf("open database: %v", err)
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		entry := &models.Loop{
			Name:     "stale-running",
			RepoPath: repo,
			State:    models.LoopStateRunning,
			Metadata: map[string]any{
				loopMetadataRunnerOwnerKey: string(loopSpawnOwnerLocal),
			},
		}
		if err := loopRepo.Create(context.Background(), entry); err != nil {
			t.Fatalf("create loop: %v", err)
		}

		originalList := listDaemonRunnersFunc
		defer func() { listDaemonRunnersFunc = originalList }()
		listDaemonRunnersFunc = func(context.Context) (map[string]*forgedv1.LoopRunner, bool) {
			return map[string]*forgedv1.LoopRunner{}, true
		}

		loops, err := loopRepo.List(context.Background())
		if err != nil {
			t.Fatalf("list loops: %v", err)
		}

		if _, err := reconcileLoopLiveness(context.Background(), loopRepo, loops); err != nil {
			t.Fatalf("reconcileLoopLiveness: %v", err)
		}

		updated, err := loopRepo.Get(context.Background(), entry.ID)
		if err != nil {
			t.Fatalf("get loop: %v", err)
		}
		if updated.State != models.LoopStateStopped {
			t.Fatalf("state = %s, want %s", updated.State, models.LoopStateStopped)
		}
		if updated.LastError != loopStaleRunnerReason {
			t.Fatalf("last_error = %q, want %q", updated.LastError, loopStaleRunnerReason)
		}
	})
}

func TestReconcileLoopLivenessSkipsDaemonOwnerWhenDaemonUnknown(t *testing.T) {
	repo := t.TempDir()
	cleanup := withTempConfig(t, repo)
	defer cleanup()

	withWorkingDir(t, repo, func() {
		database, err := openDatabase()
		if err != nil {
			t.Fatalf("open database: %v", err)
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		entry := &models.Loop{
			Name:     "daemon-owned-running",
			RepoPath: repo,
			State:    models.LoopStateRunning,
			Metadata: map[string]any{
				loopMetadataRunnerOwnerKey: string(loopSpawnOwnerDaemon),
			},
		}
		if err := loopRepo.Create(context.Background(), entry); err != nil {
			t.Fatalf("create loop: %v", err)
		}

		originalList := listDaemonRunnersFunc
		defer func() { listDaemonRunnersFunc = originalList }()
		listDaemonRunnersFunc = func(context.Context) (map[string]*forgedv1.LoopRunner, bool) {
			return nil, false
		}

		loops, err := loopRepo.List(context.Background())
		if err != nil {
			t.Fatalf("list loops: %v", err)
		}

		if _, err := reconcileLoopLiveness(context.Background(), loopRepo, loops); err != nil {
			t.Fatalf("reconcileLoopLiveness: %v", err)
		}

		updated, err := loopRepo.Get(context.Background(), entry.ID)
		if err != nil {
			t.Fatalf("get loop: %v", err)
		}
		if updated.State != models.LoopStateRunning {
			t.Fatalf("state = %s, want %s", updated.State, models.LoopStateRunning)
		}
	})
}
