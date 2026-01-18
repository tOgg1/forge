package cli

import (
	"os"
	"testing"
)

func TestHighlightLogLineWithTimestamp(t *testing.T) {
	restore := enableColorForTest(t)
	defer restore()

	highlighter := newLogHighlighter()
	input := "[2025-01-01T00:00:00Z] warning: failed to connect"
	want := colorCyan + "[2025-01-01T00:00:00Z]" + colorReset + colorRed + " warning: failed to connect" + colorReset
	if got := highlighter.HighlightLine(input); got != want {
		t.Fatalf("unexpected highlight\nwant: %q\ngot:  %q", want, got)
	}
}

func TestHighlightLogLinePreservesNewline(t *testing.T) {
	restore := enableColorForTest(t)
	defer restore()

	highlighter := newLogHighlighter()
	input := "loop started\n"
	want := colorGreen + "loop started" + colorReset + "\n"
	if got := highlighter.HighlightLine(input); got != want {
		t.Fatalf("unexpected highlight\nwant: %q\ngot:  %q", want, got)
	}
}

func TestHighlightLogLineDisabled(t *testing.T) {
	restore := enableColorForTest(t)
	defer restore()

	highlighter := newLogHighlighter()
	noColor = true
	input := "loop started"
	if got := highlighter.HighlightLine(input); got != input {
		t.Fatalf("expected no change when color disabled, got %q", got)
	}
}

func TestHighlightLogLineThinkingSection(t *testing.T) {
	restore := enableColorForTest(t)
	defer restore()

	highlighter := newLogHighlighter()
	header := highlighter.HighlightLine("thinking")
	if header != colorMagenta+"thinking"+colorReset {
		t.Fatalf("unexpected header highlight: %q", header)
	}

	line := highlighter.HighlightLine("Considering options")
	if line != colorMagenta+"Considering options"+colorReset {
		t.Fatalf("unexpected thinking highlight: %q", line)
	}
}

func TestHighlightLogLineDiffFence(t *testing.T) {
	restore := enableColorForTest(t)
	defer restore()

	highlighter := newLogHighlighter()
	if got := highlighter.HighlightLine("```diff"); got != colorBlue+"```diff"+colorReset {
		t.Fatalf("unexpected fence highlight: %q", got)
	}
	if got := highlighter.HighlightLine("+added"); got != colorGreen+"+added"+colorReset {
		t.Fatalf("unexpected diff add highlight: %q", got)
	}
	if got := highlighter.HighlightLine("-removed"); got != colorRed+"-removed"+colorReset {
		t.Fatalf("unexpected diff remove highlight: %q", got)
	}
	if got := highlighter.HighlightLine("```"); got != colorBlue+"```"+colorReset {
		t.Fatalf("unexpected fence highlight: %q", got)
	}
}

func enableColorForTest(t *testing.T) func() {
	t.Helper()

	prevNoColor := noColor
	prevJSON := jsonOutput
	prevJSONL := jsonlOutput
	prevEnv, hadEnv := os.LookupEnv("NO_COLOR")

	noColor = false
	jsonOutput = false
	jsonlOutput = false
	if hadEnv {
		_ = os.Unsetenv("NO_COLOR")
	}

	return func() {
		noColor = prevNoColor
		jsonOutput = prevJSON
		jsonlOutput = prevJSONL
		if hadEnv {
			_ = os.Setenv("NO_COLOR", prevEnv)
		} else {
			_ = os.Unsetenv("NO_COLOR")
		}
	}
}
