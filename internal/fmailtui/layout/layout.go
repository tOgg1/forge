package layout

import (
	"math"
	"strings"
)

type Mode string

const (
	ModeSingle    Mode = "single"
	ModeSplit     Mode = "split"
	ModeDashboard Mode = "dashboard"
	ModeZen       Mode = "zen"
)

type Grid string

const (
	Grid2x2 Grid = "2x2"
	Grid2x1 Grid = "2x1"
	Grid1x2 Grid = "1x2"
	Grid1x3 Grid = "1x3"
	Grid3x1 Grid = "3x1"
)

const (
	DefaultSplitRatio       = 0.35
	DefaultSplitMinPane     = 20
	DefaultDashboardMinPane = 16
	SplitToSingleWidth      = 100
	DashboardToSplitWidth   = 140
)

type Pane struct {
	ViewID  string
	X       int
	Y       int
	Width   int
	Height  int
	Focused bool
	Compact bool
}

type Spec struct {
	Width     int
	Height    int
	Primary   string
	Secondary string
	Dashboard [4]string
}

type Manager struct {
	mode           Mode
	splitRatio     float64
	splitCollapsed bool
	focus          int
	expanded       bool
	grid           Grid
	dashboard      [4]string
}

func NewManager() *Manager {
	m := &Manager{
		mode:       ModeSplit,
		splitRatio: DefaultSplitRatio,
		focus:      1,
		grid:       Grid2x2,
		dashboard:  [4]string{"agents", "live-tail", "topics", "thread"},
	}
	return m
}

func ParseMode(raw string) Mode {
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case string(ModeSingle):
		return ModeSingle
	case string(ModeSplit):
		return ModeSplit
	case string(ModeDashboard):
		return ModeDashboard
	case string(ModeZen):
		return ModeZen
	default:
		return ModeSplit
	}
}

func ParseGrid(raw string) Grid {
	switch strings.TrimSpace(raw) {
	case string(Grid2x1):
		return Grid2x1
	case string(Grid1x2):
		return Grid1x2
	case string(Grid1x3):
		return Grid1x3
	case string(Grid3x1):
		return Grid3x1
	default:
		return Grid2x2
	}
}

func (m *Manager) Mode() Mode {
	if m == nil {
		return ModeSplit
	}
	return m.mode
}

func (m *Manager) SetMode(mode Mode) {
	if m == nil {
		return
	}
	switch mode {
	case ModeSingle, ModeSplit, ModeDashboard, ModeZen:
		m.mode = mode
	default:
		m.mode = ModeSplit
	}
}

func (m *Manager) EffectiveMode(width int) Mode {
	if m == nil {
		return ModeSplit
	}
	switch m.mode {
	case ModeDashboard:
		if width < DashboardToSplitWidth {
			if width < SplitToSingleWidth {
				return ModeSingle
			}
			return ModeSplit
		}
		return ModeDashboard
	case ModeSplit:
		if width < SplitToSingleWidth {
			return ModeSingle
		}
		return ModeSplit
	case ModeZen:
		return ModeZen
	default:
		return ModeSingle
	}
}

func (m *Manager) SplitRatio() float64 {
	if m == nil {
		return DefaultSplitRatio
	}
	return m.splitRatio
}

func (m *Manager) SetSplitRatio(ratio float64) {
	if m == nil {
		return
	}
	m.splitRatio = clampFloat(ratio, 0.2, 0.8)
}

func (m *Manager) AdjustSplit(delta float64) {
	if m == nil {
		return
	}
	m.SetSplitRatio(m.splitRatio + delta)
}

func (m *Manager) SplitCollapsed() bool {
	if m == nil {
		return false
	}
	return m.splitCollapsed
}

func (m *Manager) ToggleSplitCollapsed() bool {
	if m == nil {
		return false
	}
	m.splitCollapsed = !m.splitCollapsed
	return m.splitCollapsed
}

func (m *Manager) SetSplitCollapsed(collapsed bool) {
	if m == nil {
		return
	}
	m.splitCollapsed = collapsed
}

func (m *Manager) Expanded() bool {
	if m == nil {
		return false
	}
	return m.expanded
}

func (m *Manager) ToggleExpanded() bool {
	if m == nil {
		return false
	}
	m.expanded = !m.expanded
	return m.expanded
}

func (m *Manager) SetExpanded(expanded bool) {
	if m == nil {
		return
	}
	m.expanded = expanded
}

func (m *Manager) Focus() int {
	if m == nil {
		return 0
	}
	return m.focus
}

func (m *Manager) SetFocus(focus int) {
	if m == nil || focus < 0 {
		return
	}
	m.focus = focus
}

func (m *Manager) CycleFocus(spec Spec) {
	if m == nil {
		return
	}
	panes := m.Panes(spec)
	if len(panes) <= 1 {
		m.focus = 0
		return
	}
	next := 0
	for idx, pane := range panes {
		if pane.Focused {
			next = (idx + 1) % len(panes)
			break
		}
	}
	m.focus = next
}

func (m *Manager) MoveFocusHorizontal(spec Spec, dir int) {
	if m == nil || dir == 0 {
		return
	}
	panes := m.Panes(spec)
	if len(panes) <= 1 {
		m.focus = 0
		return
	}
	if m.EffectiveMode(spec.Width) == ModeSplit {
		if dir < 0 {
			m.focus = 0
		} else {
			m.focus = 1
		}
		return
	}
	m.focus = normalizeIndex(m.focus+dir, len(panes))
}

func (m *Manager) MoveFocusVertical(spec Spec, dir int) {
	if m == nil || dir == 0 {
		return
	}
	panes := m.Panes(spec)
	if len(panes) <= 1 {
		m.focus = 0
		return
	}
	if m.EffectiveMode(spec.Width) != ModeDashboard {
		return
	}
	switch m.grid {
	case Grid2x2:
		if dir < 0 && m.focus >= 2 {
			m.focus -= 2
		} else if dir > 0 && m.focus <= 1 {
			m.focus += 2
		}
	case Grid2x1:
		if dir < 0 {
			m.focus = 0
		} else {
			m.focus = 1
		}
	case Grid3x1:
		if dir < 0 {
			m.focus = maxInt(0, m.focus-1)
		} else {
			m.focus = minInt(2, m.focus+1)
		}
	}
}

func (m *Manager) Grid() Grid {
	if m == nil {
		return Grid2x2
	}
	return m.grid
}

func (m *Manager) SetGrid(grid Grid) {
	if m == nil {
		return
	}
	m.grid = ParseGrid(string(grid))
}

func (m *Manager) CycleGrid() Grid {
	if m == nil {
		return Grid2x2
	}
	switch m.grid {
	case Grid2x2:
		m.grid = Grid2x1
	case Grid2x1:
		m.grid = Grid1x2
	case Grid1x2:
		m.grid = Grid1x3
	case Grid1x3:
		m.grid = Grid3x1
	default:
		m.grid = Grid2x2
	}
	m.focus = 0
	return m.grid
}

func (m *Manager) DashboardViews() [4]string {
	if m == nil {
		return [4]string{}
	}
	return m.dashboard
}

func (m *Manager) SetDashboardViews(views [4]string) {
	if m == nil {
		return
	}
	for i := 0; i < len(views); i++ {
		if strings.TrimSpace(views[i]) == "" {
			continue
		}
		m.dashboard[i] = strings.TrimSpace(views[i])
	}
}

func (m *Manager) SetDashboardView(slot int, viewID string) {
	if m == nil || slot < 0 || slot >= 4 {
		return
	}
	viewID = strings.TrimSpace(viewID)
	if viewID == "" {
		return
	}
	m.dashboard[slot] = viewID
}

func (m *Manager) Panes(spec Spec) []Pane {
	if m == nil || spec.Width <= 0 || spec.Height <= 0 {
		return nil
	}
	mode := m.EffectiveMode(spec.Width)
	primary := strings.TrimSpace(spec.Primary)
	if primary == "" {
		primary = "dashboard"
	}
	secondary := strings.TrimSpace(spec.Secondary)
	if secondary == "" {
		secondary = primary
	}
	if spec.Dashboard == [4]string{} {
		spec.Dashboard = m.dashboard
	}
	switch mode {
	case ModeZen:
		return []Pane{{
			ViewID:  secondary,
			X:       0,
			Y:       0,
			Width:   spec.Width,
			Height:  spec.Height,
			Focused: true,
		}}
	case ModeSingle:
		return []Pane{{
			ViewID:  secondary,
			X:       0,
			Y:       0,
			Width:   spec.Width,
			Height:  spec.Height,
			Focused: true,
			Compact: spec.Width < SplitToSingleWidth,
		}}
	case ModeSplit:
		return m.splitPanes(spec, primary, secondary)
	default:
		return m.dashboardPanes(spec)
	}
}

func (m *Manager) splitPanes(spec Spec, leftID, rightID string) []Pane {
	if m.expanded {
		focusedID := rightID
		if m.focus == 0 {
			focusedID = leftID
		}
		return []Pane{{
			ViewID:  focusedID,
			X:       0,
			Y:       0,
			Width:   spec.Width,
			Height:  spec.Height,
			Focused: true,
		}}
	}
	if m.splitCollapsed {
		return []Pane{{
			ViewID:  rightID,
			X:       0,
			Y:       0,
			Width:   spec.Width,
			Height:  spec.Height,
			Focused: true,
		}}
	}

	total := spec.Width - 1
	if total <= (DefaultSplitMinPane * 2) {
		return []Pane{{
			ViewID:  rightID,
			X:       0,
			Y:       0,
			Width:   spec.Width,
			Height:  spec.Height,
			Focused: true,
		}}
	}
	leftW := int(math.Round(float64(total) * m.splitRatio))
	leftW = clampInt(leftW, DefaultSplitMinPane, total-DefaultSplitMinPane)
	rightW := total - leftW
	focus := normalizeIndex(m.focus, 2)
	m.focus = focus
	return []Pane{
		{
			ViewID:  leftID,
			X:       0,
			Y:       0,
			Width:   leftW,
			Height:  spec.Height,
			Focused: focus == 0,
			Compact: leftW < 48,
		},
		{
			ViewID:  rightID,
			X:       leftW + 1,
			Y:       0,
			Width:   rightW,
			Height:  spec.Height,
			Focused: focus == 1,
			Compact: rightW < 72,
		},
	}
}

func (m *Manager) dashboardPanes(spec Spec) []Pane {
	views := spec.Dashboard
	for i := 0; i < len(views); i++ {
		if strings.TrimSpace(views[i]) == "" {
			views[i] = m.dashboard[i]
		}
	}
	if m.expanded {
		focus := normalizeIndex(m.focus, 4)
		m.focus = focus
		return []Pane{{
			ViewID:  views[focus],
			X:       0,
			Y:       0,
			Width:   spec.Width,
			Height:  spec.Height,
			Focused: true,
		}}
	}

	switch m.grid {
	case Grid2x1:
		return stackRows(spec, []string{views[0], views[1]}, normalizeIndex(m.focus, 2))
	case Grid1x2:
		return stackCols(spec, []string{views[0], views[1]}, normalizeIndex(m.focus, 2))
	case Grid1x3:
		return stackCols(spec, []string{views[0], views[1], views[2]}, normalizeIndex(m.focus, 3))
	case Grid3x1:
		return stackRows(spec, []string{views[0], views[1], views[2]}, normalizeIndex(m.focus, 3))
	default:
		return grid2x2(spec, views, normalizeIndex(m.focus, 4))
	}
}

func grid2x2(spec Spec, views [4]string, focus int) []Pane {
	wA, wB := splitPair(spec.Width)
	hA, hB := splitPair(spec.Height)
	out := []Pane{
		{ViewID: views[0], X: 0, Y: 0, Width: wA, Height: hA, Focused: focus == 0, Compact: wA < 58 || hA < 10},
		{ViewID: views[1], X: wA + 1, Y: 0, Width: wB, Height: hA, Focused: focus == 1, Compact: wB < 58 || hA < 10},
		{ViewID: views[2], X: 0, Y: hA + 1, Width: wA, Height: hB, Focused: focus == 2, Compact: wA < 58 || hB < 10},
		{ViewID: views[3], X: wA + 1, Y: hA + 1, Width: wB, Height: hB, Focused: focus == 3, Compact: wB < 58 || hB < 10},
	}
	return out
}

func stackRows(spec Spec, views []string, focus int) []Pane {
	n := len(views)
	if n == 0 {
		return nil
	}
	sizes := splitDimension(spec.Height, n)
	out := make([]Pane, 0, n)
	y := 0
	for idx, h := range sizes {
		out = append(out, Pane{
			ViewID:  views[idx],
			X:       0,
			Y:       y,
			Width:   spec.Width,
			Height:  h,
			Focused: focus == idx,
			Compact: h < 8,
		})
		y += h + 1
	}
	return out
}

func stackCols(spec Spec, views []string, focus int) []Pane {
	n := len(views)
	if n == 0 {
		return nil
	}
	sizes := splitDimension(spec.Width, n)
	out := make([]Pane, 0, n)
	x := 0
	for idx, w := range sizes {
		out = append(out, Pane{
			ViewID:  views[idx],
			X:       x,
			Y:       0,
			Width:   w,
			Height:  spec.Height,
			Focused: focus == idx,
			Compact: w < 52,
		})
		x += w + 1
	}
	return out
}

func splitPair(total int) (int, int) {
	if total <= (DefaultDashboardMinPane*2)+1 {
		left := total
		if left < 0 {
			left = 0
		}
		return left, 0
	}
	left := (total - 1) / 2
	right := total - left - 1
	return left, right
}

func splitDimension(total, n int) []int {
	if n <= 0 {
		return nil
	}
	gaps := n - 1
	usable := total - gaps
	if usable < n {
		usable = n
	}
	base := usable / n
	rem := usable % n
	out := make([]int, n)
	for i := 0; i < n; i++ {
		out[i] = base
		if i < rem {
			out[i]++
		}
	}
	return out
}

func clampInt(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

func clampFloat(v, lo, hi float64) float64 {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

func normalizeIndex(v, n int) int {
	if n <= 0 {
		return 0
	}
	if v < 0 {
		return 0
	}
	if v >= n {
		return n - 1
	}
	return v
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}
