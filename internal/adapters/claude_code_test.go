package adapters

import (
	"testing"

	"github.com/opencode-ai/swarm/internal/models"
)

func TestClaudeCodeAdapter_Tier(t *testing.T) {
	adapter := NewClaudeCodeAdapter()
	if adapter.Tier() != models.AdapterTierTelemetry {
		t.Errorf("expected AdapterTierTelemetry, got %v", adapter.Tier())
	}
}

func TestClaudeCodeAdapter_SupportsApprovals(t *testing.T) {
	adapter := NewClaudeCodeAdapter()
	if !adapter.SupportsApprovals() {
		t.Fatal("expected SupportsApprovals to be true")
	}
}

func TestClaudeCodeAdapter_DetectReady_StreamInit(t *testing.T) {
	adapter := NewClaudeCodeAdapter()

	ready, err := adapter.DetectReady("{\"type\":\"system\",\"subtype\":\"init\",\"permissionMode\":\"default\"}")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !ready {
		t.Fatalf("expected ready from stream-json init")
	}
}

func TestClaudeCodeAdapter_DetectState_StreamInit(t *testing.T) {
	adapter := NewClaudeCodeAdapter()

	state, reason, err := adapter.DetectState("{\"type\":\"system\",\"subtype\":\"init\"}", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if state != models.AgentStateIdle {
		t.Fatalf("expected idle state, got %v (reason: %s)", state, reason.Reason)
	}
}

func TestClaudeCodeAdapter_DetectState_PermissionEvent(t *testing.T) {
	adapter := NewClaudeCodeAdapter()

	state, reason, err := adapter.DetectState("{\"type\":\"permission\",\"subtype\":\"request\"}", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if state != models.AgentStateAwaitingApproval {
		t.Fatalf("expected awaiting approval, got %v (reason: %s)", state, reason.Reason)
	}
}

func TestClaudeCodeAdapter_DetectState_FallbackPrompt(t *testing.T) {
	adapter := NewClaudeCodeAdapter()

	state, reason, err := adapter.DetectState("claude> ", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if state != models.AgentStateIdle {
		t.Fatalf("expected idle state, got %v (reason: %s)", state, reason.Reason)
	}
}

func TestClaudeCodeAdapter_SpawnCommandPermissive(t *testing.T) {
	adapter := NewClaudeCodeAdapter()

	cmd, args := adapter.SpawnCommand(SpawnOptions{ApprovalPolicy: "permissive"})
	if cmd != "claude" {
		t.Fatalf("expected command claude, got %q", cmd)
	}

	if !containsArgsPair(args, "--permission-mode", "dontAsk") {
		t.Fatalf("expected --permission-mode dontAsk in args, got %v", args)
	}
}

func containsArgsPair(args []string, flag, value string) bool {
	for i := 0; i+1 < len(args); i++ {
		if args[i] == flag && args[i+1] == value {
			return true
		}
	}
	return false
}
