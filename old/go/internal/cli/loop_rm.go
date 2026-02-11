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
	loopRmAll     bool
	loopRmRepo    string
	loopRmPool    string
	loopRmProfile string
	loopRmState   string
	loopRmTag     string
	loopRmForce   bool
)

func init() {
	rootCmd.AddCommand(loopRmCmd)

	loopRmCmd.Flags().BoolVar(&loopRmAll, "all", false, "remove all loops")
	loopRmCmd.Flags().StringVar(&loopRmRepo, "repo", "", "filter by repo path")
	loopRmCmd.Flags().StringVar(&loopRmPool, "pool", "", "filter by pool")
	loopRmCmd.Flags().StringVar(&loopRmProfile, "profile", "", "filter by profile")
	loopRmCmd.Flags().StringVar(&loopRmState, "state", "", "filter by state")
	loopRmCmd.Flags().StringVar(&loopRmTag, "tag", "", "filter by tag")
	loopRmCmd.Flags().BoolVar(&loopRmForce, "force", false, "remove even if loops are running")
}

var loopRmCmd = &cobra.Command{
	Use:     "rm [loop]",
	Aliases: []string{"remove", "delete"},
	Short:   "Remove loop records",
	Long: `Remove loop records from Forge.

This only removes the loop record. Logs and ledgers are left on disk.`,
	Args: cobra.MaximumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		sel := loopSelector{Repo: loopRmRepo, Pool: loopRmPool, Profile: loopRmProfile, State: loopRmState, Tag: loopRmTag}
		if len(args) > 0 {
			sel.LoopRef = args[0]
		}
		if sel.LoopRef == "" && !loopRmAll && sel.Repo == "" && sel.Pool == "" && sel.Profile == "" && sel.State == "" && sel.Tag == "" {
			return fmt.Errorf("specify a loop or selector")
		}

		usesSelector := loopRmAll || sel.Repo != "" || sel.Pool != "" || sel.Profile != "" || sel.State != "" || sel.Tag != ""
		if usesSelector && !loopRmForce {
			return fmt.Errorf("selector-based removal requires --force")
		}

		ctx := context.Background()
		database, err := openDatabase()
		if err != nil {
			return err
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		poolRepo := db.NewPoolRepository(database)
		profileRepo := db.NewProfileRepository(database)

		loops, err := selectLoops(ctx, loopRepo, poolRepo, profileRepo, sel)
		if err != nil {
			return err
		}
		if len(loops) == 0 {
			return fmt.Errorf("no loops matched")
		}

		activeCount := 0
		for _, loopEntry := range loops {
			if loopEntry.State != models.LoopStateStopped {
				activeCount++
				if !loopRmForce {
					return fmt.Errorf("loop %q is %s; use --force to remove anyway", loopEntry.Name, loopEntry.State)
				}
			}
		}

		impact := fmt.Sprintf("This will remove %d loop record(s). Logs and ledgers will remain on disk.", len(loops))
		if activeCount > 0 {
			impact += " Some loops are not stopped; their processes will keep running."
		}
		resourceType := "loop"
		resourceID := loops[0].Name
		if len(loops) > 1 {
			resourceType = "loops"
			resourceID = fmt.Sprintf("%d loops", len(loops))
		}
		if !ConfirmDestructiveAction(resourceType, resourceID, impact) {
			fmt.Fprintln(os.Stderr, "Cancelled.")
			return nil
		}

		for _, loopEntry := range loops {
			if err := loopRepo.Delete(ctx, loopEntry.ID); err != nil {
				return err
			}
		}

		if IsJSONOutput() || IsJSONLOutput() {
			result := map[string]any{"removed": len(loops)}
			if len(loops) == 1 {
				result["loop_id"] = loops[0].ID
				result["name"] = loops[0].Name
			} else {
				ids := make([]string, 0, len(loops))
				names := make([]string, 0, len(loops))
				for _, loopEntry := range loops {
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

		if len(loops) == 1 {
			fmt.Printf("Loop '%s' removed\n", loops[0].Name)
			return nil
		}

		fmt.Printf("Removed %d loop(s)\n", len(loops))
		return nil
	},
}
