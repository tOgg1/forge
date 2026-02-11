package looptui

import "fmt"

type paneLayout struct {
	Rows int
	Cols int
}

var paneLayouts = []paneLayout{
	{Rows: 1, Cols: 1},
	{Rows: 1, Cols: 2},
	{Rows: 1, Cols: 3},
	{Rows: 1, Cols: 4},
	{Rows: 2, Cols: 2},
	{Rows: 2, Cols: 3},
	{Rows: 2, Cols: 4},
	{Rows: 3, Cols: 3},
	{Rows: 3, Cols: 4},
	{Rows: 4, Cols: 4},
}

func (l paneLayout) Capacity() int {
	if l.Rows < 1 || l.Cols < 1 {
		return 1
	}
	return l.Rows * l.Cols
}

func (l paneLayout) Label() string {
	return fmt.Sprintf("%dx%d", l.Rows, l.Cols)
}

func normalizeLayoutIndex(idx int) int {
	if len(paneLayouts) == 0 {
		return 0
	}
	for idx < 0 {
		idx += len(paneLayouts)
	}
	return idx % len(paneLayouts)
}

func layoutIndexFor(rows, cols int) int {
	for i, layout := range paneLayouts {
		if layout.Rows == rows && layout.Cols == cols {
			return i
		}
	}
	return 0
}

func layoutCellSize(layout paneLayout, width, height, gap int) (int, int) {
	if layout.Rows < 1 || layout.Cols < 1 {
		layout = paneLayout{Rows: 1, Cols: 1}
	}
	if gap < 0 {
		gap = 0
	}
	cellWidth := (width - ((layout.Cols - 1) * gap)) / layout.Cols
	cellHeight := (height - ((layout.Rows - 1) * gap)) / layout.Rows
	return cellWidth, cellHeight
}

func layoutFits(layout paneLayout, width, height, gap, minCellWidth, minCellHeight int) bool {
	cellWidth, cellHeight := layoutCellSize(layout, width, height, gap)
	return cellWidth >= minCellWidth && cellHeight >= minCellHeight
}

func fitPaneLayout(requested paneLayout, width, height, gap, minCellWidth, minCellHeight int) paneLayout {
	if requested.Rows < 1 || requested.Cols < 1 {
		requested = paneLayout{Rows: 1, Cols: 1}
	}
	if gap < 0 {
		gap = 0
	}
	if minCellWidth < 1 {
		minCellWidth = 1
	}
	if minCellHeight < 1 {
		minCellHeight = 1
	}

	best := paneLayout{Rows: 1, Cols: 1}
	bestScore := -1

	for _, candidate := range paneLayouts {
		if candidate.Rows > requested.Rows || candidate.Cols > requested.Cols {
			continue
		}
		if !layoutFits(candidate, width, height, gap, minCellWidth, minCellHeight) {
			continue
		}
		score := candidate.Capacity()*100 - (requested.Rows-candidate.Rows)*4 - (requested.Cols-candidate.Cols)*3
		if score > bestScore {
			best = candidate
			bestScore = score
		}
	}
	if bestScore >= 0 {
		return best
	}

	for i := len(paneLayouts) - 1; i >= 0; i-- {
		candidate := paneLayouts[i]
		if layoutFits(candidate, width, height, gap, minCellWidth, minCellHeight) {
			return candidate
		}
	}

	return best
}
