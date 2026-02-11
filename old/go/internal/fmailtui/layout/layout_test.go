package layout

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestEffectiveModeResponsiveFallback(t *testing.T) {
	m := NewManager()
	m.SetMode(ModeSplit)
	require.Equal(t, ModeSingle, m.EffectiveMode(90))
	require.Equal(t, ModeSplit, m.EffectiveMode(120))

	m.SetMode(ModeDashboard)
	require.Equal(t, ModeSingle, m.EffectiveMode(90))
	require.Equal(t, ModeSplit, m.EffectiveMode(120))
	require.Equal(t, ModeDashboard, m.EffectiveMode(160))
}

func TestSplitPaneGeometryAndFocus(t *testing.T) {
	m := NewManager()
	m.SetMode(ModeSplit)
	m.SetSplitRatio(0.4)
	m.SetFocus(0)

	panes := m.Panes(Spec{
		Width:     120,
		Height:    30,
		Primary:   "topics",
		Secondary: "thread",
	})

	require.Len(t, panes, 2)
	require.Equal(t, "topics", panes[0].ViewID)
	require.Equal(t, "thread", panes[1].ViewID)
	require.True(t, panes[0].Focused)
	require.False(t, panes[1].Focused)
	require.Equal(t, 30, panes[0].Height)
	require.Equal(t, 30, panes[1].Height)
	require.Equal(t, 120, panes[0].Width+panes[1].Width+1)

	m.CycleFocus(Spec{Width: 120, Height: 30, Primary: "topics", Secondary: "thread"})
	panes = m.Panes(Spec{Width: 120, Height: 30, Primary: "topics", Secondary: "thread"})
	require.True(t, panes[1].Focused)
}

func TestDashboardGridModes(t *testing.T) {
	m := NewManager()
	m.SetMode(ModeDashboard)
	m.SetFocus(2)

	spec := Spec{
		Width:     160,
		Height:    40,
		Primary:   "topics",
		Secondary: "thread",
		Dashboard: [4]string{"agents", "live-tail", "topics", "thread"},
	}

	panes := m.Panes(spec)
	require.Len(t, panes, 4)
	require.True(t, panes[2].Focused)
	require.Equal(t, 160, panes[0].Width+panes[1].Width+1)
	require.Equal(t, 40, panes[0].Height+panes[2].Height+1)

	m.CycleGrid() // 2x1
	panes = m.Panes(spec)
	require.Len(t, panes, 2)

	m.CycleGrid() // 1x2
	panes = m.Panes(spec)
	require.Len(t, panes, 2)
	require.Equal(t, 160, panes[0].Width+panes[1].Width+1)

	m.CycleGrid() // 1x3
	panes = m.Panes(spec)
	require.Len(t, panes, 3)

	m.CycleGrid() // 3x1
	panes = m.Panes(spec)
	require.Len(t, panes, 3)
	require.Equal(t, 40, panes[0].Height+panes[1].Height+panes[2].Height+2)
}

func TestExpandedAndCollapsedModes(t *testing.T) {
	m := NewManager()
	m.SetMode(ModeSplit)
	m.SetFocus(1)
	m.ToggleExpanded()

	panes := m.Panes(Spec{
		Width:     120,
		Height:    30,
		Primary:   "topics",
		Secondary: "thread",
	})
	require.Len(t, panes, 1)
	require.Equal(t, "thread", panes[0].ViewID)

	m.ToggleExpanded()
	m.ToggleSplitCollapsed()
	panes = m.Panes(Spec{
		Width:     120,
		Height:    30,
		Primary:   "topics",
		Secondary: "thread",
	})
	require.Len(t, panes, 1)
	require.Equal(t, "thread", panes[0].ViewID)
}
