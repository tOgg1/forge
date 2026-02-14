//! Inline embedded terminal mode (status bar style) with one-key full-screen toggle.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalDisplayMode {
    InlineSingle,
    InlineTriple,
    FullTui,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineStatusSnapshot {
    pub active_loops: usize,
    pub degraded_loops: usize,
    pub queue_depth: usize,
    pub running_runs: usize,
    pub unread_fmail: usize,
    pub selected_loop_id: Option<String>,
    pub selected_tab: Option<String>,
    pub status_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineTerminalState {
    pub mode: TerminalDisplayMode,
    pub toggle_hotkey: char,
    pub last_full_tab: String,
}

impl Default for InlineTerminalState {
    fn default() -> Self {
        Self {
            mode: TerminalDisplayMode::InlineSingle,
            toggle_hotkey: '`',
            last_full_tab: "overview".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleOutcome {
    pub mode: TerminalDisplayMode,
    pub keypress_count: u8,
    pub hint: String,
}

pub fn toggle_inline_full_mode(state: &mut InlineTerminalState) -> ToggleOutcome {
    state.mode = match state.mode {
        TerminalDisplayMode::FullTui => TerminalDisplayMode::InlineSingle,
        TerminalDisplayMode::InlineSingle | TerminalDisplayMode::InlineTriple => {
            TerminalDisplayMode::FullTui
        }
    };

    ToggleOutcome {
        mode: state.mode,
        keypress_count: 1,
        hint: match state.mode {
            TerminalDisplayMode::FullTui => {
                format!("full TUI opened (toggle with '{}')", state.toggle_hotkey)
            }
            TerminalDisplayMode::InlineSingle | TerminalDisplayMode::InlineTriple => {
                format!(
                    "inline mode enabled (toggle with '{}')",
                    state.toggle_hotkey
                )
            }
        },
    }
}

pub fn cycle_inline_density(state: &mut InlineTerminalState) {
    state.mode = match state.mode {
        TerminalDisplayMode::InlineSingle => TerminalDisplayMode::InlineTriple,
        TerminalDisplayMode::InlineTriple => TerminalDisplayMode::InlineSingle,
        TerminalDisplayMode::FullTui => TerminalDisplayMode::FullTui,
    };
}

#[must_use]
pub fn render_inline_lines(
    state: &InlineTerminalState,
    snapshot: &InlineStatusSnapshot,
    width: usize,
) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }

    match state.mode {
        TerminalDisplayMode::FullTui => vec![fit_width(
            &format!(
                "Forge full TUI active (press '{}' for inline)",
                state.toggle_hotkey
            ),
            width,
        )],
        TerminalDisplayMode::InlineSingle => {
            vec![fit_width(
                &render_primary_status_line(state, snapshot),
                width,
            )]
        }
        TerminalDisplayMode::InlineTriple => render_triple_lines(state, snapshot, width),
    }
}

fn render_primary_status_line(
    state: &InlineTerminalState,
    snapshot: &InlineStatusSnapshot,
) -> String {
    let selected_loop = snapshot
        .selected_loop_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("-");
    let selected_tab = snapshot
        .selected_tab
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("overview");

    format!(
        "forge:inline loops={} degraded={} queue={} runs={} unread={} loop={} tab={} status={} [{}]",
        snapshot.active_loops,
        snapshot.degraded_loops,
        snapshot.queue_depth,
        snapshot.running_runs,
        snapshot.unread_fmail,
        selected_loop,
        selected_tab,
        normalize_text(&snapshot.status_text),
        state.toggle_hotkey
    )
}

fn render_triple_lines(
    state: &InlineTerminalState,
    snapshot: &InlineStatusSnapshot,
    width: usize,
) -> Vec<String> {
    let selected_loop = snapshot
        .selected_loop_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("-");
    let selected_tab = snapshot
        .selected_tab
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("overview");

    vec![
        fit_width(
            &format!(
                "forge:inline(3) loops:{} degraded:{} queue:{} runs:{} unread:{}",
                snapshot.active_loops,
                snapshot.degraded_loops,
                snapshot.queue_depth,
                snapshot.running_runs,
                snapshot.unread_fmail
            ),
            width,
        ),
        fit_width(
            &format!(
                "focus loop:{} tab:{} last_full_tab:{}",
                selected_loop, selected_tab, state.last_full_tab
            ),
            width,
        ),
        fit_width(
            &format!(
                "status:{} | toggle:{} | density-cycle:d",
                normalize_text(&snapshot.status_text),
                state.toggle_hotkey
            ),
            width,
        ),
    ]
}

fn normalize_text(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        "ok".to_owned()
    } else {
        normalized
    }
}

fn fit_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let chars = value.chars().count();
    if chars <= width {
        return value.to_owned();
    }
    if width <= 3 {
        return value.chars().take(width).collect();
    }
    let mut out: String = value.chars().take(width - 3).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::{
        cycle_inline_density, render_inline_lines, toggle_inline_full_mode, InlineStatusSnapshot,
        InlineTerminalState, TerminalDisplayMode,
    };

    fn sample_snapshot() -> InlineStatusSnapshot {
        InlineStatusSnapshot {
            active_loops: 12,
            degraded_loops: 2,
            queue_depth: 8,
            running_runs: 5,
            unread_fmail: 3,
            selected_loop_id: Some("loop-a".to_owned()),
            selected_tab: Some("logs".to_owned()),
            status_text: "investigating parser timeout".to_owned(),
        }
    }

    #[test]
    fn toggle_inline_full_mode_is_one_keypress() {
        let mut state = InlineTerminalState::default();
        let outcome = toggle_inline_full_mode(&mut state);
        assert_eq!(outcome.keypress_count, 1);
        assert_eq!(state.mode, TerminalDisplayMode::FullTui);

        let outcome = toggle_inline_full_mode(&mut state);
        assert_eq!(outcome.keypress_count, 1);
        assert_eq!(state.mode, TerminalDisplayMode::InlineSingle);
    }

    #[test]
    fn cycle_density_switches_single_and_triple_inline_modes() {
        let mut state = InlineTerminalState::default();
        assert_eq!(state.mode, TerminalDisplayMode::InlineSingle);
        cycle_inline_density(&mut state);
        assert_eq!(state.mode, TerminalDisplayMode::InlineTriple);
        cycle_inline_density(&mut state);
        assert_eq!(state.mode, TerminalDisplayMode::InlineSingle);
    }

    #[test]
    fn cycle_density_noops_in_full_mode() {
        let mut state = InlineTerminalState {
            mode: TerminalDisplayMode::FullTui,
            ..InlineTerminalState::default()
        };
        cycle_inline_density(&mut state);
        assert_eq!(state.mode, TerminalDisplayMode::FullTui);
    }

    #[test]
    fn render_inline_single_line_includes_core_metrics() {
        let state = InlineTerminalState::default();
        let lines = render_inline_lines(&state, &sample_snapshot(), 200);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("loops=12"));
        assert!(lines[0].contains("degraded=2"));
        assert!(lines[0].contains("queue=8"));
        assert!(lines[0].contains("tab=logs"));
    }

    #[test]
    fn render_inline_triple_line_shows_focus_and_status() {
        let state = InlineTerminalState {
            mode: TerminalDisplayMode::InlineTriple,
            ..InlineTerminalState::default()
        };
        let lines = render_inline_lines(&state, &sample_snapshot(), 200);
        assert_eq!(lines.len(), 3);
        assert!(lines[1].contains("focus loop:loop-a"));
        assert!(lines[2].contains("investigating parser timeout"));
    }
}
