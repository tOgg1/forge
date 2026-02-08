package cli

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strings"
	"time"

	"github.com/spf13/cobra"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

var (
	loopPsRepo    string
	loopPsPool    string
	loopPsProfile string
	loopPsState   string
	loopPsTag     string
)

type loopPSJSONEntry struct {
	*models.Loop
	RunnerOwner      string `json:"runner_owner,omitempty"`
	RunnerInstanceID string `json:"runner_instance_id,omitempty"`
	RunnerPIDAlive   *bool  `json:"runner_pid_alive,omitempty"`
	RunnerDaemonLive *bool  `json:"runner_daemon_alive,omitempty"`
}

func init() {
	rootCmd.AddCommand(loopPsCmd)

	loopPsCmd.Flags().StringVar(&loopPsRepo, "repo", "", "filter by repo path")
	loopPsCmd.Flags().StringVar(&loopPsPool, "pool", "", "filter by pool")
	loopPsCmd.Flags().StringVar(&loopPsProfile, "profile", "", "filter by profile")
	loopPsCmd.Flags().StringVar(&loopPsState, "state", "", "filter by state")
	loopPsCmd.Flags().StringVar(&loopPsTag, "tag", "", "filter by tag")
}

var loopPsCmd = &cobra.Command{
	Use:     "ps",
	Aliases: []string{"ls"},
	Short:   "List loops",
	Args:    cobra.NoArgs,
	RunE: func(cmd *cobra.Command, args []string) error {
		database, err := openDatabase()
		if err != nil {
			return err
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		poolRepo := db.NewPoolRepository(database)
		profileRepo := db.NewProfileRepository(database)
		queueRepo := db.NewLoopQueueRepository(database)
		runRepo := db.NewLoopRunRepository(database)

		repoPath := loopPsRepo
		if repoPath == "" && chdirPath != "" {
			repoPath = chdirPath
		}
		if repoPath != "" {
			repoPath, err = resolveRepoPath(repoPath)
			if err != nil {
				return err
			}
		}

		selector := loopSelector{
			Repo:    repoPath,
			Pool:    loopPsPool,
			Profile: loopPsProfile,
			State:   loopPsState,
			Tag:     loopPsTag,
		}

		loops, err := selectLoops(context.Background(), loopRepo, poolRepo, profileRepo, selector)
		if err != nil {
			return err
		}
		livenessByLoop, err := reconcileLoopLiveness(context.Background(), loopRepo, loops)
		if err != nil {
			return err
		}

		if IsJSONOutput() || IsJSONLOutput() {
			rows := make([]loopPSJSONEntry, 0, len(loops))
			for _, loopEntry := range loops {
				if loopEntry == nil {
					continue
				}
				liveness := livenessByLoop[loopEntry.ID]
				rows = append(rows, loopPSJSONEntry{
					Loop:             loopEntry,
					RunnerOwner:      liveness.Owner,
					RunnerInstanceID: liveness.InstanceID,
					RunnerPIDAlive:   liveness.PIDAlive,
					RunnerDaemonLive: liveness.DaemonAlive,
				})
			}
			return WriteOutput(os.Stdout, rows)
		}

		if len(loops) == 0 {
			fmt.Fprintln(os.Stdout, "No loops found")
			return nil
		}

		sort.Slice(loops, func(i, j int) bool { return loops[i].CreatedAt.Before(loops[j].CreatedAt) })

		loopIDs := make([]string, 0, len(loops))
		for _, loopEntry := range loops {
			loopIDs = append(loopIDs, loopShortID(loopEntry))
		}
		uniquePrefixes := loopUniquePrefixLengths(loopIDs)

		rows := make([][]string, 0, len(loops))
		for _, loopEntry := range loops {
			queueItems, _ := queueRepo.List(context.Background(), loopEntry.ID)
			pending := 0
			for _, item := range queueItems {
				if item.Status == models.LoopQueueStatusPending {
					pending++
				}
			}
			runCount, err := runRepo.CountByLoop(context.Background(), loopEntry.ID)
			if err != nil {
				return err
			}

			lastRun := ""
			if loopEntry.LastRunAt != nil {
				lastRun = loopEntry.LastRunAt.UTC().Format(time.RFC3339)
			}

			waitUntil := ""
			if loopEntry.State == models.LoopStateWaiting && loopEntry.Metadata != nil {
				if value, ok := loopEntry.Metadata["wait_until"]; ok {
					waitUntil = fmt.Sprintf("%v", value)
				}
			}

			displayID := loopShortID(loopEntry)
			uniqueLen := uniquePrefixes[displayID]
			if uniqueLen == 0 {
				uniqueLen = len(displayID)
			}

			rows = append(rows, []string{
				formatLoopShortID(displayID, uniqueLen),
				loopEntry.Name,
				fmt.Sprintf("%d", runCount),
				string(loopEntry.State),
				waitUntil,
				loopEntry.ProfileID,
				loopEntry.PoolID,
				fmt.Sprintf("%d", pending),
				lastRun,
				loopEntry.RepoPath,
			})
		}

		return writeTable(os.Stdout, []string{"ID", "NAME", "RUNS", "STATE", "WAIT_UNTIL", "PROFILE", "POOL", "QUEUE", "LAST_RUN", "REPO"}, rows)
	},
}

func loopUniquePrefixLengths(ids []string) map[string]int {
	result := make(map[string]int, len(ids))
	for idx, id := range ids {
		if id == "" {
			continue
		}
		maxLen := len(id)
		if maxLen == 0 {
			continue
		}
		for length := 1; length <= maxLen; length++ {
			prefix := id[:length]
			unique := true
			for otherIdx, other := range ids {
				if otherIdx == idx {
					continue
				}
				if strings.HasPrefix(other, prefix) {
					unique = false
					break
				}
			}
			if unique {
				result[id] = length
				break
			}
		}
		if result[id] == 0 {
			result[id] = maxLen
		}
	}
	return result
}

func formatLoopShortID(id string, uniqueLen int) string {
	if id == "" {
		return ""
	}
	if uniqueLen <= 0 || uniqueLen > len(id) {
		uniqueLen = len(id)
	}
	if !colorEnabled() {
		return id
	}
	prefix := colorize(id[:uniqueLen], colorYellow)
	if uniqueLen == len(id) {
		return prefix
	}
	return prefix + colorize(id[uniqueLen:], colorCyan)
}
