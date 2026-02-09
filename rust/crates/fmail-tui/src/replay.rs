use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayEntry {
    pub timestamp: String,
    pub from: String,
    pub target: String,
    pub preview: String,
}

impl ReplayEntry {
    #[must_use]
    pub fn new(timestamp: &str, from: &str, target: &str, preview: &str) -> Self {
        Self {
            timestamp: timestamp.to_owned(),
            from: from.to_owned(),
            target: target.to_owned(),
            preview: preview.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayViewModel {
    entries: Vec<ReplayEntry>,
    cursor: usize,
    playing: bool,
    speed: u8,
}

impl Default for ReplayViewModel {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            playing: false,
            speed: 1,
        }
    }
}

impl ReplayViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_entries(&mut self, mut entries: Vec<ReplayEntry>) {
        entries.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
        self.entries = entries;
        self.cursor = 0;
    }

    pub fn toggle_playing(&mut self) {
        self.playing = !self.playing;
    }

    pub fn step_back(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn step_forward(&mut self) {
        self.cursor = (self.cursor + 1).min(self.entries.len().saturating_sub(1));
    }

    pub fn tick(&mut self) {
        if !self.playing {
            return;
        }
        for _ in 0..self.speed {
            self.step_forward();
        }
    }

    pub fn set_speed(&mut self, speed: u8) {
        self.speed = speed.clamp(1, 4);
    }

    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    #[must_use]
    pub fn playing(&self) -> bool {
        self.playing
    }

    #[must_use]
    pub fn current(&self) -> Option<&ReplayEntry> {
        self.entries.get(self.cursor)
    }
}

pub fn apply_replay_input(view: &mut ReplayViewModel, event: InputEvent) {
    match event {
        InputEvent::Key(KeyEvent {
            key: Key::Char(' '),
            ..
        }) => {
            view.toggle_playing();
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char('1'),
            ..
        }) => {
            view.set_speed(1);
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char('2'),
            ..
        }) => {
            view.set_speed(2);
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char('3'),
            ..
        }) => {
            view.set_speed(3);
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char('4'),
            ..
        }) => {
            view.set_speed(4);
            return;
        }
        _ => {}
    }
    match translate_input(&event) {
        UiAction::MoveLeft | UiAction::MoveUp => view.step_back(),
        UiAction::MoveRight | UiAction::MoveDown => view.step_forward(),
        UiAction::Refresh => view.tick(),
        _ => {}
    }
}

#[must_use]
pub fn render_replay_frame(
    view: &ReplayViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    let state = if view.playing() { "playing" } else { "paused" };
    frame.draw_text(
        0,
        0,
        &truncate(
            &format!(
                "REPLAY  {}  cursor:{}/{}  speed:{}x",
                state,
                view.cursor().saturating_add(1),
                view.entries.len(),
                view.speed
            ),
            width,
        ),
        TextRole::Accent,
    );
    if height == 1 {
        return frame;
    }

    if view.entries.is_empty() {
        frame.draw_text(0, 1, "(no replay entries)", TextRole::Muted);
        return frame;
    }

    let start = view.cursor().saturating_sub(1);
    let end = (start + height - 1).min(view.entries.len());
    for (row, idx) in (start..end).enumerate() {
        let entry = &view.entries[idx];
        let marker = if idx == view.cursor() { ">" } else { " " };
        let line = format!(
            "{}{} {} -> {}  {}",
            marker,
            entry.timestamp,
            truncate(entry.from.trim(), 12),
            truncate(entry.target.trim(), 12),
            truncate(entry.preview.trim(), 26)
        );
        frame.draw_text(
            0,
            row + 1,
            &truncate(&line, width),
            if idx == view.cursor() {
                TextRole::Accent
            } else {
                TextRole::Primary
            },
        );
    }
    frame
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
    use super::{apply_replay_input, render_replay_frame, ReplayEntry, ReplayViewModel};
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    #[test]
    fn replay_step_and_tick() {
        let mut view = ReplayViewModel::new();
        view.set_entries(vec![
            ReplayEntry::new("15:30:00", "a", "task", "one"),
            ReplayEntry::new("15:31:00", "b", "task", "two"),
            ReplayEntry::new("15:32:00", "c", "task", "three"),
        ]);

        apply_replay_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Right)));
        assert_eq!(view.cursor(), 1);

        apply_replay_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char(' '))));
        apply_replay_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('2'))));
        apply_replay_input(&mut view, InputEvent::Tick);
        assert_eq!(view.cursor(), 2);
    }

    #[test]
    fn replay_snapshot() {
        let mut view = ReplayViewModel::new();
        view.set_entries(vec![
            ReplayEntry::new("15:30:00", "architect", "task", "bootstrap"),
            ReplayEntry::new("15:30:03", "coder-1", "@architect", "need context"),
            ReplayEntry::new("15:30:09", "architect", "@coder-1", "use sliding window"),
        ]);
        view.step_forward();

        let frame = render_replay_frame(&view, 62, 5, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_replay_frame",
            &frame,
            "REPLAY  paused  cursor:2/3  speed:1x                          \n 15:30:00 architect -> task  bootstrap                        \n>15:30:03 coder-1 -> @architect  need context                 \n 15:30:09 architect -> @coder-1  use sliding window           \n                                                              ",
        );
    }
}
