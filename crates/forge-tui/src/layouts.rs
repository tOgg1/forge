//! Loop TUI pane layout helpers.
//!
//! Parity port of `internal/looptui/layouts.go`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneLayout {
    pub rows: i32,
    pub cols: i32,
}

pub const PANE_LAYOUTS: [PaneLayout; 10] = [
    PaneLayout { rows: 1, cols: 1 },
    PaneLayout { rows: 1, cols: 2 },
    PaneLayout { rows: 1, cols: 3 },
    PaneLayout { rows: 1, cols: 4 },
    PaneLayout { rows: 2, cols: 2 },
    PaneLayout { rows: 2, cols: 3 },
    PaneLayout { rows: 2, cols: 4 },
    PaneLayout { rows: 3, cols: 3 },
    PaneLayout { rows: 3, cols: 4 },
    PaneLayout { rows: 4, cols: 4 },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointTier {
    Xs,
    Sm,
    Md,
    Lg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakpointContract {
    pub tier: BreakpointTier,
    pub max_rows: i32,
    pub max_cols: i32,
    pub min_cell_width: i32,
    pub min_cell_height: i32,
}

impl PaneLayout {
    #[must_use]
    pub fn capacity(self) -> i32 {
        if self.rows < 1 || self.cols < 1 {
            return 1;
        }
        self.rows.saturating_mul(self.cols).max(1)
    }

    #[must_use]
    pub fn label(self) -> String {
        format!("{}x{}", self.rows, self.cols)
    }
}

#[must_use]
pub fn normalize_layout_index(mut idx: i32) -> usize {
    if PANE_LAYOUTS.is_empty() {
        return 0;
    }
    while idx < 0 {
        idx += PANE_LAYOUTS.len() as i32;
    }
    (idx as usize) % PANE_LAYOUTS.len()
}

#[must_use]
pub fn layout_index_for(rows: i32, cols: i32) -> usize {
    for (i, layout) in PANE_LAYOUTS.iter().enumerate() {
        if layout.rows == rows && layout.cols == cols {
            return i;
        }
    }
    0
}

#[must_use]
pub fn layout_cell_size(
    mut layout: PaneLayout,
    width: i32,
    height: i32,
    mut gap: i32,
) -> (i32, i32) {
    if layout.rows < 1 || layout.cols < 1 {
        layout = PaneLayout { rows: 1, cols: 1 };
    }
    if gap < 0 {
        gap = 0;
    }

    let cell_width = (width - ((layout.cols - 1) * gap)) / layout.cols;
    let cell_height = (height - ((layout.rows - 1) * gap)) / layout.rows;
    (cell_width, cell_height)
}

#[must_use]
pub fn layout_fits(
    layout: PaneLayout,
    width: i32,
    height: i32,
    gap: i32,
    min_cell_width: i32,
    min_cell_height: i32,
) -> bool {
    let (cell_width, cell_height) = layout_cell_size(layout, width, height, gap);
    cell_width >= min_cell_width && cell_height >= min_cell_height
}

#[must_use]
pub fn fit_pane_layout(
    mut requested: PaneLayout,
    width: i32,
    height: i32,
    mut gap: i32,
    mut min_cell_width: i32,
    mut min_cell_height: i32,
) -> PaneLayout {
    if requested.rows < 1 || requested.cols < 1 {
        requested = PaneLayout { rows: 1, cols: 1 };
    }
    if gap < 0 {
        gap = 0;
    }
    if min_cell_width < 1 {
        min_cell_width = 1;
    }
    if min_cell_height < 1 {
        min_cell_height = 1;
    }

    let mut best = PaneLayout { rows: 1, cols: 1 };
    let mut best_score = -1i32;

    for candidate in PANE_LAYOUTS {
        if candidate.rows > requested.rows || candidate.cols > requested.cols {
            continue;
        }
        if !layout_fits(
            candidate,
            width,
            height,
            gap,
            min_cell_width,
            min_cell_height,
        ) {
            continue;
        }
        let score = candidate.capacity() * 100
            - (requested.rows - candidate.rows) * 4
            - (requested.cols - candidate.cols) * 3;
        if score > best_score {
            best = candidate;
            best_score = score;
        }
    }
    if best_score >= 0 {
        return best;
    }

    for candidate in PANE_LAYOUTS.iter().rev() {
        if layout_fits(
            *candidate,
            width,
            height,
            gap,
            min_cell_width,
            min_cell_height,
        ) {
            return *candidate;
        }
    }

    best
}

#[must_use]
pub fn classify_breakpoint(width: i32, height: i32) -> BreakpointTier {
    if width <= 90 || height <= 26 {
        BreakpointTier::Xs
    } else if width <= 110 || height <= 34 {
        BreakpointTier::Sm
    } else if width <= 160 || height <= 46 {
        BreakpointTier::Md
    } else {
        BreakpointTier::Lg
    }
}

#[must_use]
pub fn breakpoint_contract(
    width: i32,
    height: i32,
    min_cell_width: i32,
    min_cell_height: i32,
) -> BreakpointContract {
    let min_cell_width = min_cell_width.max(1);
    let min_cell_height = min_cell_height.max(1);
    let tier = classify_breakpoint(width, height);
    match tier {
        BreakpointTier::Xs => BreakpointContract {
            tier,
            max_rows: 2,
            max_cols: 2,
            min_cell_width: min_cell_width.saturating_sub(18).max(20),
            min_cell_height: min_cell_height.saturating_sub(4).max(4),
        },
        BreakpointTier::Sm => BreakpointContract {
            tier,
            max_rows: 2,
            max_cols: 3,
            min_cell_width: min_cell_width.saturating_sub(14).max(22),
            min_cell_height: min_cell_height.saturating_sub(3).max(5),
        },
        BreakpointTier::Md => BreakpointContract {
            tier,
            max_rows: 3,
            max_cols: 4,
            min_cell_width: min_cell_width.saturating_sub(10).max(28),
            min_cell_height: min_cell_height.saturating_sub(2).max(6),
        },
        BreakpointTier::Lg => BreakpointContract {
            tier,
            max_rows: 4,
            max_cols: 4,
            min_cell_width,
            min_cell_height,
        },
    }
}

#[must_use]
pub fn fit_pane_layout_for_breakpoint(
    mut requested: PaneLayout,
    width: i32,
    height: i32,
    gap: i32,
    min_cell_width: i32,
    min_cell_height: i32,
) -> PaneLayout {
    if requested.rows < 1 || requested.cols < 1 {
        requested = PaneLayout { rows: 1, cols: 1 };
    }
    let contract = breakpoint_contract(width, height, min_cell_width, min_cell_height);
    let capped = PaneLayout {
        rows: requested.rows.min(contract.max_rows).max(1),
        cols: requested.cols.min(contract.max_cols).max(1),
    };
    fit_pane_layout(
        capped,
        width,
        height,
        gap,
        contract.min_cell_width,
        contract.min_cell_height,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        classify_breakpoint, fit_pane_layout, fit_pane_layout_for_breakpoint, layout_cell_size,
        BreakpointTier, PaneLayout,
    };

    #[test]
    fn fit_pane_layout_degrades_when_space_too_small() {
        let requested = PaneLayout { rows: 4, cols: 4 };
        let actual = fit_pane_layout(requested, 120, 30, 2, 44, 10);
        assert!(actual.rows < requested.rows || actual.cols < requested.cols);
        assert!(actual.capacity() >= 1);
    }

    #[test]
    fn fit_pane_layout_keeps_requested_when_large_enough() {
        let requested = PaneLayout { rows: 2, cols: 2 };
        let actual = fit_pane_layout(requested, 240, 80, 2, 44, 10);
        assert_eq!(actual, requested);
    }

    #[test]
    fn fit_pane_layout_preserves_columns_when_height_is_limited() {
        let requested = PaneLayout { rows: 4, cols: 4 };
        let actual = fit_pane_layout(requested, 220, 26, 1, 44, 10);
        let expected = PaneLayout { rows: 2, cols: 4 };
        assert_eq!(actual, expected);
    }

    #[test]
    fn layout_cell_size_matches_go_math() {
        let (cell_width, cell_height) =
            layout_cell_size(PaneLayout { rows: 2, cols: 4 }, 220, 26, 1);
        assert_eq!(cell_width, 54);
        assert_eq!(cell_height, 12);
    }

    #[test]
    fn classify_breakpoint_maps_standard_snapshot_sizes() {
        assert_eq!(classify_breakpoint(80, 24), BreakpointTier::Xs);
        assert_eq!(classify_breakpoint(104, 32), BreakpointTier::Sm);
        assert_eq!(classify_breakpoint(120, 40), BreakpointTier::Md);
        assert_eq!(classify_breakpoint(200, 50), BreakpointTier::Lg);
    }

    #[test]
    fn fit_breakpoint_contract_caps_dense_layouts_on_small_viewports() {
        let requested = PaneLayout { rows: 4, cols: 4 };
        assert_eq!(
            fit_pane_layout_for_breakpoint(requested, 80, 24, 1, 38, 8),
            PaneLayout { rows: 2, cols: 2 }
        );
        assert_eq!(
            fit_pane_layout_for_breakpoint(requested, 104, 32, 1, 38, 8),
            PaneLayout { rows: 2, cols: 3 }
        );
        assert_eq!(
            fit_pane_layout_for_breakpoint(requested, 120, 40, 1, 38, 8),
            PaneLayout { rows: 3, cols: 4 }
        );
        assert_eq!(
            fit_pane_layout_for_breakpoint(requested, 200, 50, 1, 38, 8),
            PaneLayout { rows: 4, cols: 4 }
        );
    }
}
