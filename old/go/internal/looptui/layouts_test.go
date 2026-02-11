package looptui

import "testing"

func TestFitPaneLayoutDegradesWhenSpaceTooSmall(t *testing.T) {
	requested := paneLayout{Rows: 4, Cols: 4}
	actual := fitPaneLayout(requested, 120, 30, 2, 44, 10)
	if actual.Rows >= requested.Rows && actual.Cols >= requested.Cols {
		t.Fatalf("expected degraded layout, got %s", actual.Label())
	}
	if actual.Capacity() < 1 {
		t.Fatalf("expected valid capacity")
	}
}

func TestFitPaneLayoutKeepsRequestedWhenLargeEnough(t *testing.T) {
	requested := paneLayout{Rows: 2, Cols: 2}
	actual := fitPaneLayout(requested, 240, 80, 2, 44, 10)
	if actual != requested {
		t.Fatalf("expected %s got %s", requested.Label(), actual.Label())
	}
}

func TestFitPaneLayoutPreservesColumnsWhenHeightIsLimited(t *testing.T) {
	requested := paneLayout{Rows: 4, Cols: 4}
	actual := fitPaneLayout(requested, 220, 26, 1, 44, 10)
	expected := paneLayout{Rows: 2, Cols: 4}
	if actual != expected {
		t.Fatalf("expected %s got %s", expected.Label(), actual.Label())
	}
}

func TestLayoutCellSize(t *testing.T) {
	cellWidth, cellHeight := layoutCellSize(paneLayout{Rows: 2, Cols: 4}, 220, 26, 1)
	if cellWidth != 54 || cellHeight != 12 {
		t.Fatalf("unexpected cell size width=%d height=%d", cellWidth, cellHeight)
	}
}
