package components

import (
	"testing"

	"github.com/opencode-ai/swarm/internal/tui/styles"
)

func TestSpinner(t *testing.T) {
	// Test that spinner cycles through frames
	frames := make(map[string]bool)
	for i := 0; i < 20; i++ {
		frame := Spinner(i)
		if frame == "" {
			t.Errorf("Spinner(%d) returned empty string", i)
		}
		frames[frame] = true
	}

	// Should cycle through multiple frames
	if len(frames) < 2 {
		t.Errorf("Spinner should cycle through multiple frames, got %d unique", len(frames))
	}
}

func TestSpinnerNegativeFrame(t *testing.T) {
	// Test that negative frame index doesn't panic
	frame := Spinner(-5)
	if frame == "" {
		t.Error("Spinner(-5) returned empty string")
	}
}

func TestSpinnerFramesContent(t *testing.T) {
	// Verify SpinnerFrames contains expected braille characters
	if len(SpinnerFrames) == 0 {
		t.Error("SpinnerFrames should not be empty")
	}

	// Braille spinner should use braille characters
	for i, f := range SpinnerFrames {
		if len(f) == 0 {
			t.Errorf("SpinnerFrames[%d] is empty", i)
		}
	}
}

func TestRenderSpinner(t *testing.T) {
	styleSet := styles.DefaultStyles()

	// Without label
	result := RenderSpinner(styleSet, 0, "")
	if result == "" {
		t.Error("RenderSpinner without label returned empty string")
	}

	// With label
	result = RenderSpinner(styleSet, 0, "Loading...")
	if result == "" {
		t.Error("RenderSpinner with label returned empty string")
	}
}
