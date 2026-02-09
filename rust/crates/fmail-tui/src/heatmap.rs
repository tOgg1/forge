use forge_ftui_adapter::input::{translate_input, InputEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeatmapViewModel {
    row_labels: Vec<String>,
    grid: Vec<Vec<u32>>,
    selected_row: usize,
    selected_col: usize,
}

impl Default for HeatmapViewModel {
    fn default() -> Self {
        Self::new(
            vec![
                "Mon".to_owned(),
                "Tue".to_owned(),
                "Wed".to_owned(),
                "Thu".to_owned(),
                "Fri".to_owned(),
                "Sat".to_owned(),
                "Sun".to_owned(),
            ],
            12,
        )
    }
}

impl HeatmapViewModel {
    #[must_use]
    pub fn new(row_labels: Vec<String>, columns: usize) -> Self {
        let cols = columns.max(1);
        let rows = row_labels.len().max(1);
        let labels = if row_labels.is_empty() {
            vec!["All".to_owned()]
        } else {
            row_labels
        };
        Self {
            row_labels: labels,
            grid: vec![vec![0; cols]; rows],
            selected_row: 0,
            selected_col: 0,
        }
    }

    pub fn set(&mut self, row: usize, col: usize, count: u32) {
        if row >= self.grid.len() || col >= self.grid[row].len() {
            return;
        }
        self.grid[row][col] = count;
    }

    pub fn increment(&mut self, row: usize, col: usize) {
        if row >= self.grid.len() || col >= self.grid[row].len() {
            return;
        }
        self.grid[row][col] = self.grid[row][col].saturating_add(1);
    }

    #[must_use]
    pub fn max_count(&self) -> u32 {
        self.grid
            .iter()
            .flat_map(|row| row.iter())
            .copied()
            .max()
            .unwrap_or(0)
    }

    pub fn move_up(&mut self) {
        self.selected_row = self.selected_row.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        self.selected_row = (self.selected_row + 1).min(self.grid.len().saturating_sub(1));
    }

    pub fn move_left(&mut self) {
        self.selected_col = self.selected_col.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        let max_col = self.grid[self.selected_row].len().saturating_sub(1);
        self.selected_col = (self.selected_col + 1).min(max_col);
    }

    #[must_use]
    pub fn selected_cell(&self) -> (usize, usize, u32) {
        (
            self.selected_row,
            self.selected_col,
            self.grid[self.selected_row][self.selected_col],
        )
    }

    #[must_use]
    pub fn selected_label(&self) -> &str {
        &self.row_labels[self.selected_row]
    }
}

pub fn apply_heatmap_input(view: &mut HeatmapViewModel, event: InputEvent) {
    match translate_input(&event) {
        UiAction::MoveUp => view.move_up(),
        UiAction::MoveDown => view.move_down(),
        UiAction::MoveLeft => view.move_left(),
        UiAction::MoveRight => view.move_right(),
        _ => {}
    }
}

#[must_use]
pub fn render_heatmap_frame(
    view: &HeatmapViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    let max = view.max_count();
    let selected_count = view.grid[view.selected_row][view.selected_col];
    frame.draw_text(
        0,
        0,
        &truncate(
            &format!(
                "HEATMAP  max:{}  selected:{}:{}={}",
                max, view.row_labels[view.selected_row], view.selected_col, selected_count
            ),
            width,
        ),
        TextRole::Accent,
    );
    if height == 1 {
        return frame;
    }

    let rows_to_draw = view.grid.len().min(height - 1);
    for row in 0..rows_to_draw {
        let mut line = String::new();
        line.push(if row == view.selected_row { '>' } else { ' ' });
        line.push_str(&truncate(&view.row_labels[row], 3));
        line.push(' ');
        for (col, value) in view.grid[row].iter().enumerate() {
            let glyph = glyph_for(*value, max);
            if row == view.selected_row && col == view.selected_col {
                line.push('[');
                line.push(glyph);
                line.push(']');
            } else {
                line.push(' ');
                line.push(glyph);
                line.push(' ');
            }
        }
        frame.draw_text(0, row + 1, &truncate(&line, width), TextRole::Primary);
    }

    frame
}

fn glyph_for(value: u32, max: u32) -> char {
    if max == 0 || value == 0 {
        return '·';
    }
    let ratio = value as f32 / max as f32;
    if ratio < 0.25 {
        '░'
    } else if ratio < 0.5 {
        '▒'
    } else if ratio < 0.75 {
        '▓'
    } else {
        '█'
    }
}

fn truncate(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return input.to_owned();
    }
    if max_chars == 1 {
        return "…".to_owned();
    }
    let mut out = chars.into_iter().take(max_chars - 1).collect::<String>();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::{apply_heatmap_input, render_heatmap_frame, HeatmapViewModel};
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    #[test]
    fn glyph_density_and_navigation() {
        let mut view = HeatmapViewModel::new(vec!["Mon".to_owned(), "Tue".to_owned()], 3);
        view.set(0, 0, 1);
        view.set(0, 1, 3);
        view.set(0, 2, 6);
        view.set(1, 0, 9);
        assert_eq!(view.max_count(), 9);

        apply_heatmap_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Down)));
        apply_heatmap_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Right)));
        let selected = view.selected_cell();
        assert_eq!(selected, (1, 1, 0));
        assert_eq!(view.selected_label(), "Tue");
    }

    #[test]
    fn heatmap_snapshot() {
        let mut view = HeatmapViewModel::new(
            vec!["Mon".to_owned(), "Tue".to_owned(), "Wed".to_owned()],
            4,
        );
        view.set(0, 0, 1);
        view.set(0, 1, 2);
        view.set(0, 2, 3);
        view.set(0, 3, 4);
        view.set(1, 0, 4);
        view.set(2, 2, 4);

        let frame = render_heatmap_frame(&view, 42, 5, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_heatmap_frame",
            &frame,
            "HEATMAP  max:4  selected:Mon:0=1          \n>Mon [▒] ▓  █  █                          \n Tue  █  ·  ·  ·                          \n Wed  ·  ·  █  ·                          \n                                          ",
        );
    }
}
