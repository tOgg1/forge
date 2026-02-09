package state

import (
	"testing"

	"github.com/tOgg1/forge/internal/fmailtui/data"
)

func TestSavedSearchesUpsertAndDelete(t *testing.T) {
	t.Parallel()

	m := New("")
	m.UpsertSavedSearch("today", data.SearchQuery{Text: "hello", From: "arch"})
	m.UpsertSavedSearch("auth", data.SearchQuery{Tags: []string{"auth"}})
	m.UpsertSavedSearch("today", data.SearchQuery{Text: "world"})

	saved := m.SavedSearches()
	if len(saved) != 2 {
		t.Fatalf("saved len=%d want 2", len(saved))
	}
	if saved[0].Name != "auth" || saved[1].Name != "today" {
		t.Fatalf("names=%q,%q", saved[0].Name, saved[1].Name)
	}
	if saved[1].Query.Text != "world" {
		t.Fatalf("today text=%q", saved[1].Query.Text)
	}

	m.DeleteSavedSearch("auth")
	saved = m.SavedSearches()
	if len(saved) != 1 || saved[0].Name != "today" {
		t.Fatalf("after delete: %+v", saved)
	}
}
