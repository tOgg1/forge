package adapters

import (
	"strings"

	"github.com/opencode-ai/swarm/internal/models"
)

// geminiAdapter provides Gemini CLI-specific state detection and spawn options.
type geminiAdapter struct {
	*GenericAdapter
}

// NewGeminiAdapter creates a Gemini adapter with tuned indicators.
func NewGeminiAdapter() *geminiAdapter {
	base := NewGenericAdapter(
		string(models.AgentTypeGemini),
		"gemini",
		WithIdleIndicators(
			"gemini>",
			">",
			"❯",
			"waiting for input",
			"ready",
		),
		WithBusyIndicators(
			"thinking",
			"working",
			"processing",
			"generating",
			"executing",
			"running",
			"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", // spinner chars
		),
	)

	return &geminiAdapter{GenericAdapter: base}
}

// Tier returns the adapter integration tier.
func (a *geminiAdapter) Tier() models.AdapterTier {
	return models.AdapterTierGeneric
}

// SpawnCommand returns the command and args to launch Gemini CLI.
func (a *geminiAdapter) SpawnCommand(opts SpawnOptions) (cmd string, args []string) {
	cmd = "gemini"
	args = []string{}

	// Handle approval policy using --approval-mode
	if opts.ApprovalPolicy != "" {
		switch strings.ToLower(strings.TrimSpace(opts.ApprovalPolicy)) {
		case "permissive":
			// Use yolo mode for permissive (auto-approve all tools)
			args = append(args, "--approval-mode", "yolo")
		case "strict":
			// Use default mode for strict (prompt for approval)
			args = append(args, "--approval-mode", "default")
		default:
			// Default: auto-approve edits but ask for other tools
			args = append(args, "--approval-mode", "auto_edit")
		}
	}

	return cmd, args
}

// DetectReady reports whether the agent is ready based on screen output.
func (a *geminiAdapter) DetectReady(screen string) (bool, error) {
	lower := strings.ToLower(screen)

	// Gemini-specific ready patterns
	if strings.Contains(lower, "gemini>") || strings.Contains(screen, "❯") {
		return true, nil
	}

	// Check for session start indicators
	if strings.Contains(lower, "session started") || strings.Contains(lower, "ready") {
		return true, nil
	}

	// Check for interactive mode prompt
	if strings.Contains(lower, "enter your prompt") || strings.Contains(lower, "type your message") {
		return true, nil
	}

	return a.GenericAdapter.DetectReady(screen)
}

// DetectState returns the current state with a reason.
func (a *geminiAdapter) DetectState(screen string, meta any) (models.AgentState, StateReason, error) {
	lower := strings.ToLower(screen)

	// Check for Gemini-specific approval patterns
	approvalPatterns := []string{
		"do you want to proceed",
		"approve this action",
		"allow this operation",
		"confirm execution",
		"run this command",
		"execute?",
		"[y/n]",
		"(y/n)",
		"press y to confirm",
	}
	for _, pattern := range approvalPatterns {
		if strings.Contains(lower, pattern) {
			return models.AgentStateAwaitingApproval, StateReason{
				Reason:     "gemini approval prompt detected",
				Confidence: models.StateConfidenceMedium,
				Evidence:   []string{pattern},
			}, nil
		}
	}

	// Check for tool execution approval (Gemini-specific)
	if strings.Contains(lower, "tool") && (strings.Contains(lower, "approve") || strings.Contains(lower, "allow") || strings.Contains(lower, "confirm")) {
		return models.AgentStateAwaitingApproval, StateReason{
			Reason:     "gemini tool approval detected",
			Confidence: models.StateConfidenceMedium,
			Evidence:   []string{"tool approval"},
		}, nil
	}

	// Check for edit approval (Gemini has auto_edit mode)
	if strings.Contains(lower, "edit") && (strings.Contains(lower, "approve") || strings.Contains(lower, "allow") || strings.Contains(lower, "confirm")) {
		return models.AgentStateAwaitingApproval, StateReason{
			Reason:     "gemini edit approval detected",
			Confidence: models.StateConfidenceMedium,
			Evidence:   []string{"edit approval"},
		}, nil
	}

	// Delegate to generic detection for other states
	return a.GenericAdapter.DetectState(screen, meta)
}

// SupportsApprovals indicates if the adapter supports approvals routing.
func (a *geminiAdapter) SupportsApprovals() bool {
	return true
}

// Ensure geminiAdapter implements AgentAdapter
var _ AgentAdapter = (*geminiAdapter)(nil)
