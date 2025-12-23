package components

import (
	"strings"
	"testing"

	"github.com/opencode-ai/swarm/internal/models"
	"github.com/opencode-ai/swarm/internal/tui/styles"
)

func TestRenderFileClaimsLine_NilSummary(t *testing.T) {
	styleSet := styles.DefaultStyles()
	result := RenderFileClaimsLine(styleSet, nil)
	if result != "" {
		t.Errorf("RenderFileClaimsLine(nil) = %q, want empty string", result)
	}
}

func TestRenderFileClaimsLine_NoClaims(t *testing.T) {
	styleSet := styles.DefaultStyles()
	summary := models.NewFileClaimSummary()
	result := RenderFileClaimsLine(styleSet, summary)
	if result != "" {
		t.Errorf("RenderFileClaimsLine(empty) = %q, want empty string", result)
	}
}

func TestRenderFileClaimsLine_ExclusiveOnly(t *testing.T) {
	styleSet := styles.DefaultStyles()
	summary := models.NewFileClaimSummary()
	summary.AddClaim(models.FileClaim{ID: 1, Exclusive: true})
	summary.AddClaim(models.FileClaim{ID: 2, Exclusive: true})

	result := RenderFileClaimsLine(styleSet, summary)

	if !strings.Contains(result, "Claims:") {
		t.Error("Result should contain 'Claims:' prefix")
	}
	if !strings.Contains(result, "2 exclusive") {
		t.Errorf("Result should contain '2 exclusive', got: %s", result)
	}
}

func TestRenderFileClaimsLine_SharedOnly(t *testing.T) {
	styleSet := styles.DefaultStyles()
	summary := models.NewFileClaimSummary()
	summary.AddClaim(models.FileClaim{ID: 1, Exclusive: false})

	result := RenderFileClaimsLine(styleSet, summary)

	if !strings.Contains(result, "1 shared") {
		t.Errorf("Result should contain '1 shared', got: %s", result)
	}
}

func TestRenderFileClaimsLine_Mixed(t *testing.T) {
	styleSet := styles.DefaultStyles()
	summary := models.NewFileClaimSummary()
	summary.AddClaim(models.FileClaim{ID: 1, Exclusive: true})
	summary.AddClaim(models.FileClaim{ID: 2, Exclusive: false})
	summary.AddClaim(models.FileClaim{ID: 3, Exclusive: false})

	result := RenderFileClaimsLine(styleSet, summary)

	if !strings.Contains(result, "1 exclusive") {
		t.Errorf("Result should contain '1 exclusive', got: %s", result)
	}
	if !strings.Contains(result, "2 shared") {
		t.Errorf("Result should contain '2 shared', got: %s", result)
	}
}

func TestRenderFileClaimsLine_WithConflicts(t *testing.T) {
	styleSet := styles.DefaultStyles()
	summary := models.NewFileClaimSummary()
	summary.AddClaim(models.FileClaim{ID: 1, Exclusive: true})
	summary.AddConflict(models.FileClaimConflict{Path: "src/main.go"})
	summary.AddConflict(models.FileClaimConflict{Path: "src/app.go"})

	result := RenderFileClaimsLine(styleSet, summary)

	if !strings.Contains(result, "2 conflicts") {
		t.Errorf("Result should contain '2 conflicts', got: %s", result)
	}
}

func TestRenderFileClaimsBadge_NilSummary(t *testing.T) {
	styleSet := styles.DefaultStyles()
	result := RenderFileClaimsBadge(styleSet, nil)
	if result != "" {
		t.Errorf("RenderFileClaimsBadge(nil) = %q, want empty string", result)
	}
}

func TestRenderFileClaimsBadge_NoClaims(t *testing.T) {
	styleSet := styles.DefaultStyles()
	summary := models.NewFileClaimSummary()
	result := RenderFileClaimsBadge(styleSet, summary)
	if result != "" {
		t.Errorf("RenderFileClaimsBadge(empty) = %q, want empty string", result)
	}
}

func TestRenderFileClaimsBadge_WithClaims(t *testing.T) {
	styleSet := styles.DefaultStyles()
	summary := models.NewFileClaimSummary()
	summary.AddClaim(models.FileClaim{ID: 1, Exclusive: true})
	summary.AddClaim(models.FileClaim{ID: 2, Exclusive: true})

	result := RenderFileClaimsBadge(styleSet, summary)

	if !strings.Contains(result, "F2") {
		t.Errorf("Result should contain 'F2', got: %s", result)
	}
}

func TestRenderFileClaimsBadge_WithConflicts(t *testing.T) {
	styleSet := styles.DefaultStyles()
	summary := models.NewFileClaimSummary()
	summary.AddClaim(models.FileClaim{ID: 1, Exclusive: true})
	summary.AddConflict(models.FileClaimConflict{Path: "src/main.go"})

	result := RenderFileClaimsBadge(styleSet, summary)

	if !strings.Contains(result, "!1") {
		t.Errorf("Result should contain '!1' for 1 conflict, got: %s", result)
	}
}

func TestRenderAgentCard_WithClaimSummary(t *testing.T) {
	styleSet := styles.DefaultStyles()
	summary := models.NewFileClaimSummary()
	summary.AddClaim(models.FileClaim{ID: 1, Exclusive: true, PathPattern: "src/*.go"})

	card := AgentCard{
		Name:         "test-agent",
		Type:         models.AgentTypeClaudeCode,
		State:        models.AgentStateWorking,
		Confidence:   models.StateConfidenceHigh,
		ClaimSummary: summary,
	}

	result := RenderAgentCard(styleSet, card, true)

	if !strings.Contains(result, "Claims:") {
		t.Error("Agent card should contain 'Claims:' when claim summary is present")
	}
	if !strings.Contains(result, "1 exclusive") {
		t.Error("Agent card should show '1 exclusive' claim")
	}
}

func TestRenderAgentCard_WithoutClaimSummary(t *testing.T) {
	styleSet := styles.DefaultStyles()

	card := AgentCard{
		Name:         "test-agent",
		Type:         models.AgentTypeClaudeCode,
		State:        models.AgentStateIdle,
		Confidence:   models.StateConfidenceHigh,
		ClaimSummary: nil,
	}

	result := RenderAgentCard(styleSet, card, true)

	// Should not contain claims line when summary is nil
	if strings.Contains(result, "Claims:") {
		t.Error("Agent card should not contain 'Claims:' when claim summary is nil")
	}
}
