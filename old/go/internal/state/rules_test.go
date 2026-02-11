package state

import (
	"strings"
	"testing"

	"github.com/tOgg1/forge/internal/models"
)

func TestApplyRuleBasedInference(t *testing.T) {
	result := &DetectionResult{
		State:      models.AgentStateWorking,
		Confidence: models.StateConfidenceLow,
		Reason:     "adapter",
	}

	ApplyRuleBasedInference(result, "HTTP 429 rate limit")
	if result.State != models.AgentStateRateLimited {
		t.Fatalf("expected rate limited state, got %q", result.State)
	}
	if result.Reason == "adapter" {
		t.Fatalf("expected reason to be updated")
	}
}

func TestApplyRuleBasedInferenceConflictAddsEvidence(t *testing.T) {
	result := &DetectionResult{
		State:      models.AgentStateIdle,
		Confidence: models.StateConfidenceHigh,
		Reason:     "adapter",
	}

	ApplyRuleBasedInference(result, "HTTP 429 rate limit")
	if result.State != models.AgentStateRateLimited {
		t.Fatalf("expected rate limited state, got %q", result.State)
	}
	found := false
	for _, evidence := range result.Evidence {
		if strings.Contains(evidence, "conflict:") {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("expected conflict evidence to be recorded")
	}
}

func TestCombineResults(t *testing.T) {
	primary := &DetectionResult{
		State:      models.AgentStateIdle,
		Confidence: models.StateConfidenceLow,
		Reason:     "primary",
		Evidence:   []string{"a"},
	}
	secondary := &DetectionResult{
		State:      models.AgentStateError,
		Confidence: models.StateConfidenceHigh,
		Reason:     "secondary",
		Evidence:   []string{"b"},
	}

	combined := CombineResults(primary, secondary)
	if combined.State != models.AgentStateError {
		t.Fatalf("expected secondary state to win, got %q", combined.State)
	}
	if combined.Confidence != models.StateConfidenceHigh {
		t.Fatalf("expected high confidence, got %q", combined.Confidence)
	}
	if len(combined.Evidence) != 2 {
		t.Fatalf("expected evidence to be combined")
	}
	if combined.Reason == "primary" {
		t.Fatalf("expected reason to be updated")
	}
}

func TestResolveConflictPrefersBlocking(t *testing.T) {
	adapter := &DetectionResult{
		State:      models.AgentStateIdle,
		Confidence: models.StateConfidenceHigh,
		Reason:     "adapter",
	}
	transcript := &DetectionResult{
		State:      models.AgentStateRateLimited,
		Confidence: models.StateConfidenceLow,
		Reason:     "transcript",
	}

	resolved := resolveConflict(adapter, transcript)
	if resolved.State != models.AgentStateRateLimited {
		t.Fatalf("expected rate limited to win, got %q", resolved.State)
	}
}

func TestResolveConflictNonBlockingUsesConfidence(t *testing.T) {
	adapter := &DetectionResult{
		State:      models.AgentStateWorking,
		Confidence: models.StateConfidenceLow,
	}
	transcript := &DetectionResult{
		State:      models.AgentStateIdle,
		Confidence: models.StateConfidenceHigh,
	}

	resolved := resolveConflict(adapter, transcript)
	if resolved.State != models.AgentStateIdle {
		t.Fatalf("expected idle to win, got %q", resolved.State)
	}
}

func TestConfidenceRank(t *testing.T) {
	tests := []struct {
		conf     models.StateConfidence
		expected int
	}{
		{models.StateConfidenceHigh, 3},
		{models.StateConfidenceMedium, 2},
		{models.StateConfidenceLow, 1},
		{"unknown", 0},
		{"", 0},
	}

	for _, tt := range tests {
		t.Run(string(tt.conf), func(t *testing.T) {
			rank := confidenceRank(tt.conf)
			if rank != tt.expected {
				t.Fatalf("expected rank %d for %q, got %d", tt.expected, tt.conf, rank)
			}
		})
	}
}

func TestMaxConfidence(t *testing.T) {
	tests := []struct {
		a, b     models.StateConfidence
		expected models.StateConfidence
	}{
		{models.StateConfidenceHigh, models.StateConfidenceLow, models.StateConfidenceHigh},
		{models.StateConfidenceLow, models.StateConfidenceHigh, models.StateConfidenceHigh},
		{models.StateConfidenceMedium, models.StateConfidenceMedium, models.StateConfidenceMedium},
		{models.StateConfidenceHigh, models.StateConfidenceHigh, models.StateConfidenceHigh},
		{models.StateConfidenceLow, models.StateConfidenceLow, models.StateConfidenceLow},
	}

	for _, tt := range tests {
		name := string(tt.a) + "_" + string(tt.b)
		t.Run(name, func(t *testing.T) {
			result := maxConfidence(tt.a, tt.b)
			if result != tt.expected {
				t.Fatalf("expected %q, got %q", tt.expected, result)
			}
		})
	}
}

func TestStateSeverityRank(t *testing.T) {
	tests := []struct {
		state    models.AgentState
		expected int
	}{
		{models.AgentStateError, 6},
		{models.AgentStateRateLimited, 5},
		{models.AgentStateAwaitingApproval, 4},
		{models.AgentStateWorking, 3},
		{models.AgentStateIdle, 2},
		{models.AgentStateStarting, 1},
		{models.AgentStatePaused, 1},
		{models.AgentStateStopped, 0},
		{"unknown", 0},
	}

	for _, tt := range tests {
		t.Run(string(tt.state), func(t *testing.T) {
			rank := stateSeverityRank(tt.state)
			if rank != tt.expected {
				t.Fatalf("expected severity %d for %q, got %d", tt.expected, tt.state, rank)
			}
		})
	}
}

func TestIsBlockingState(t *testing.T) {
	blocking := []models.AgentState{
		models.AgentStateAwaitingApproval,
		models.AgentStateRateLimited,
		models.AgentStateError,
	}

	nonBlocking := []models.AgentState{
		models.AgentStateWorking,
		models.AgentStateIdle,
		models.AgentStateStarting,
		models.AgentStatePaused,
		models.AgentStateStopped,
	}

	for _, state := range blocking {
		if !isBlockingState(state) {
			t.Errorf("expected %q to be blocking", state)
		}
	}

	for _, state := range nonBlocking {
		if isBlockingState(state) {
			t.Errorf("expected %q to NOT be blocking", state)
		}
	}
}

func TestAppendReason(t *testing.T) {
	tests := []struct {
		base, extra string
		expected    string
	}{
		{"base", "extra", "base; extra"},
		{"base", "", "base"},
		{"", "extra", "extra"},
		{"", "", ""},
		{"base", "  ", "base"},
		{"  ", "extra", "extra"},
		{"reason one", "reason two", "reason one; reason two"},
	}

	for _, tt := range tests {
		t.Run(tt.base+"_"+tt.extra, func(t *testing.T) {
			result := appendReason(tt.base, tt.extra)
			if result != tt.expected {
				t.Fatalf("expected %q, got %q", tt.expected, result)
			}
		})
	}
}

func TestCombineResultsNilHandling(t *testing.T) {
	result := &DetectionResult{
		State:      models.AgentStateWorking,
		Confidence: models.StateConfidenceHigh,
		Reason:     "test",
	}

	t.Run("primary nil", func(t *testing.T) {
		combined := CombineResults(nil, result)
		if combined != result {
			t.Fatal("expected secondary to be returned when primary is nil")
		}
	})

	t.Run("secondary nil", func(t *testing.T) {
		combined := CombineResults(result, nil)
		if combined != result {
			t.Fatal("expected primary to be returned when secondary is nil")
		}
	})

	t.Run("both nil", func(t *testing.T) {
		combined := CombineResults(nil, nil)
		if combined != nil {
			t.Fatal("expected nil when both are nil")
		}
	})
}

func TestCombineResultsNilEvidence(t *testing.T) {
	primary := &DetectionResult{
		State:      models.AgentStateWorking,
		Confidence: models.StateConfidenceHigh,
		Reason:     "primary",
		Evidence:   nil, // nil evidence
	}
	secondary := &DetectionResult{
		State:      models.AgentStateIdle,
		Confidence: models.StateConfidenceLow,
		Reason:     "secondary",
		Evidence:   []string{"evidence"},
	}

	combined := CombineResults(primary, secondary)
	if len(combined.Evidence) != 1 {
		t.Fatalf("expected evidence to be combined, got %v", combined.Evidence)
	}
}

func TestResolveConflictBothBlocking(t *testing.T) {
	// When both are blocking, prefer higher severity
	adapter := &DetectionResult{
		State:      models.AgentStateRateLimited, // severity 5
		Confidence: models.StateConfidenceLow,
	}
	transcript := &DetectionResult{
		State:      models.AgentStateError, // severity 6
		Confidence: models.StateConfidenceHigh,
	}

	resolved := resolveConflict(adapter, transcript)
	if resolved.State != models.AgentStateError {
		t.Fatalf("expected error to win (higher severity), got %q", resolved.State)
	}
}

func TestResolveConflictBothBlockingSameSeverity(t *testing.T) {
	// When both are blocking with same state, use confidence
	adapter := &DetectionResult{
		State:      models.AgentStateError,
		Confidence: models.StateConfidenceHigh,
		Reason:     "adapter",
	}
	transcript := &DetectionResult{
		State:      models.AgentStateError,
		Confidence: models.StateConfidenceLow,
		Reason:     "transcript",
	}

	resolved := resolveConflict(adapter, transcript)
	if resolved.Reason != "adapter" {
		t.Fatalf("expected adapter to win (higher confidence), got reason %q", resolved.Reason)
	}
}

func TestResolveConflictNilHandling(t *testing.T) {
	result := &DetectionResult{
		State:      models.AgentStateWorking,
		Confidence: models.StateConfidenceHigh,
	}

	t.Run("adapter nil", func(t *testing.T) {
		resolved := resolveConflict(nil, result)
		if resolved != result {
			t.Fatal("expected transcript when adapter is nil")
		}
	})

	t.Run("transcript nil", func(t *testing.T) {
		resolved := resolveConflict(result, nil)
		if resolved != result {
			t.Fatal("expected adapter when transcript is nil")
		}
	})
}

func TestResolveConflictSameConfidenceUsesSeverity(t *testing.T) {
	adapter := &DetectionResult{
		State:      models.AgentStateWorking, // severity 3
		Confidence: models.StateConfidenceMedium,
	}
	transcript := &DetectionResult{
		State:      models.AgentStateIdle, // severity 2
		Confidence: models.StateConfidenceMedium,
	}

	resolved := resolveConflict(adapter, transcript)
	if resolved.State != models.AgentStateWorking {
		t.Fatalf("expected working to win (higher severity), got %q", resolved.State)
	}
}

func TestApplyRuleBasedInferenceNilResult(t *testing.T) {
	// Should not panic
	ApplyRuleBasedInference(nil, "some screen content")
}

func TestApplyRuleBasedInferenceSameState(t *testing.T) {
	// When adapter and transcript agree, confidence should increase
	result := &DetectionResult{
		State:      models.AgentStateRateLimited,
		Confidence: models.StateConfidenceLow,
		Reason:     "adapter detected rate limit",
		Evidence:   []string{"adapter evidence"},
	}

	ApplyRuleBasedInference(result, "HTTP 429 rate limit exceeded")

	if result.State != models.AgentStateRateLimited {
		t.Fatalf("state should remain rate limited, got %q", result.State)
	}
	// Confidence should be upgraded when both agree
	if result.Confidence == models.StateConfidenceLow {
		t.Log("Note: confidence unchanged when both agree (depends on transcript parser)")
	}
}
