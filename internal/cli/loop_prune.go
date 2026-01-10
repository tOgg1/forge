package cli

import (
	"context"
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

var (
	loopPruneRepo    string
	loopPrunePool    string
	loopPruneProfile string
	loopPruneTag     string
)

func init() {
	rootCmd.AddCommand(loopPruneCmd)

	loopPruneCmd.Flags().StringVar(&loopPruneRepo, "repo", "", "filter by repo path")
	loopPruneCmd.Flags().StringVar(&loopPrunePool, "pool", "", "filter by pool")
	loopPruneCmd.Flags().StringVar(&loopPruneProfile, "profile", "", "filter by profile")
	loopPruneCmd.Flags().StringVar(&loopPruneTag, "tag", "", "filter by tag")
}

var loopPruneCmd = &cobra.Command{
	Use:   "prune",
	Short: "Remove inactive loops",
	Long: `Remove inactive loop records (stopped or errored).

Logs and ledgers are left on disk.`,
	Args: cobra.NoArgs,
	RunE: func(cmd *cobra.Command, args []string) error {
		ctx := context.Background()
		database, err := openDatabase()
		if err != nil {
			return err
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		poolRepo := db.NewPoolRepository(database)
		profileRepo := db.NewProfileRepository(database)

		selector := loopSelector{
			Repo:    loopPruneRepo,
			Pool:    loopPrunePool,
			Profile: loopPruneProfile,
			Tag:     loopPruneTag,
		}

		loops, err := selectLoops(ctx, loopRepo, poolRepo, profileRepo, selector)
		if err != nil {
			return err
		}

		prunable := make([]*models.Loop, 0, len(loops))
		skipped := 0
		for _, loopEntry := range loops {
			switch loopEntry.State {
			case models.LoopStateStopped, models.LoopStateError:
				prunable = append(prunable, loopEntry)
			default:
				skipped++
			}
		}

		if len(prunable) == 0 {
			return fmt.Errorf("no inactive loops matched")
		}

		impact := fmt.Sprintf("This will remove %d loop record(s). Logs and ledgers will remain on disk.", len(prunable))
		if skipped > 0 {
			impact += fmt.Sprintf(" %d loop(s) are still active and will be left untouched.", skipped)
		}
		resourceType := "loop"
		resourceID := prunable[0].Name
		if len(prunable) > 1 {
			resourceType = "loops"
			resourceID = fmt.Sprintf("%d loops", len(prunable))
		}
		if !ConfirmDestructiveAction(resourceType, resourceID, impact) {
			fmt.Fprintln(os.Stderr, "Cancelled.")
			return nil
		}

		for _, loopEntry := range prunable {
			if err := loopRepo.Delete(ctx, loopEntry.ID); err != nil {
				return err
			}
		}

		if IsJSONOutput() || IsJSONLOutput() {
			result := map[string]any{"removed": len(prunable)}
			if skipped > 0 {
				result["skipped"] = skipped
			}
			if len(prunable) == 1 {
				result["loop_id"] = prunable[0].ID
				result["name"] = prunable[0].Name
			} else {
				ids := make([]string, 0, len(prunable))
				names := make([]string, 0, len(prunable))
				for _, loopEntry := range prunable {
					ids = append(ids, loopEntry.ID)
					names = append(names, loopEntry.Name)
				}
				result["loop_ids"] = ids
				result["names"] = names
			}
			return WriteOutput(os.Stdout, result)
		}

		if IsQuiet() {
			return nil
		}

		if len(prunable) == 1 {
			fmt.Printf("Loop '%s' removed\n", prunable[0].Name)
			return nil
		}

		if skipped > 0 {
			fmt.Printf("Removed %d loop(s); skipped %d active loop(s)\n", len(prunable), skipped)
			return nil
		}

		fmt.Printf("Removed %d loop(s)\n", len(prunable))
		return nil
	},
}
