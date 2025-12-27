// Package cli provides CLI helpers for resolving IDs and names.
package cli

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"

	"github.com/opencode-ai/swarm/internal/config"
	"github.com/opencode-ai/swarm/internal/db"
	"github.com/opencode-ai/swarm/internal/models"
	"github.com/opencode-ai/swarm/internal/node"
)

const maxSuggestions = 5

func shortID(id string) string {
	const limit = 8
	if len(id) <= limit {
		return id
	}
	return id[:limit]
}

func findNode(ctx context.Context, service *node.Service, idOrName string) (*models.Node, error) {
	if strings.TrimSpace(idOrName) == "" {
		return nil, errors.New("node name or ID required")
	}

	n, err := service.GetNodeByName(ctx, idOrName)
	if err == nil {
		return n, nil
	}
	if !errors.Is(err, node.ErrNodeNotFound) {
		return nil, fmt.Errorf("failed to get node: %w", err)
	}

	n, err = service.GetNode(ctx, idOrName)
	if err == nil {
		return n, nil
	}
	if !errors.Is(err, node.ErrNodeNotFound) {
		return nil, fmt.Errorf("failed to get node: %w", err)
	}

	nodes, err := service.ListNodes(ctx, nil)
	if err != nil {
		return nil, fmt.Errorf("failed to list nodes: %w", err)
	}

	matches := matchNodes(nodes, idOrName)
	if len(matches) == 1 {
		return matches[0], nil
	}
	if len(matches) > 1 {
		return nil, fmt.Errorf("node '%s' is ambiguous; matches: %s (use a longer prefix or full ID)", idOrName, formatNodeMatches(matches))
	}
	if len(nodes) == 0 {
		return nil, fmt.Errorf("node '%s' not found (no nodes registered yet)", idOrName)
	}

	example := fmt.Sprintf("Example input: '%s' or '%s'", nodes[0].Name, shortID(nodes[0].ID))
	return nil, fmt.Errorf("node '%s' not found. %s", idOrName, example)
}

func findWorkspace(ctx context.Context, repo *db.WorkspaceRepository, idOrName string) (*models.Workspace, error) {
	if strings.TrimSpace(idOrName) == "" {
		return nil, errors.New("workspace name or ID required")
	}

	ws, err := repo.GetByName(ctx, idOrName)
	if err == nil {
		return ws, nil
	}
	if !errors.Is(err, db.ErrWorkspaceNotFound) {
		return nil, fmt.Errorf("failed to get workspace: %w", err)
	}

	ws, err = repo.Get(ctx, idOrName)
	if err == nil {
		return ws, nil
	}
	if !errors.Is(err, db.ErrWorkspaceNotFound) {
		return nil, fmt.Errorf("failed to get workspace: %w", err)
	}

	workspaces, err := repo.List(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to list workspaces: %w", err)
	}

	matches := matchWorkspaces(workspaces, idOrName)
	if len(matches) == 1 {
		return matches[0], nil
	}
	if len(matches) > 1 {
		return nil, fmt.Errorf("workspace '%s' is ambiguous; matches: %s (use a longer prefix or full ID)", idOrName, formatWorkspaceMatches(matches))
	}
	if len(workspaces) == 0 {
		return nil, fmt.Errorf("workspace '%s' not found (no workspaces registered yet)", idOrName)
	}

	example := fmt.Sprintf("Example input: '%s' or '%s'", workspaces[0].Name, shortID(workspaces[0].ID))
	return nil, fmt.Errorf("workspace '%s' not found. %s", idOrName, example)
}

func findAgent(ctx context.Context, repo *db.AgentRepository, idOrPrefix string) (*models.Agent, error) {
	if strings.TrimSpace(idOrPrefix) == "" {
		return nil, errors.New("agent ID required")
	}

	agent, err := repo.Get(ctx, idOrPrefix)
	if err == nil {
		return agent, nil
	}
	if !errors.Is(err, db.ErrAgentNotFound) {
		return nil, fmt.Errorf("failed to get agent: %w", err)
	}

	agents, err := repo.List(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to list agents: %w", err)
	}

	matches := matchAgents(agents, idOrPrefix)
	if len(matches) == 1 {
		return matches[0], nil
	}
	if len(matches) > 1 {
		return nil, fmt.Errorf("agent '%s' is ambiguous; matches: %s (use a longer prefix or full ID)", idOrPrefix, formatAgentMatches(matches))
	}
	if len(agents) == 0 {
		return nil, fmt.Errorf("agent '%s' not found (no agents registered yet)", idOrPrefix)
	}

	example := fmt.Sprintf("Example input: '%s'", shortID(agents[0].ID))
	return nil, fmt.Errorf("agent '%s' not found. %s", idOrPrefix, example)
}

func findAccount(ctx context.Context, repo *db.AccountRepository, idOrProfile string) (*models.Account, error) {
	if strings.TrimSpace(idOrProfile) == "" {
		return nil, errors.New("account ID or profile name required")
	}

	account, err := repo.Get(ctx, idOrProfile)
	if err == nil {
		return account, nil
	}
	if !errors.Is(err, db.ErrAccountNotFound) {
		return nil, fmt.Errorf("failed to get account: %w", err)
	}

	accounts, err := repo.List(ctx, nil)
	if err != nil {
		return nil, fmt.Errorf("failed to list accounts: %w", err)
	}

	matches := matchAccounts(accounts, idOrProfile)
	if len(matches) == 1 {
		return matches[0], nil
	}
	if len(matches) > 1 {
		return nil, fmt.Errorf("account '%s' is ambiguous; matches: %s (use a longer prefix or full ID)", idOrProfile, formatAccountMatches(matches))
	}
	if len(accounts) == 0 {
		return nil, fmt.Errorf("account '%s' not found (no accounts configured yet)", idOrProfile)
	}

	example := fmt.Sprintf("Example input: '%s' or '%s'", accounts[0].ProfileName, shortID(accounts[0].ID))
	return nil, fmt.Errorf("account '%s' not found. %s", idOrProfile, example)
}

func matchNodes(nodes []*models.Node, query string) []*models.Node {
	normalized := strings.ToLower(strings.TrimSpace(query))
	if normalized == "" {
		return nil
	}

	matches := make([]*models.Node, 0)
	seen := make(map[string]struct{})

	for _, n := range nodes {
		if n == nil {
			continue
		}
		if strings.HasPrefix(n.ID, query) {
			if _, ok := seen[n.ID]; !ok {
				matches = append(matches, n)
				seen[n.ID] = struct{}{}
			}
			continue
		}
		name := strings.ToLower(n.Name)
		if strings.HasPrefix(name, normalized) || (len(normalized) >= 3 && strings.Contains(name, normalized)) {
			if _, ok := seen[n.ID]; !ok {
				matches = append(matches, n)
				seen[n.ID] = struct{}{}
			}
		}
	}

	sort.Slice(matches, func(i, j int) bool {
		left := strings.ToLower(matches[i].Name)
		right := strings.ToLower(matches[j].Name)
		if left == right {
			return matches[i].ID < matches[j].ID
		}
		return left < right
	})

	return matches
}

func matchWorkspaces(workspaces []*models.Workspace, query string) []*models.Workspace {
	normalized := strings.ToLower(strings.TrimSpace(query))
	if normalized == "" {
		return nil
	}

	matches := make([]*models.Workspace, 0)
	seen := make(map[string]struct{})

	for _, ws := range workspaces {
		if ws == nil {
			continue
		}
		if strings.HasPrefix(ws.ID, query) {
			if _, ok := seen[ws.ID]; !ok {
				matches = append(matches, ws)
				seen[ws.ID] = struct{}{}
			}
			continue
		}
		name := strings.ToLower(ws.Name)
		if strings.HasPrefix(name, normalized) || (len(normalized) >= 3 && strings.Contains(name, normalized)) {
			if _, ok := seen[ws.ID]; !ok {
				matches = append(matches, ws)
				seen[ws.ID] = struct{}{}
			}
		}
	}

	sort.Slice(matches, func(i, j int) bool {
		left := strings.ToLower(matches[i].Name)
		right := strings.ToLower(matches[j].Name)
		if left == right {
			return matches[i].ID < matches[j].ID
		}
		return left < right
	})

	return matches
}

func matchAccounts(accounts []*models.Account, query string) []*models.Account {
	normalized := strings.ToLower(strings.TrimSpace(query))
	if normalized == "" {
		return nil
	}

	matches := make([]*models.Account, 0)
	seen := make(map[string]struct{})

	for _, account := range accounts {
		if account == nil {
			continue
		}
		if strings.HasPrefix(account.ID, query) {
			if _, ok := seen[account.ID]; !ok {
				matches = append(matches, account)
				seen[account.ID] = struct{}{}
			}
			continue
		}
		name := strings.ToLower(account.ProfileName)
		if strings.HasPrefix(name, normalized) || (len(normalized) >= 3 && strings.Contains(name, normalized)) {
			if _, ok := seen[account.ID]; !ok {
				matches = append(matches, account)
				seen[account.ID] = struct{}{}
			}
		}
	}

	sort.Slice(matches, func(i, j int) bool {
		left := strings.ToLower(matches[i].ProfileName)
		right := strings.ToLower(matches[j].ProfileName)
		if left == right {
			return matches[i].ID < matches[j].ID
		}
		return left < right
	})

	return matches
}

func matchAgents(agents []*models.Agent, query string) []*models.Agent {
	normalized := strings.TrimSpace(query)
	if normalized == "" {
		return nil
	}

	matches := make([]*models.Agent, 0)
	for _, agent := range agents {
		if agent == nil {
			continue
		}
		if strings.HasPrefix(agent.ID, normalized) {
			matches = append(matches, agent)
		}
	}

	sort.Slice(matches, func(i, j int) bool {
		return matches[i].ID < matches[j].ID
	})

	return matches
}

func formatNodeMatches(nodes []*models.Node) string {
	return formatMatchList(len(nodes), func(i int) string {
		node := nodes[i]
		return fmt.Sprintf("%s (%s)", node.Name, shortID(node.ID))
	})
}

func formatWorkspaceMatches(workspaces []*models.Workspace) string {
	return formatMatchList(len(workspaces), func(i int) string {
		ws := workspaces[i]
		return fmt.Sprintf("%s (%s)", ws.Name, shortID(ws.ID))
	})
}

func formatAgentMatches(agents []*models.Agent) string {
	return formatMatchList(len(agents), func(i int) string {
		agent := agents[i]
		descriptor := fmt.Sprintf("%s (%s", shortID(agent.ID), agent.Type)
		if agent.WorkspaceID != "" {
			descriptor += fmt.Sprintf(", ws %s", shortID(agent.WorkspaceID))
		}
		descriptor += ")"
		return descriptor
	})
}

func formatAccountMatches(accounts []*models.Account) string {
	return formatMatchList(len(accounts), func(i int) string {
		account := accounts[i]
		return fmt.Sprintf("%s (%s)", account.ProfileName, shortID(account.ID))
	})
}

func formatMatchList(count int, format func(int) string) string {
	if count == 0 {
		return "none"
	}

	limit := count
	if limit > maxSuggestions {
		limit = maxSuggestions
	}

	parts := make([]string, 0, limit+1)
	for i := 0; i < limit; i++ {
		parts = append(parts, format(i))
	}
	if count > maxSuggestions {
		parts = append(parts, fmt.Sprintf("... and %d more", count-maxSuggestions))
	}

	return strings.Join(parts, ", ")
}

// ResolvedContext holds the result of context resolution.
type ResolvedContext struct {
	WorkspaceID   string
	WorkspaceName string
	AgentID       string
	AgentName     string
	Source        string // "flag", "directory", "stored", or ""
}

// ResolveWorkspaceContext resolves a workspace using the priority order:
// 1. Explicit flag (if provided)
// 2. Current directory detection (if in a git repo matching a workspace)
// 3. Stored context from `swarm use`
func ResolveWorkspaceContext(ctx context.Context, repo *db.WorkspaceRepository, explicitFlag string) (*ResolvedContext, error) {
	result := &ResolvedContext{}

	// Priority 1: Explicit flag
	if explicitFlag != "" {
		ws, err := findWorkspace(ctx, repo, explicitFlag)
		if err != nil {
			return nil, err
		}
		result.WorkspaceID = ws.ID
		result.WorkspaceName = ws.Name
		result.Source = "flag"
		return result, nil
	}

	// Priority 2: Current directory detection
	if ws := detectWorkspaceFromCwd(ctx, repo); ws != nil {
		result.WorkspaceID = ws.ID
		result.WorkspaceName = ws.Name
		result.Source = "directory"
		return result, nil
	}

	// Priority 3: Stored context
	store := config.DefaultContextStore()
	storedCtx, err := store.Load()
	if err == nil && storedCtx.HasWorkspace() {
		// Verify the workspace still exists
		ws, err := repo.Get(ctx, storedCtx.WorkspaceID)
		if err == nil {
			result.WorkspaceID = ws.ID
			result.WorkspaceName = ws.Name
			result.Source = "stored"
			return result, nil
		}
		// Workspace no longer exists, ignore stored context
	}

	return result, nil
}

// ResolveAgentContext resolves an agent using the priority order:
// 1. Explicit flag (if provided)
// 2. Stored context from `swarm use`
func ResolveAgentContext(ctx context.Context, repo *db.AgentRepository, explicitFlag string, workspaceID string) (*ResolvedContext, error) {
	result := &ResolvedContext{}

	// Priority 1: Explicit flag
	if explicitFlag != "" {
		agent, err := findAgent(ctx, repo, explicitFlag)
		if err != nil {
			return nil, err
		}
		// Verify agent belongs to workspace if specified
		if workspaceID != "" && agent.WorkspaceID != workspaceID {
			return nil, fmt.Errorf("agent %s does not belong to workspace %s", explicitFlag, workspaceID)
		}
		result.AgentID = agent.ID
		result.Source = "flag"
		return result, nil
	}

	// Priority 2: Stored context
	store := config.DefaultContextStore()
	storedCtx, err := store.Load()
	if err == nil && storedCtx.HasAgent() {
		// Verify the agent still exists
		agent, err := repo.Get(ctx, storedCtx.AgentID)
		if err == nil {
			// If workspace is specified, verify agent belongs to it
			if workspaceID != "" && agent.WorkspaceID != workspaceID {
				// Agent doesn't match workspace, ignore stored context
				return result, nil
			}
			result.AgentID = agent.ID
			result.AgentName = storedCtx.AgentName
			result.Source = "stored"
			return result, nil
		}
		// Agent no longer exists, ignore stored context
	}

	return result, nil
}

// RequireWorkspaceContext is like ResolveWorkspaceContext but returns an error
// if no workspace could be resolved.
func RequireWorkspaceContext(ctx context.Context, repo *db.WorkspaceRepository, explicitFlag string) (*ResolvedContext, error) {
	resolved, err := ResolveWorkspaceContext(ctx, repo, explicitFlag)
	if err != nil {
		return nil, err
	}
	if resolved.WorkspaceID == "" {
		return nil, errors.New("workspace required: use --workspace flag, run from a workspace directory, or set context with 'swarm use <workspace>'")
	}
	return resolved, nil
}

// RequireAgentContext is like ResolveAgentContext but returns an error
// if no agent could be resolved.
func RequireAgentContext(ctx context.Context, repo *db.AgentRepository, explicitFlag string, workspaceID string) (*ResolvedContext, error) {
	resolved, err := ResolveAgentContext(ctx, repo, explicitFlag, workspaceID)
	if err != nil {
		return nil, err
	}
	if resolved.AgentID == "" {
		return nil, errors.New("agent required: provide agent ID as argument or set context with 'swarm use --agent <agent>'")
	}
	return resolved, nil
}

// detectWorkspaceFromCwd attempts to find a workspace that matches the current
// working directory by looking for a git repository root.
func detectWorkspaceFromCwd(ctx context.Context, repo *db.WorkspaceRepository) *models.Workspace {
	cwd, err := os.Getwd()
	if err != nil {
		return nil
	}

	// Get git root directory
	gitRoot := getGitRoot(cwd)
	if gitRoot == "" {
		return nil
	}

	// Normalize path
	gitRoot, err = filepath.Abs(gitRoot)
	if err != nil {
		return nil
	}

	// Look for a workspace with matching repo path
	workspaces, err := repo.List(ctx)
	if err != nil {
		return nil
	}

	for _, ws := range workspaces {
		if ws.RepoPath == "" {
			continue
		}
		wsPath, err := filepath.Abs(ws.RepoPath)
		if err != nil {
			continue
		}
		if wsPath == gitRoot {
			return ws
		}
	}

	return nil
}

// getGitRoot returns the root directory of the git repository containing the
// given path, or empty string if not in a git repo.
func getGitRoot(path string) string {
	cmd := exec.Command("git", "rev-parse", "--show-toplevel")
	cmd.Dir = path
	out, err := cmd.Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}
