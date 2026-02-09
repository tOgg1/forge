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

#[cfg(test)]
mod tests {
    use super::{fit_pane_layout, layout_cell_size, PaneLayout};

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
}
