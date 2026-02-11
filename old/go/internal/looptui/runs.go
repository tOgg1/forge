package looptui

import (
	"context"
	"strings"

	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

type runView struct {
	Run         *models.LoopRun
	ProfileName string
	Harness     models.Harness
	AuthKind    string
}

func loadRunViews(ctx context.Context, database *db.DB, loopID string) ([]runView, error) {
	if database == nil || strings.TrimSpace(loopID) == "" {
		return nil, nil
	}

	runRepo := db.NewLoopRunRepository(database)
	profileRepo := db.NewProfileRepository(database)

	runs, err := runRepo.ListByLoop(ctx, loopID)
	if err != nil {
		return nil, err
	}
	if len(runs) == 0 {
		return nil, nil
	}

	profiles, _ := profileRepo.List(ctx)
	profileByID := make(map[string]*models.Profile, len(profiles))
	for _, profile := range profiles {
		if profile == nil {
			continue
		}
		profileByID[profile.ID] = profile
	}

	views := make([]runView, 0, len(runs))
	for _, run := range runs {
		if run == nil {
			continue
		}
		view := runView{Run: run}
		if profile, ok := profileByID[run.ProfileID]; ok && profile != nil {
			view.ProfileName = profile.Name
			view.Harness = profile.Harness
			view.AuthKind = profile.AuthKind
		}
		views = append(views, view)
	}
	return views, nil
}

func runOutputLines(run *models.LoopRun, maxLines int) []string {
	if run == nil {
		return nil
	}
	content := strings.TrimRight(run.OutputTail, "\n")
	if strings.TrimSpace(content) == "" {
		return nil
	}
	lines := strings.Split(content, "\n")
	if maxLines > 0 && len(lines) > maxLines {
		lines = lines[len(lines)-maxLines:]
	}
	return lines
}
