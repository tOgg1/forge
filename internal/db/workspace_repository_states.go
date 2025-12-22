// Package db provides SQLite database access for Swarm.
package db

import (
	"context"
	"fmt"

	"github.com/opencode-ai/swarm/internal/models"
)

// GetAgentStateCounts returns counts of agents by state for a workspace.
func (r *WorkspaceRepository) GetAgentStateCounts(ctx context.Context, workspaceID string) (map[models.AgentState]int, error) {
	rows, err := r.db.QueryContext(ctx, `
		SELECT state, COUNT(*)
		FROM agents
		WHERE workspace_id = ?
		GROUP BY state
	`, workspaceID)
	if err != nil {
		return nil, fmt.Errorf("failed to query agent state counts: %w", err)
	}
	defer rows.Close()

	counts := make(map[models.AgentState]int)
	for rows.Next() {
		var state string
		var count int
		if err := rows.Scan(&state, &count); err != nil {
			return nil, fmt.Errorf("failed to scan agent state count: %w", err)
		}
		counts[models.AgentState(state)] = count
	}

	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating agent state counts: %w", err)
	}

	return counts, nil
}
