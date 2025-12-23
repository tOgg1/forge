package adapters

import (
	"testing"

	"github.com/opencode-ai/swarm/internal/models"
)

func TestNewGeminiAdapter(t *testing.T) {
	adapter := NewGeminiAdapter()

	if adapter.Name() != string(models.AgentTypeGemini) {
		t.Errorf("expected name %q, got %q", models.AgentTypeGemini, adapter.Name())
	}

	if adapter.Tier() != models.AdapterTierGeneric {
		t.Errorf("expected tier %v, got %v", models.AdapterTierGeneric, adapter.Tier())
	}
}

func TestGeminiAdapter_SpawnCommand(t *testing.T) {
	adapter := NewGeminiAdapter()

	tests := []struct {
		name           string
		approvalPolicy string
		wantArgs       []string
	}{
		{
			name:           "no approval policy",
			approvalPolicy: "",
			wantArgs:       []string{},
		},
		{
			name:           "permissive approval policy",
			approvalPolicy: "permissive",
			wantArgs:       []string{"--approval-mode", "yolo"},
		},
		{
			name:           "strict approval policy",
			approvalPolicy: "strict",
			wantArgs:       []string{"--approval-mode", "default"},
		},
		{
			name:           "default approval policy",
			approvalPolicy: "default",
			wantArgs:       []string{"--approval-mode", "auto_edit"},
		},
		{
			name:           "case insensitive permissive",
			approvalPolicy: "PERMISSIVE",
			wantArgs:       []string{"--approval-mode", "yolo"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			opts := SpawnOptions{ApprovalPolicy: tt.approvalPolicy}
			cmd, args := adapter.SpawnCommand(opts)

			if cmd != "gemini" {
				t.Errorf("expected command %q, got %q", "gemini", cmd)
			}

			if len(args) != len(tt.wantArgs) {
				t.Errorf("expected %d args, got %d: %v", len(tt.wantArgs), len(args), args)
				return
			}

			for i, arg := range args {
				if arg != tt.wantArgs[i] {
					t.Errorf("arg[%d]: expected %q, got %q", i, tt.wantArgs[i], arg)
				}
			}
		})
	}
}

func TestGeminiAdapter_DetectReady(t *testing.T) {
	adapter := NewGeminiAdapter()

	tests := []struct {
		name   string
		screen string
		want   bool
	}{
		{
			name:   "gemini prompt",
			screen: "gemini>",
			want:   true,
		},
		{
			name:   "unicode prompt",
			screen: "Welcome to Gemini\n❯",
			want:   true,
		},
		{
			name:   "session started",
			screen: "Session started, ready for input",
			want:   true,
		},
		{
			name:   "enter your prompt",
			screen: "Enter your prompt:",
			want:   true,
		},
		{
			name:   "processing spinner",
			screen: "⠋ Thinking...",
			want:   false,
		},
		{
			name:   "empty screen",
			screen: "",
			want:   false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := adapter.DetectReady(tt.screen)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tt.want {
				t.Errorf("DetectReady() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestGeminiAdapter_DetectState(t *testing.T) {
	adapter := NewGeminiAdapter()

	tests := []struct {
		name      string
		screen    string
		wantState models.AgentState
	}{
		{
			name:      "approval prompt y/n",
			screen:    "Do you want to proceed? [y/n]",
			wantState: models.AgentStateAwaitingApproval,
		},
		{
			name:      "tool approval",
			screen:    "This tool requires approval. Allow?",
			wantState: models.AgentStateAwaitingApproval,
		},
		{
			name:      "edit approval",
			screen:    "Confirm this edit? Approve the changes.",
			wantState: models.AgentStateAwaitingApproval,
		},
		{
			name:      "execute prompt",
			screen:    "Run this command? Execute? (y/n)",
			wantState: models.AgentStateAwaitingApproval,
		},
		{
			name:      "press y to confirm",
			screen:    "Press y to confirm this action",
			wantState: models.AgentStateAwaitingApproval,
		},
		{
			name:      "idle at prompt",
			screen:    "gemini>",
			wantState: models.AgentStateIdle,
		},
		{
			name:      "working with spinner",
			screen:    "⠋ Processing request...",
			wantState: models.AgentStateWorking,
		},
		{
			name:      "error state",
			screen:    "Error: Connection failed",
			wantState: models.AgentStateError,
		},
		{
			name:      "rate limited",
			screen:    "Rate limit exceeded. Try again later.",
			wantState: models.AgentStateRateLimited,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			state, reason, err := adapter.DetectState(tt.screen, nil)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if state != tt.wantState {
				t.Errorf("DetectState() = %v (reason: %s), want %v", state, reason.Reason, tt.wantState)
			}
		})
	}
}

func TestGeminiAdapter_SupportsApprovals(t *testing.T) {
	adapter := NewGeminiAdapter()
	if !adapter.SupportsApprovals() {
		t.Error("expected SupportsApprovals() to return true")
	}
}

func TestGeminiAdapter_ImplementsInterface(t *testing.T) {
	var _ AgentAdapter = NewGeminiAdapter()
}

// Legacy test kept for backward compatibility
func TestGeminiAdapter_SpawnCommandDefaults(t *testing.T) {
	adapter := GeminiAdapter()
	cmd, args := adapter.SpawnCommand(SpawnOptions{})
	if cmd != "gemini" {
		t.Fatalf("expected command gemini, got %q", cmd)
	}
	if len(args) != 0 {
		t.Fatalf("expected no args, got %v", args)
	}
}

func TestGeminiAdapter_Tier(t *testing.T) {
	adapter := NewGeminiAdapter()
	if adapter.Tier() != models.AdapterTierGeneric {
		t.Fatalf("expected AdapterTierGeneric, got %v", adapter.Tier())
	}
}
