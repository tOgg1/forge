package cli

import (
	"context"
	"fmt"
	"os"
	"sort"

	"github.com/spf13/cobra"
	"github.com/tOgg1/forge/internal/db"
)

var (
	memLoopRef string
)

func init() {
	rootCmd.AddCommand(memCmd)
	memCmd.Flags().StringVar(&memLoopRef, "loop", "", "loop ref (defaults to FORGE_LOOP_ID)")

	memCmd.AddCommand(memSetCmd)
	memCmd.AddCommand(memGetCmd)
	memCmd.AddCommand(memListCmd)
	memCmd.AddCommand(memRmCmd)
}

var memCmd = &cobra.Command{
	Use:   "mem",
	Short: "Persistent per-loop key/value memory",
}

var memSetCmd = &cobra.Command{
	Use:   "set <key> <value>",
	Short: "Set a memory key",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		loopRef, err := requireLoopRef(memLoopRef)
		if err != nil {
			return err
		}

		database, err := openDatabase()
		if err != nil {
			return err
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		loopEntry, err := resolveLoopByRef(context.Background(), loopRepo, loopRef)
		if err != nil {
			return err
		}

		repo := db.NewLoopKVRepository(database)
		if err := repo.Set(context.Background(), loopEntry.ID, args[0], args[1]); err != nil {
			return err
		}

		if IsJSONOutput() || IsJSONLOutput() {
			return WriteOutput(os.Stdout, map[string]any{"loop": loopEntry.Name, "key": args[0], "ok": true})
		}
		if IsQuiet() {
			return nil
		}
		fmt.Fprintln(os.Stdout, "ok")
		return nil
	},
}

var memGetCmd = &cobra.Command{
	Use:   "get <key>",
	Short: "Get a memory key",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		loopRef, err := requireLoopRef(memLoopRef)
		if err != nil {
			return err
		}

		database, err := openDatabase()
		if err != nil {
			return err
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		loopEntry, err := resolveLoopByRef(context.Background(), loopRepo, loopRef)
		if err != nil {
			return err
		}

		repo := db.NewLoopKVRepository(database)
		entry, err := repo.Get(context.Background(), loopEntry.ID, args[0])
		if err != nil {
			return err
		}

		if IsJSONOutput() || IsJSONLOutput() {
			return WriteOutput(os.Stdout, entry)
		}
		fmt.Fprintln(os.Stdout, entry.Value)
		return nil
	},
}

var memListCmd = &cobra.Command{
	Use:     "ls",
	Aliases: []string{"list"},
	Short:   "List memory keys",
	Args:    cobra.NoArgs,
	RunE: func(cmd *cobra.Command, args []string) error {
		loopRef, err := requireLoopRef(memLoopRef)
		if err != nil {
			return err
		}

		database, err := openDatabase()
		if err != nil {
			return err
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		loopEntry, err := resolveLoopByRef(context.Background(), loopRepo, loopRef)
		if err != nil {
			return err
		}

		repo := db.NewLoopKVRepository(database)
		items, err := repo.ListByLoop(context.Background(), loopEntry.ID)
		if err != nil {
			return err
		}

		if IsJSONOutput() || IsJSONLOutput() {
			return WriteOutput(os.Stdout, items)
		}
		if len(items) == 0 {
			fmt.Fprintln(os.Stdout, "(empty)")
			return nil
		}
		sort.Slice(items, func(i, j int) bool { return items[i].Key < items[j].Key })
		for _, it := range items {
			fmt.Fprintf(os.Stdout, "%s=%s\n", it.Key, it.Value)
		}
		return nil
	},
}

var memRmCmd = &cobra.Command{
	Use:   "rm <key>",
	Short: "Remove a memory key",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		loopRef, err := requireLoopRef(memLoopRef)
		if err != nil {
			return err
		}

		database, err := openDatabase()
		if err != nil {
			return err
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		loopEntry, err := resolveLoopByRef(context.Background(), loopRepo, loopRef)
		if err != nil {
			return err
		}

		repo := db.NewLoopKVRepository(database)
		if err := repo.Delete(context.Background(), loopEntry.ID, args[0]); err != nil {
			return err
		}

		if IsJSONOutput() || IsJSONLOutput() {
			return WriteOutput(os.Stdout, map[string]any{"loop": loopEntry.Name, "key": args[0], "ok": true})
		}
		if IsQuiet() {
			return nil
		}
		fmt.Fprintln(os.Stdout, "ok")
		return nil
	},
}
