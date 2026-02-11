package cli

import (
	"context"
	"strings"
	"time"

	forgedv1 "github.com/tOgg1/forge/gen/forged/v1"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
	"github.com/tOgg1/forge/internal/procutil"
)

type loopRunnerLiveness struct {
	Owner       string
	InstanceID  string
	PIDAlive    *bool
	DaemonAlive *bool
}

var listDaemonRunnersFunc = listDaemonRunners

func reconcileLoopLiveness(ctx context.Context, loopRepo *db.LoopRepository, loops []*models.Loop) (map[string]loopRunnerLiveness, error) {
	result := make(map[string]loopRunnerLiveness, len(loops))
	if len(loops) == 0 {
		return result, nil
	}

	daemonRunners, daemonReachable := listDaemonRunnersFunc(ctx)

	for _, loopEntry := range loops {
		if loopEntry == nil {
			continue
		}

		info := loopRunnerLiveness{
			Owner:      loopRunnerOwner(loopEntry),
			InstanceID: loopRunnerInstanceID(loopEntry),
		}

		if pid, ok := loopPID(loopEntry); ok {
			pidAlive := procutil.IsProcessAlive(pid)
			info.PIDAlive = &pidAlive
		}

		if daemonReachable {
			daemonAlive := daemonRunnerAlive(daemonRunners[loopEntry.ID], info.InstanceID)
			info.DaemonAlive = &daemonAlive
		}

		if shouldMarkLoopStale(loopEntry, info, daemonReachable) {
			if err := markLoopStale(ctx, loopRepo, loopEntry, info); err != nil {
				return nil, err
			}
		}

		result[loopEntry.ID] = info
	}

	return result, nil
}

func shouldMarkLoopStale(loopEntry *models.Loop, info loopRunnerLiveness, daemonReachable bool) bool {
	if loopEntry == nil || loopEntry.State != models.LoopStateRunning {
		return false
	}

	pidMissingOrDead := info.PIDAlive == nil || !*info.PIDAlive
	if !pidMissingOrDead {
		return false
	}

	if daemonReachable {
		return info.DaemonAlive == nil || !*info.DaemonAlive
	}

	// If daemon reachability is unknown, only reconcile non-daemon-owned loops.
	return info.Owner != string(loopSpawnOwnerDaemon)
}

func markLoopStale(ctx context.Context, loopRepo *db.LoopRepository, loopEntry *models.Loop, info loopRunnerLiveness) error {
	if loopEntry == nil {
		return nil
	}

	loopEntry.State = models.LoopStateStopped
	loopEntry.LastError = loopStaleRunnerReason
	if loopEntry.Metadata == nil {
		loopEntry.Metadata = make(map[string]any)
	}
	loopEntry.Metadata[loopMetadataRunnerLivenessKey] = map[string]any{
		"pid_alive":           boolPtrValue(info.PIDAlive),
		"daemon_runner_alive": boolPtrValue(info.DaemonAlive),
		"reconciled_at":       time.Now().UTC().Format(time.RFC3339),
		"reason":              loopStaleRunnerReason,
	}

	if err := loopRepo.Update(ctx, loopEntry); err != nil {
		return err
	}
	return nil
}

func listDaemonRunners(parent context.Context) (map[string]*forgedv1.LoopRunner, bool) {
	ctx, cancel := context.WithTimeout(parent, 2*time.Second)
	defer cancel()

	client, err := dialForgedClientFunc(ctx)
	if err != nil {
		return nil, false
	}
	defer client.Close()

	resp, err := client.ListLoopRunners(ctx, &forgedv1.ListLoopRunnersRequest{})
	if err != nil || resp == nil {
		return nil, false
	}

	result := make(map[string]*forgedv1.LoopRunner, len(resp.Runners))
	for _, runner := range resp.Runners {
		if runner == nil {
			continue
		}
		result[runner.LoopId] = runner
	}
	return result, true
}

func daemonRunnerAlive(runner *forgedv1.LoopRunner, instanceID string) bool {
	if runner == nil {
		return false
	}
	if strings.TrimSpace(instanceID) != "" && strings.TrimSpace(runner.InstanceId) != strings.TrimSpace(instanceID) {
		return false
	}
	return runner.State == forgedv1.LoopRunnerState_LOOP_RUNNER_STATE_RUNNING
}

func boolPtrValue(v *bool) bool {
	if v == nil {
		return false
	}
	return *v
}
