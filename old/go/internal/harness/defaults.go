package harness

import "github.com/tOgg1/forge/internal/models"

// DefaultCommandTemplate returns the default command template for a harness.
func DefaultCommandTemplate(harness models.Harness, model string) string {
	switch harness {
	case models.HarnessPi:
		return "pi -p \"$FORGE_PROMPT_CONTENT\""
	case models.HarnessClaude:
		// Use script to create a PTY so Claude streams output in real-time
		return "script -q -c 'claude -p \"$FORGE_PROMPT_CONTENT\" --dangerously-skip-permissions' /dev/null"
	case models.HarnessCodex:
		return "codex exec --dangerously-bypass-approvals-and-sandbox -"
	case models.HarnessOpenCode:
		if model == "" {
			model = "anthropic/claude-opus-4-5"
		}
		return "opencode run --model " + model + " \"$FORGE_PROMPT_CONTENT\""
	case models.HarnessDroid:
		// Droid reads from stdin when no prompt argument is provided
		return "droid exec --skip-permissions-unsafe"
	default:
		return ""
	}
}

// DefaultPromptMode returns the default prompt mode for a harness.
func DefaultPromptMode(harness models.Harness) models.PromptMode {
	switch harness {
	case models.HarnessPi:
		return models.PromptModeEnv
	case models.HarnessCodex, models.HarnessDroid:
		return models.PromptModeStdin
	case models.HarnessClaude, models.HarnessOpenCode:
		return models.PromptModeEnv
	default:
		return models.PromptModeEnv
	}
}
