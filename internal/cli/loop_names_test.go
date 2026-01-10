package cli

import "testing"

func TestGenerateLoopNameDoesNotMutateExisting(t *testing.T) {
	existing := map[string]struct{}{
		"Slick Nelson": {},
	}
	before := len(existing)

	name := generateLoopName(existing)
	if name == "" {
		t.Fatal("expected generated name")
	}
	if name == "Slick Nelson" {
		t.Fatal("expected generated name to avoid existing values")
	}
	if len(existing) != before {
		t.Fatalf("expected existing names map to remain unchanged, got %d want %d", len(existing), before)
	}
}
