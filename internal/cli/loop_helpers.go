package cli

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

type loopSelector struct {
	All     bool
	LoopRef string
	Repo    string
	Pool    string
	Profile string
	State   string
	Tag     string
}

func resolveRepoPath(path string) (string, error) {
	if path == "" {
		cwd, err := os.Getwd()
		if err != nil {
			return "", fmt.Errorf("failed to get current directory: %w", err)
		}
		return filepath.Abs(cwd)
	}
	return filepath.Abs(path)
}

func resolvePromptPath(repoPath, value string) (string, bool, error) {
	if strings.TrimSpace(value) == "" {
		return "", false, errors.New("prompt is required")
	}

	candidate := value
	if !filepath.IsAbs(candidate) {
		candidate = filepath.Join(repoPath, value)
	}
	if exists(candidate) {
		return candidate, true, nil
	}

	if !strings.HasSuffix(value, ".md") {
		candidate = filepath.Join(repoPath, ".forge", "prompts", value+".md")
	} else {
		candidate = filepath.Join(repoPath, ".forge", "prompts", value)
	}
	if exists(candidate) {
		return candidate, true, nil
	}

	return "", false, fmt.Errorf("prompt not found: %s", value)
}

func parseTags(value string) []string {
	if strings.TrimSpace(value) == "" {
		return nil
	}
	parts := strings.Split(value, ",")
	seen := make(map[string]struct{})
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		tag := strings.TrimSpace(part)
		if tag == "" {
			continue
		}
		if _, ok := seen[tag]; ok {
			continue
		}
		seen[tag] = struct{}{}
		out = append(out, tag)
	}
	return out
}

func parseCSVInts(value string) ([]int, error) {
	value = strings.TrimSpace(value)
	if value == "" {
		return nil, nil
	}
	parts := strings.Split(value, ",")
	out := make([]int, 0, len(parts))
	for _, part := range parts {
		part = strings.TrimSpace(part)
		if part == "" {
			continue
		}
		n, err := strconv.Atoi(part)
		if err != nil {
			return nil, fmt.Errorf("invalid int %q", part)
		}
		out = append(out, n)
	}
	return out, nil
}

func parseDuration(value string, fallback time.Duration) (time.Duration, error) {
	if strings.TrimSpace(value) == "" {
		return fallback, nil
	}
	parsed, err := time.ParseDuration(value)
	if err != nil {
		return 0, fmt.Errorf("invalid duration %q", value)
	}
	return parsed, nil
}

func selectLoops(ctx context.Context, loopRepo *db.LoopRepository, poolRepo *db.PoolRepository, profileRepo *db.ProfileRepository, selector loopSelector) ([]*models.Loop, error) {
	loops, err := loopRepo.List(ctx)
	if err != nil {
		return nil, err
	}

	repoFilter := selector.Repo
	if repoFilter != "" {
		repoFilter, err = filepath.Abs(repoFilter)
		if err != nil {
			return nil, err
		}
	}

	var poolID string
	if selector.Pool != "" {
		pool, err := resolvePoolByRef(ctx, poolRepo, selector.Pool)
		if err != nil {
			return nil, err
		}
		poolID = pool.ID
	}

	var profileID string
	if selector.Profile != "" {
		profile, err := resolveProfileByRef(ctx, profileRepo, selector.Profile)
		if err != nil {
			return nil, err
		}
		profileID = profile.ID
	}

	filtered := make([]*models.Loop, 0)
	for _, loop := range loops {
		if repoFilter != "" && loop.RepoPath != repoFilter {
			continue
		}
		if poolID != "" && loop.PoolID != poolID {
			continue
		}
		if profileID != "" && loop.ProfileID != profileID {
			continue
		}
		if selector.State != "" && string(loop.State) != selector.State {
			continue
		}
		if selector.Tag != "" && !loopHasTag(loop, selector.Tag) {
			continue
		}
		filtered = append(filtered, loop)
	}

	if selector.LoopRef == "" {
		return filtered, nil
	}
	if len(filtered) == 0 {
		return nil, fmt.Errorf("loop %q not found", selector.LoopRef)
	}

	matches, err := matchLoopRef(filtered, selector.LoopRef)
	if err != nil {
		return nil, err
	}

	return matches, nil
}

func resolveLoopByRef(ctx context.Context, repo *db.LoopRepository, ref string) (*models.Loop, error) {
	ref = strings.TrimSpace(ref)
	if ref == "" {
		return nil, errors.New("loop name or ID required")
	}

	loopEntry, err := repo.GetByShortID(ctx, strings.ToLower(ref))
	if err == nil {
		return loopEntry, nil
	}
	if !errors.Is(err, db.ErrLoopNotFound) {
		return nil, fmt.Errorf("failed to get loop by short ID: %w", err)
	}

	loopEntry, err = repo.Get(ctx, ref)
	if err == nil {
		return loopEntry, nil
	}
	if !errors.Is(err, db.ErrLoopNotFound) {
		return nil, fmt.Errorf("failed to get loop: %w", err)
	}

	loopEntry, err = repo.GetByName(ctx, ref)
	if err == nil {
		return loopEntry, nil
	}
	if !errors.Is(err, db.ErrLoopNotFound) {
		return nil, fmt.Errorf("failed to get loop by name: %w", err)
	}

	loops, err := repo.List(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to list loops: %w", err)
	}
	matches, err := matchLoopRef(loops, ref)
	if err != nil {
		return nil, err
	}
	return matches[0], nil
}

func loopHasTag(loop *models.Loop, tag string) bool {
	for _, value := range loop.Tags {
		if value == tag {
			return true
		}
	}
	return false
}

func exists(path string) bool {
	if _, err := os.Stat(path); err == nil {
		return true
	}
	return false
}

func matchLoopRef(loops []*models.Loop, ref string) ([]*models.Loop, error) {
	ref = strings.TrimSpace(ref)
	if ref == "" {
		return nil, errors.New("loop name or ID required")
	}
	normalized := strings.ToLower(ref)

	for _, loop := range loops {
		if loop == nil {
			continue
		}
		if strings.EqualFold(loop.ShortID, ref) {
			return []*models.Loop{loop}, nil
		}
	}

	for _, loop := range loops {
		if loop == nil {
			continue
		}
		if loop.ID == ref {
			return []*models.Loop{loop}, nil
		}
	}

	for _, loop := range loops {
		if loop == nil {
			continue
		}
		if loop.Name == ref {
			return []*models.Loop{loop}, nil
		}
	}

	matches := make([]*models.Loop, 0)
	seen := make(map[string]struct{})
	for _, loop := range loops {
		if loop == nil {
			continue
		}
		shortID := strings.ToLower(loop.ShortID)
		if shortID != "" && strings.HasPrefix(shortID, normalized) {
			if _, ok := seen[loop.ID]; !ok {
				matches = append(matches, loop)
				seen[loop.ID] = struct{}{}
			}
			continue
		}
		if loop.ID != "" && strings.HasPrefix(loop.ID, ref) {
			if _, ok := seen[loop.ID]; !ok {
				matches = append(matches, loop)
				seen[loop.ID] = struct{}{}
			}
		}
	}

	if len(matches) == 1 {
		return matches, nil
	}
	if len(matches) > 1 {
		sort.Slice(matches, func(i, j int) bool {
			left := strings.ToLower(matches[i].Name)
			right := strings.ToLower(matches[j].Name)
			if left == right {
				return loopShortID(matches[i]) < loopShortID(matches[j])
			}
			return left < right
		})
		return nil, fmt.Errorf("loop '%s' is ambiguous; matches: %s (use a longer prefix or full ID)", ref, formatLoopMatches(matches))
	}
	if len(loops) == 0 {
		return nil, fmt.Errorf("loop '%s' not found (no loops registered yet)", ref)
	}

	example := fmt.Sprintf("Example input: '%s' or '%s'", loops[0].Name, loopShortID(loops[0]))
	return nil, fmt.Errorf("loop '%s' not found. %s", ref, example)
}

func loopShortID(loopEntry *models.Loop) string {
	if loopEntry == nil {
		return ""
	}
	if loopEntry.ShortID != "" {
		return loopEntry.ShortID
	}
	return shortID(loopEntry.ID)
}

func formatLoopMatches(loops []*models.Loop) string {
	parts := make([]string, 0, len(loops))
	for _, loopEntry := range loops {
		if loopEntry == nil {
			continue
		}
		label := loopShortID(loopEntry)
		if loopEntry.Name != "" {
			label = fmt.Sprintf("%s (%s)", loopEntry.Name, label)
		}
		parts = append(parts, label)
	}
	return strings.Join(parts, ", ")
}
