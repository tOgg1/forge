// Package cli provides actionable next-step hints for CLI commands.
package cli

import (
	"fmt"
	"os"
)

// HintContext provides context for generating relevant next steps.
type HintContext struct {
	// Action is the command that was executed (e.g., "up", "send", "spawn")
	Action string

	// AgentID is the agent involved (if any)
	AgentID string

	// AgentIDs is a list of agents involved (for multi-agent operations)
	AgentIDs []string

	// WorkspaceID is the workspace involved (if any)
	WorkspaceID string

	// WorkspaceName is the workspace name (for display)
	WorkspaceName string

	// TmuxSession is the tmux session name (if any)
	TmuxSession string

	// QueuePosition is the position in queue (for send operations)
	QueuePosition int

	// Extra is any additional context
	Extra map[string]any
}

// PrintNextSteps prints contextual next steps after a successful command.
// Does nothing if JSON output is enabled.
func PrintNextSteps(ctx HintContext) {
	if IsJSONOutput() || IsJSONLOutput() {
		return
	}

	hints := generateHints(ctx)
	if len(hints) == 0 {
		return
	}

	fmt.Fprintln(os.Stdout)
	fmt.Fprintln(os.Stdout, "Next steps:")
	for _, hint := range hints {
		fmt.Fprintf(os.Stdout, "  %s\n", hint)
	}
}

// generateHints generates context-aware hints for the given action.
func generateHints(ctx HintContext) []string {
	switch ctx.Action {
	case "up":
		return hintsForUp(ctx)
	case "send":
		return hintsForSend(ctx)
	case "spawn":
		return hintsForSpawn(ctx)
	case "queue":
		return hintsForQueue(ctx)
	case "workspace_create":
		return hintsForWorkspaceCreate(ctx)
	case "agent_terminate":
		return hintsForAgentTerminate(ctx)
	default:
		return nil
	}
}

func hintsForUp(ctx HintContext) []string {
	hints := make([]string, 0, 4)

	// Use first agent ID if available
	agentID := ctx.AgentID
	if agentID == "" && len(ctx.AgentIDs) > 0 {
		agentID = ctx.AgentIDs[0]
	}

	if agentID != "" {
		hints = append(hints,
			fmt.Sprintf("swarm send %s \"your task here\"   # Send instructions", shortID(agentID)),
			fmt.Sprintf("swarm attach %s                   # Watch agent work", shortID(agentID)),
			fmt.Sprintf("swarm log %s                      # View transcript", shortID(agentID)),
		)
	}

	hints = append(hints, "swarm ps                            # List all agents")

	return hints
}

func hintsForSend(ctx HintContext) []string {
	hints := make([]string, 0, 3)

	agentID := ctx.AgentID
	if agentID == "" && len(ctx.AgentIDs) > 0 {
		agentID = ctx.AgentIDs[0]
	}

	if agentID != "" {
		hints = append(hints,
			fmt.Sprintf("swarm queue ls --agent %s        # View queue", shortID(agentID)),
			fmt.Sprintf("swarm log %s --follow            # Watch output", shortID(agentID)),
			fmt.Sprintf("swarm explain %s                  # Check status", shortID(agentID)),
		)
	}

	return hints
}

func hintsForSpawn(ctx HintContext) []string {
	hints := make([]string, 0, 3)

	agentID := ctx.AgentID
	if agentID == "" && len(ctx.AgentIDs) > 0 {
		agentID = ctx.AgentIDs[0]
	}

	if agentID != "" {
		hints = append(hints,
			fmt.Sprintf("swarm send %s \"your task\"        # Send instructions", shortID(agentID)),
			fmt.Sprintf("swarm attach %s                   # Watch agent", shortID(agentID)),
			fmt.Sprintf("swarm agent status %s            # Check status", shortID(agentID)),
		)
	}

	return hints
}

func hintsForQueue(ctx HintContext) []string {
	hints := make([]string, 0, 3)

	agentID := ctx.AgentID
	if agentID == "" && len(ctx.AgentIDs) > 0 {
		agentID = ctx.AgentIDs[0]
	}

	if agentID != "" {
		hints = append(hints,
			fmt.Sprintf("swarm queue ls --agent %s        # View full queue", shortID(agentID)),
			fmt.Sprintf("swarm explain %s                  # Check dispatch status", shortID(agentID)),
		)
	}

	hints = append(hints, "swarm queue ls                      # View all queues")

	return hints
}

func hintsForWorkspaceCreate(ctx HintContext) []string {
	hints := make([]string, 0, 4)

	wsName := ctx.WorkspaceName
	if wsName == "" {
		wsName = shortID(ctx.WorkspaceID)
	}

	hints = append(hints,
		fmt.Sprintf("swarm agent spawn -w %s           # Spawn an agent", wsName),
		fmt.Sprintf("swarm use %s                       # Set as current context", wsName),
	)

	if ctx.TmuxSession != "" {
		hints = append(hints,
			fmt.Sprintf("tmux attach -t %s              # Attach to session", ctx.TmuxSession),
		)
	}

	hints = append(hints, "swarm ls                            # List all workspaces")

	return hints
}

func hintsForAgentTerminate(ctx HintContext) []string {
	hints := make([]string, 0, 2)

	if ctx.WorkspaceID != "" {
		hints = append(hints,
			"swarm ps                            # List remaining agents",
			fmt.Sprintf("swarm agent spawn -w %s        # Spawn a new agent", shortID(ctx.WorkspaceID)),
		)
	} else {
		hints = append(hints,
			"swarm ps                            # List remaining agents",
		)
	}

	return hints
}
