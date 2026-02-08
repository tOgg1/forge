package looptui

import (
	"testing"

	"github.com/tOgg1/forge/internal/models"
)

func TestCodexLineColorForSection(t *testing.T) {
	palette := resolvePalette("default")
	color, ok := codexLineColor(palette, "thinking")
	if !ok || color == "" {
		t.Fatalf("expected codex section color for thinking")
	}
}

func TestJSONEventTypeParsesClaudeEvent(t *testing.T) {
	event, ok := jsonEventType(`{"type":"error","message":"boom"}`)
	if !ok {
		t.Fatalf("expected json event type")
	}
	if event != "error" {
		t.Fatalf("expected event=error, got %q", event)
	}
}

func TestParseTimestampPrefix(t *testing.T) {
	prefix, ok := parseTimestampPrefix("[2026-02-08T10:00:00Z] run start")
	if !ok {
		t.Fatalf("expected timestamp prefix parse")
	}
	if prefix != "[2026-02-08T10:00:00Z]" {
		t.Fatalf("unexpected prefix %q", prefix)
	}
}

func TestParseTimestampPrefixWithIndent(t *testing.T) {
	prefix, ok := parseTimestampPrefix("  [2026-02-08T10:00:00Z] run start")
	if !ok {
		t.Fatalf("expected timestamp prefix parse")
	}
	if prefix != "  [2026-02-08T10:00:00Z]" {
		t.Fatalf("unexpected prefix %q", prefix)
	}
}

func TestHarnessHighlighterKeepsPlainForUnknownLine(t *testing.T) {
	h := newHarnessLogHighlighter(models.HarnessOpenCode)
	line := "plain text"
	if out := h.HighlightLine(resolvePalette("default"), line); out != line {
		t.Fatalf("expected unchanged line, got %q", out)
	}
}

func TestSanitizeLogLineStripsANSI(t *testing.T) {
	line := "\x1b[31merror\x1b[0m\ttext\r"
	clean := sanitizeLogLine(line)
	if clean != "error    text" {
		t.Fatalf("unexpected sanitized line %q", clean)
	}
}

func TestSanitizeLogLineStripsOSC(t *testing.T) {
	line := "\x1b]0;forge-title\x07content"
	clean := sanitizeLogLine(line)
	if clean != "content" {
		t.Fatalf("unexpected sanitized line %q", clean)
	}
}

func TestLineMatchesLayer(t *testing.T) {
	if !lineMatchesLayer(models.HarnessCodex, "tool: exec", logLayerTools) {
		t.Fatalf("expected tool line match")
	}
	if !lineMatchesLayer(models.HarnessOpenCode, "[2026-02-08T10:00:00Z] started", logLayerEvents) {
		t.Fatalf("expected event line match")
	}
	if !lineMatchesLayer(models.HarnessClaude, "fatal: failed to run", logLayerErrors) {
		t.Fatalf("expected error line match")
	}
	if !lineMatchesLayer(models.HarnessCodex, "+++ b/internal/cli/ui.go", logLayerDiff) {
		t.Fatalf("expected diff line match")
	}
	if lineMatchesLayer(models.HarnessCodex, "plain message", logLayerErrors) {
		t.Fatalf("expected plain line to not match errors layer")
	}
}
