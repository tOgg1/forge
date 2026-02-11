package fmail

import (
	"encoding/json"
	"os"
	"strings"
	"time"

	"github.com/spf13/cobra"
)

func runInit(cmd *cobra.Command, args []string) error {
	root, err := DiscoverProjectRoot("")
	if err != nil {
		return Exitf(ExitCodeFailure, "resolve project root: %v", err)
	}

	projectFlag, _ := cmd.Flags().GetString("project")
	projectID := strings.TrimSpace(projectFlag)
	if cmd.Flags().Changed("project") && projectID == "" {
		return usageError(cmd, "project id is required")
	}

	store, err := NewStore(root)
	if err != nil {
		return Exitf(ExitCodeFailure, "init store: %v", err)
	}
	if err := store.EnsureRoot(); err != nil {
		return Exitf(ExitCodeFailure, "ensure store: %v", err)
	}

	existing, err := readProjectIfExists(store.ProjectFile())
	if err != nil {
		return Exitf(ExitCodeFailure, "load project: %v", err)
	}

	if !cmd.Flags().Changed("project") {
		if existing != nil {
			return nil
		}
		projectID, err = DeriveProjectID(root)
		if err != nil {
			return Exitf(ExitCodeFailure, "derive project id: %v", err)
		}
		if _, err := store.EnsureProject(projectID); err != nil {
			return Exitf(ExitCodeFailure, "ensure project: %v", err)
		}
		return nil
	}

	if existing != nil && strings.TrimSpace(existing.ID) == projectID {
		return nil
	}

	created := time.Now().UTC()
	if existing != nil && !existing.Created.IsZero() {
		created = existing.Created
	}
	project := Project{ID: projectID, Created: created}
	data, err := json.MarshalIndent(project, "", "  ")
	if err != nil {
		return Exitf(ExitCodeFailure, "encode project: %v", err)
	}
	if err := os.WriteFile(store.ProjectFile(), data, 0o644); err != nil {
		return Exitf(ExitCodeFailure, "write project: %v", err)
	}
	return nil
}

func readProjectIfExists(path string) (*Project, error) {
	project, err := readProject(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	return project, nil
}
