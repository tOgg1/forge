package harness

import (
	"context"
	"strings"
	"testing"

	"github.com/tOgg1/forge/internal/models"
)

func TestBuildExecutionEnvMode(t *testing.T) {
	profile := models.Profile{
		Name:            "claude",
		Harness:         models.HarnessClaude,
		PromptMode:      models.PromptModeEnv,
		CommandTemplate: "claude -p \"$FORGE_PROMPT_CONTENT\"",
	}

	exec, err := BuildExecution(context.Background(), profile, "", "hello")
	if err != nil {
		t.Fatalf("BuildExecution failed: %v", err)
	}

	found := false
	for _, value := range exec.Env {
		if value == "FORGE_PROMPT_CONTENT=hello" {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("expected FORGE_PROMPT_CONTENT env to be set")
	}
}

func TestBuildExecutionPathMode(t *testing.T) {
	profile := models.Profile{
		Name:            "pi",
		Harness:         models.HarnessPi,
		PromptMode:      models.PromptModePath,
		AuthHome:        "/tmp/pi",
		CommandTemplate: "pi -p \"{prompt}\"",
	}

	exec, err := BuildExecution(context.Background(), profile, "/repo/PROMPT.md", "")
	if err != nil {
		t.Fatalf("BuildExecution failed: %v", err)
	}

	command := strings.Join(exec.Cmd.Args, " ")
	if !strings.Contains(command, "/repo/PROMPT.md") {
		t.Fatalf("expected prompt path in command, got %s", command)
	}

	found := false
	for _, value := range exec.Env {
		if value == "PI_CODING_AGENT_DIR=/tmp/pi" {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("expected PI_CODING_AGENT_DIR env to be set")
	}
}

func TestBuildExecutionClaudeWithAuthHome(t *testing.T) {
	profile := models.Profile{
		Name:            "cc1",
		Harness:         models.HarnessClaude,
		PromptMode:      models.PromptModeEnv,
		AuthHome:        "/tmp/claude-1",
		CommandTemplate: "script -q -c 'claude -p \"$FORGE_PROMPT_CONTENT\" --dangerously-skip-permissions' /dev/null",
	}

	exec, err := BuildExecution(context.Background(), profile, "", "test prompt")
	if err != nil {
		t.Fatalf("BuildExecution failed: %v", err)
	}

	command := strings.Join(exec.Cmd.Args, " ")
	if !strings.Contains(command, "claude -p") {
		t.Fatalf("expected claude command, got %s", command)
	}
	if !strings.Contains(command, "--dangerously-skip-permissions") {
		t.Fatalf("expected --dangerously-skip-permissions flag, got %s", command)
	}

	foundConfigDir := false
	foundPrompt := false
	for _, value := range exec.Env {
		if value == "HOME=/tmp/claude-1" {
			t.Fatalf("HOME should not be set for Claude harness (breaks tilde expansion)")
		}
		if value == "CLAUDE_CONFIG_DIR=/tmp/claude-1" {
			foundConfigDir = true
		}
		if value == "FORGE_PROMPT_CONTENT=test prompt" {
			foundPrompt = true
		}
	}
	if !foundConfigDir {
		t.Fatalf("expected CLAUDE_CONFIG_DIR env to be set to AuthHome")
	}
	if !foundPrompt {
		t.Fatalf("expected FORGE_PROMPT_CONTENT env to be set")
	}
}

func TestBuildExecutionClaudeWithExtraArgs(t *testing.T) {
	profile := models.Profile{
		Name:            "claude-custom",
		Harness:         models.HarnessClaude,
		PromptMode:      models.PromptModeEnv,
		CommandTemplate: "claude -p \"$FORGE_PROMPT_CONTENT\"",
		ExtraArgs:       []string{"--dangerously-skip-permissions", "--verbose"},
	}

	exec, err := BuildExecution(context.Background(), profile, "", "hello")
	if err != nil {
		t.Fatalf("BuildExecution failed: %v", err)
	}

	command := strings.Join(exec.Cmd.Args, " ")
	if !strings.Contains(command, "--dangerously-skip-permissions") {
		t.Fatalf("expected extra args in command, got %s", command)
	}
	if !strings.Contains(command, "--verbose") {
		t.Fatalf("expected --verbose in command, got %s", command)
	}
}

func TestBuildExecutionStdinMode(t *testing.T) {
	profile := models.Profile{
		Name:            "codex",
		Harness:         models.HarnessCodex,
		PromptMode:      models.PromptModeStdin,
		CommandTemplate: "codex exec --full-auto -",
	}

	exec, err := BuildExecution(context.Background(), profile, "", "prompt")
	if err != nil {
		t.Fatalf("BuildExecution failed: %v", err)
	}

	if exec.Stdin == nil {
		t.Fatalf("expected stdin to be set")
	}
}
