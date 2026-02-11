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
	loopCleanRepo    string
	loopCleanPool    string
	loopCleanProfile string
	loopCleanTag     string
)

func init() {
	rootCmd.AddCommand(loopCleanCmd)

	loopCleanCmd.Flags().StringVar(&loopCleanRepo, "repo", "", "filter by repo path")
	loopCleanCmd.Flags().StringVar(&loopCleanPool, "pool", "", "filter by pool")
	loopCleanCmd.Flags().StringVar(&loopCleanProfile, "profile", "", "filter by profile")
	loopCleanCmd.Flags().StringVar(&loopCleanTag, "tag", "", "filter by tag")
}

var loopCleanCmd = &cobra.Command{
	Use:   "clean",
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
			Repo:    loopCleanRepo,
			Pool:    loopCleanPool,
			Profile: loopCleanProfile,
			Tag:     loopCleanTag,
		}

		loops, err := selectLoops(ctx, loopRepo, poolRepo, profileRepo, selector)
		if err != nil {
			return err
		}

		cleanable := make([]*models.Loop, 0, len(loops))
		skipped := 0
		for _, loopEntry := range loops {
			switch loopEntry.State {
			case models.LoopStateStopped, models.LoopStateError:
				cleanable = append(cleanable, loopEntry)
			default:
				skipped++
			}
		}

		if len(cleanable) == 0 {
			return fmt.Errorf("no inactive loops matched")
		}

		impact := fmt.Sprintf("This will remove %d loop record(s). Logs and ledgers will remain on disk.", len(cleanable))
		if skipped > 0 {
			impact += fmt.Sprintf(" %d loop(s) are still active and will be left untouched.", skipped)
		}
		resourceType := "loop"
		resourceID := cleanable[0].Name
		if len(cleanable) > 1 {
			resourceType = "loops"
			resourceID = fmt.Sprintf("%d loops", len(cleanable))
		}
		if !ConfirmDestructiveAction(resourceType, resourceID, impact) {
			fmt.Fprintln(os.Stderr, "Cancelled.")
			return nil
		}

		for _, loopEntry := range cleanable {
			if err := loopRepo.Delete(ctx, loopEntry.ID); err != nil {
				return err
			}
		}

		if IsJSONOutput() || IsJSONLOutput() {
			result := map[string]any{"removed": len(cleanable)}
			if skipped > 0 {
				result["skipped"] = skipped
			}
			if len(cleanable) == 1 {
				result["loop_id"] = cleanable[0].ID
				result["name"] = cleanable[0].Name
			} else {
				ids := make([]string, 0, len(cleanable))
				names := make([]string, 0, len(cleanable))
				for _, loopEntry := range cleanable {
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

		if len(cleanable) == 1 {
			fmt.Printf("Loop '%s' removed\n", cleanable[0].Name)
			return nil
		}

		if skipped > 0 {
			fmt.Printf("Removed %d loop(s); skipped %d active loop(s)\n", len(cleanable), skipped)
			return nil
		}

		fmt.Printf("Removed %d loop(s)\n", len(cleanable))
		return nil
	},
}
