// Package components provides reusable TUI components.
package components

import (
	"github.com/tOgg1/forge/internal/tui/styles"
)

// SpinnerFrames contains the braille spinner animation frames.
var SpinnerFrames = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

// Spinner returns a spinner character for the given frame index.
func Spinner(frame int) string {
	if len(SpinnerFrames) == 0 {
		return "⠿"
	}
	idx := frame % len(SpinnerFrames)
	if idx < 0 {
		idx = -idx
	}
	return SpinnerFrames[idx]
}

// RenderSpinner renders a styled spinner with optional label.
func RenderSpinner(styleSet styles.Styles, frame int, label string) string {
	spinner := Spinner(frame)
	if label == "" {
		return styleSet.Accent.Render(spinner)
	}
	return styleSet.Accent.Render(spinner) + " " + styleSet.Muted.Render(label)
}
