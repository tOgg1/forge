package fmail

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"time"

	"github.com/spf13/cobra"
)

func runGC(cmd *cobra.Command, args []string) error {
	root, err := DiscoverProjectRoot("")
	if err != nil {
		return Exitf(ExitCodeFailure, "resolve project root: %v", err)
	}

	days, _ := cmd.Flags().GetInt("days")
	if days < 0 {
		return usageError(cmd, "days must be >= 0")
	}
	dryRun, _ := cmd.Flags().GetBool("dry-run")

	store, err := NewStore(root)
	if err != nil {
		return Exitf(ExitCodeFailure, "init store: %v", err)
	}

	cutoff := time.Now().UTC().Add(-time.Duration(days) * 24 * time.Hour)
	files, err := listGCFiles(store)
	if err != nil {
		return Exitf(ExitCodeFailure, "gc scan: %v", err)
	}

	for _, file := range files {
		fileTime := file.modTime.UTC()
		if ts, ok := parseMessageTime(filepath.Base(file.path)); ok {
			fileTime = ts
		}
		if fileTime.IsZero() || !fileTime.Before(cutoff) {
			continue
		}

		if dryRun {
			path := file.path
			if rel, err := filepath.Rel(store.Root, file.path); err == nil {
				path = rel
			}
			fmt.Fprintln(cmd.OutOrStdout(), path)
			continue
		}

		if err := os.Remove(file.path); err != nil {
			if os.IsNotExist(err) {
				continue
			}
			return Exitf(ExitCodeFailure, "remove %s: %v", file.path, err)
		}
	}
	return nil
}

func listGCFiles(store *Store) ([]messageFile, error) {
	if store == nil {
		return nil, fmt.Errorf("store is nil")
	}

	files := make([]messageFile, 0)
	topicsRoot := filepath.Join(store.Root, "topics")
	topicNames, err := listSubDirs(topicsRoot)
	if err != nil {
		return nil, err
	}
	for _, topic := range topicNames {
		if err := ValidateTopic(topic); err != nil {
			continue
		}
		list, err := listFilesInDir(filepath.Join(topicsRoot, topic))
		if err != nil {
			return nil, err
		}
		files = append(files, list...)
	}

	dmRoot := filepath.Join(store.Root, "dm")
	agentNames, err := listSubDirs(dmRoot)
	if err != nil {
		return nil, err
	}
	for _, agent := range agentNames {
		if err := ValidateAgentName(agent); err != nil {
			continue
		}
		list, err := listFilesInDir(filepath.Join(dmRoot, agent))
		if err != nil {
			return nil, err
		}
		files = append(files, list...)
	}

	return files, nil
}

func listSubDirs(root string) ([]string, error) {
	entries, err := os.ReadDir(root)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	names := make([]string, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() {
			names = append(names, entry.Name())
		}
	}
	sort.Strings(names)
	return names, nil
}
