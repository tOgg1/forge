package components

import (
	"testing"

	"github.com/tOgg1/forge/internal/models"
	"github.com/tOgg1/forge/internal/sequences"
	"github.com/tOgg1/forge/internal/tui/styles"
)

func TestNewLaunchpad(t *testing.T) {
	lp := NewLaunchpad()

	if lp.Step != LaunchpadStepWorkspace {
		t.Errorf("expected initial step to be Workspace, got %v", lp.Step)
	}

	if lp.CountInput != "1" {
		t.Errorf("expected initial count to be '1', got %q", lp.CountInput)
	}

	if lp.Config.Count != 1 {
		t.Errorf("expected config count to be 1, got %d", lp.Config.Count)
	}

	if lp.Config.RotationMode != AccountRotationRoundRobin {
		t.Errorf("expected rotation mode to be round-robin, got %v", lp.Config.RotationMode)
	}

	if len(lp.AgentTypes) != 5 {
		t.Errorf("expected 5 agent types, got %d", len(lp.AgentTypes))
	}
}

func TestLaunchpad_SetWorkspaces(t *testing.T) {
	lp := NewLaunchpad()
	workspaces := []LaunchpadWorkspace{
		{ID: "1", Name: "project-a", Path: "/home/user/project-a"},
		{ID: "2", Name: "project-b", Path: "/home/user/project-b"},
	}

	lp.SetWorkspaces(workspaces)

	if len(lp.Workspaces) != 2 {
		t.Errorf("expected 2 workspaces, got %d", len(lp.Workspaces))
	}
}

func TestLaunchpad_SetSequences(t *testing.T) {
	lp := NewLaunchpad()
	seqs := []*sequences.Sequence{
		{Name: "bugfix", Description: "Bug fix workflow"},
		{Name: "feature", Description: "Feature workflow"},
	}

	lp.SetSequences(seqs)

	// Should have: none + 2 sequences + custom = 4
	if len(lp.Sequences) != 4 {
		t.Errorf("expected 4 sequence options, got %d", len(lp.Sequences))
	}

	if lp.Sequences[0].Name != "none" {
		t.Errorf("expected first option to be 'none', got %q", lp.Sequences[0].Name)
	}

	if lp.Sequences[3].Name != "custom" {
		t.Errorf("expected last option to be 'custom', got %q", lp.Sequences[3].Name)
	}
}

func TestLaunchpad_MoveSelection(t *testing.T) {
	lp := NewLaunchpad()
	workspaces := []LaunchpadWorkspace{
		{ID: "1", Name: "project-a", Path: "/home/user/project-a"},
		{ID: "2", Name: "project-b", Path: "/home/user/project-b"},
		{ID: "3", Name: "project-c", Path: "/home/user/project-c"},
	}
	lp.SetWorkspaces(workspaces)

	// Initial position should be 0
	if lp.WorkspaceIndex != 0 {
		t.Errorf("expected initial index 0, got %d", lp.WorkspaceIndex)
	}

	// Move down
	lp.MoveSelection(1)
	if lp.WorkspaceIndex != 1 {
		t.Errorf("expected index 1 after moving down, got %d", lp.WorkspaceIndex)
	}

	// Move to end (including "create new" option)
	lp.MoveSelection(1)
	lp.MoveSelection(1)
	if lp.WorkspaceIndex != 3 { // 3 workspaces + 1 "create new" = max index 3
		t.Errorf("expected index 3, got %d", lp.WorkspaceIndex)
	}

	// Should not go beyond max
	lp.MoveSelection(1)
	if lp.WorkspaceIndex != 3 {
		t.Errorf("expected index to stay at 3, got %d", lp.WorkspaceIndex)
	}

	// Move up
	lp.MoveSelection(-1)
	if lp.WorkspaceIndex != 2 {
		t.Errorf("expected index 2, got %d", lp.WorkspaceIndex)
	}
}

func TestLaunchpad_NextStep_AgentType(t *testing.T) {
	lp := NewLaunchpad()
	workspaces := []LaunchpadWorkspace{
		{ID: "1", Name: "project-a", Path: "/home/user/project-a"},
	}
	lp.SetWorkspaces(workspaces)

	// Select first workspace and advance
	completed := lp.NextStep()
	if completed {
		t.Error("expected not completed after first step")
	}

	if lp.Step != LaunchpadStepAgentType {
		t.Errorf("expected step to be AgentType, got %v", lp.Step)
	}

	if lp.Config.WorkspaceID != "1" {
		t.Errorf("expected workspace ID '1', got %q", lp.Config.WorkspaceID)
	}
}

func TestLaunchpad_NextStep_Count(t *testing.T) {
	lp := NewLaunchpad()
	lp.Step = LaunchpadStepCount

	// Invalid count
	lp.CountInput = "abc"
	completed := lp.NextStep()
	if completed {
		t.Error("expected not completed with invalid count")
	}
	if lp.Error == "" {
		t.Error("expected error message for invalid count")
	}

	// Valid count
	lp.CountInput = "4"
	lp.Error = ""
	completed = lp.NextStep()
	if completed {
		t.Error("expected not completed")
	}
	if lp.Config.Count != 4 {
		t.Errorf("expected count 4, got %d", lp.Config.Count)
	}
	if lp.Step != LaunchpadStepAccounts {
		t.Errorf("expected step to be Accounts, got %v", lp.Step)
	}
}

func TestLaunchpad_NextStep_FullWizard(t *testing.T) {
	lp := NewLaunchpad()
	workspaces := []LaunchpadWorkspace{
		{ID: "ws1", Name: "my-project", Path: "/home/user/my-project"},
	}
	lp.SetWorkspaces(workspaces)

	accounts := []LaunchpadAccount{
		{ID: "acc1", Name: "personal", Selected: true},
		{ID: "acc2", Name: "work", Selected: false},
	}
	lp.SetAccounts(accounts)

	seqs := []*sequences.Sequence{
		{Name: "continue", Description: "Resume current task"},
	}
	lp.SetSequences(seqs)

	// Step 1: Workspace
	if lp.NextStep() {
		t.Error("should not be completed at step 1")
	}
	if lp.Step != LaunchpadStepAgentType {
		t.Errorf("expected AgentType step, got %v", lp.Step)
	}

	// Step 2: Agent Type (select opencode)
	lp.AgentTypeIndex = 0
	if lp.NextStep() {
		t.Error("should not be completed at step 2")
	}
	if lp.Step != LaunchpadStepCount {
		t.Errorf("expected Count step, got %v", lp.Step)
	}
	if lp.Config.AgentType != models.AgentTypeOpenCode {
		t.Errorf("expected opencode agent type, got %v", lp.Config.AgentType)
	}

	// Step 3: Count
	lp.CountInput = "2"
	if lp.NextStep() {
		t.Error("should not be completed at step 3")
	}
	if lp.Step != LaunchpadStepAccounts {
		t.Errorf("expected Accounts step, got %v", lp.Step)
	}

	// Step 4: Accounts
	if lp.NextStep() {
		t.Error("should not be completed at step 4")
	}
	if lp.Step != LaunchpadStepSequence {
		t.Errorf("expected Sequence step, got %v", lp.Step)
	}

	// Step 5: Sequence (select "continue")
	lp.SequenceIndex = 1 // "continue" is at index 1 (after "none")
	if lp.NextStep() {
		t.Error("should not be completed at step 5")
	}
	if lp.Step != LaunchpadStepConfirm {
		t.Errorf("expected Confirm step, got %v", lp.Step)
	}

	// Step 6: Confirm - should complete
	if !lp.NextStep() {
		t.Error("should be completed at confirm step")
	}

	// Verify final config
	cfg := lp.GetConfig()
	if cfg.WorkspaceID != "ws1" {
		t.Errorf("expected workspace ID 'ws1', got %q", cfg.WorkspaceID)
	}
	if cfg.AgentType != models.AgentTypeOpenCode {
		t.Errorf("expected opencode, got %v", cfg.AgentType)
	}
	if cfg.Count != 2 {
		t.Errorf("expected count 2, got %d", cfg.Count)
	}
	if cfg.SequenceName != "continue" {
		t.Errorf("expected sequence 'continue', got %q", cfg.SequenceName)
	}
}

func TestLaunchpad_PrevStep(t *testing.T) {
	lp := NewLaunchpad()
	lp.Step = LaunchpadStepConfirm

	// Go back through steps
	cancel := lp.PrevStep()
	if cancel {
		t.Error("expected not cancelled")
	}
	if lp.Step != LaunchpadStepSequence {
		t.Errorf("expected Sequence step, got %v", lp.Step)
	}

	lp.PrevStep()
	if lp.Step != LaunchpadStepAccounts {
		t.Errorf("expected Accounts step, got %v", lp.Step)
	}

	lp.PrevStep()
	if lp.Step != LaunchpadStepCount {
		t.Errorf("expected Count step, got %v", lp.Step)
	}

	lp.PrevStep()
	if lp.Step != LaunchpadStepAgentType {
		t.Errorf("expected AgentType step, got %v", lp.Step)
	}

	lp.PrevStep()
	if lp.Step != LaunchpadStepWorkspace {
		t.Errorf("expected Workspace step, got %v", lp.Step)
	}

	// At first step, PrevStep should return true (cancel)
	cancel = lp.PrevStep()
	if !cancel {
		t.Error("expected cancel at first step")
	}
}

func TestLaunchpad_Reset(t *testing.T) {
	lp := NewLaunchpad()
	lp.Step = LaunchpadStepConfirm
	lp.CountInput = "10"
	lp.Config.Count = 10
	lp.Error = "some error"

	lp.Reset()

	if lp.Step != LaunchpadStepWorkspace {
		t.Errorf("expected Workspace step after reset, got %v", lp.Step)
	}
	if lp.CountInput != "1" {
		t.Errorf("expected count '1' after reset, got %q", lp.CountInput)
	}
	if lp.Config.Count != 1 {
		t.Errorf("expected config count 1 after reset, got %d", lp.Config.Count)
	}
	if lp.Error != "" {
		t.Errorf("expected no error after reset, got %q", lp.Error)
	}
}

func TestLaunchpad_CountPresets(t *testing.T) {
	lp := NewLaunchpad()

	lp.SetCountPreset(8)
	if lp.CountInput != "8" {
		t.Errorf("expected count '8', got %q", lp.CountInput)
	}

	lp.SetCountPreset(16)
	if lp.CountInput != "16" {
		t.Errorf("expected count '16', got %q", lp.CountInput)
	}
}

func TestLaunchpad_Render(t *testing.T) {
	lp := NewLaunchpad()
	workspaces := []LaunchpadWorkspace{
		{ID: "1", Name: "project-a", Path: "/home/user/project-a"},
	}
	lp.SetWorkspaces(workspaces)

	styleSet := styles.DefaultStyles()
	lines := lp.Render(styleSet, 80)

	if len(lines) == 0 {
		t.Error("expected render output")
	}

	// Should contain launchpad title
	found := false
	for _, line := range lines {
		if containsString(line, "Launchpad") {
			found = true
			break
		}
	}
	if !found {
		t.Error("expected 'Launchpad' in render output")
	}
}

func TestLaunchpad_StepName(t *testing.T) {
	lp := NewLaunchpad()

	tests := []struct {
		step LaunchpadStep
		name string
	}{
		{LaunchpadStepWorkspace, "Workspace"},
		{LaunchpadStepAgentType, "Agent Type"},
		{LaunchpadStepCount, "Count"},
		{LaunchpadStepAccounts, "Accounts"},
		{LaunchpadStepSequence, "Sequence"},
		{LaunchpadStepConfirm, "Confirm"},
	}

	for _, tt := range tests {
		lp.Step = tt.step
		if lp.StepName() != tt.name {
			t.Errorf("expected step name %q for step %v, got %q", tt.name, tt.step, lp.StepName())
		}
	}
}

func containsString(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > 0 && containsString(s[1:], substr) || s[:len(substr)] == substr)
}
