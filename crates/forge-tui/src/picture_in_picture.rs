//! Terminal picture-in-picture (PiP) window state + placement helpers.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PiPSource {
    Panel(String),
    LoopHealth { loop_id: String },
    LoopLogs { loop_id: String },
    InboxThread { thread_id: String },
    RunsSummary,
}

impl PiPSource {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Panel(name) => normalize_text(name),
            Self::LoopHealth { loop_id } => format!("loop-health:{}", normalize_id(loop_id)),
            Self::LoopLogs { loop_id } => format!("loop-logs:{}", normalize_id(loop_id)),
            Self::InboxThread { thread_id } => {
                format!("inbox-thread:{}", normalize_id(thread_id))
            }
            Self::RunsSummary => "runs-summary".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiPAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiPWindow {
    pub id: String,
    pub source: PiPSource,
    pub title: String,
    pub lines: Vec<String>,
    pub width: usize,
    pub height: usize,
    pub opacity_percent: u8,
    pub anchor: PiPAnchor,
    pub offset_x: usize,
    pub offset_y: usize,
    pub collapsed: bool,
    pub updated_at_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiPState {
    pub windows: Vec<PiPWindow>,
    pub focused_window_id: Option<String>,
    pub max_windows: usize,
    next_id: u64,
}

impl Default for PiPState {
    fn default() -> Self {
        Self {
            windows: Vec::new(),
            focused_window_id: None,
            max_windows: 4,
            next_id: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiPRenderWindow {
    pub id: String,
    pub title: String,
    pub anchor: PiPAnchor,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub opacity_percent: u8,
    pub focused: bool,
    pub lines: Vec<String>,
}

pub fn pin_pip_window(
    state: &mut PiPState,
    source: PiPSource,
    title: &str,
    lines: &[String],
    anchor: PiPAnchor,
    now_epoch_s: i64,
) -> String {
    let source_key = source.label();
    if let Some(existing) = state
        .windows
        .iter_mut()
        .find(|window| window.source.label() == source_key)
    {
        existing.title = resolve_title(title, &source);
        existing.lines = normalize_lines(lines);
        existing.anchor = anchor;
        existing.updated_at_epoch_s = now_epoch_s.max(0);
        state.focused_window_id = Some(existing.id.clone());
        return existing.id.clone();
    }

    if state.windows.len() >= state.max_windows.max(1) {
        state.windows.remove(0);
    }

    let id = format!("pip-{}", state.next_id);
    state.next_id = state.next_id.saturating_add(1);

    let window = PiPWindow {
        id: id.clone(),
        source,
        title: resolve_title(title, &PiPSource::Panel(source_key.clone())),
        lines: normalize_lines(lines),
        width: 42,
        height: 9,
        opacity_percent: 85,
        anchor,
        offset_x: 0,
        offset_y: 0,
        collapsed: false,
        updated_at_epoch_s: now_epoch_s.max(0),
    };
    state.windows.push(window);
    state.focused_window_id = Some(id.clone());

    id
}

pub fn unpin_pip_window(state: &mut PiPState, id: &str) -> bool {
    let target = normalize_id(id);
    let before = state.windows.len();
    state
        .windows
        .retain(|window| normalize_id(&window.id) != target);
    let removed = state.windows.len() != before;
    if removed
        && state
            .focused_window_id
            .as_deref()
            .is_some_and(|value| normalize_id(value) == target)
    {
        state.focused_window_id = state.windows.last().map(|window| window.id.clone());
    }
    removed
}

pub fn focus_next_pip_window(state: &mut PiPState) -> Option<String> {
    if state.windows.is_empty() {
        state.focused_window_id = None;
        return None;
    }

    let current = state
        .focused_window_id
        .as_deref()
        .map(normalize_id)
        .unwrap_or_default();
    let mut index = state
        .windows
        .iter()
        .position(|window| normalize_id(&window.id) == current)
        .unwrap_or(0);
    index = (index + 1) % state.windows.len();
    let next = state.windows[index].id.clone();
    state.focused_window_id = Some(next.clone());
    Some(next)
}

pub fn set_pip_opacity(state: &mut PiPState, id: &str, opacity_percent: i32) -> bool {
    let target = normalize_id(id);
    let Some(window) = state
        .windows
        .iter_mut()
        .find(|window| normalize_id(&window.id) == target)
    else {
        return false;
    };
    window.opacity_percent = opacity_percent.clamp(20, 100) as u8;
    true
}

pub fn resize_pip_window(state: &mut PiPState, id: &str, width: i32, height: i32) -> bool {
    let target = normalize_id(id);
    let Some(window) = state
        .windows
        .iter_mut()
        .find(|window| normalize_id(&window.id) == target)
    else {
        return false;
    };
    window.width = width.clamp(20, 120) as usize;
    window.height = height.clamp(4, 30) as usize;
    true
}

pub fn move_pip_window_to_anchor(
    state: &mut PiPState,
    id: &str,
    anchor: PiPAnchor,
    offset_x: i32,
    offset_y: i32,
) -> bool {
    let target = normalize_id(id);
    let Some(window) = state
        .windows
        .iter_mut()
        .find(|window| normalize_id(&window.id) == target)
    else {
        return false;
    };

    window.anchor = anchor;
    window.offset_x = offset_x.max(0) as usize;
    window.offset_y = offset_y.max(0) as usize;
    true
}

pub fn toggle_pip_collapsed(state: &mut PiPState, id: &str) -> bool {
    let target = normalize_id(id);
    let Some(window) = state
        .windows
        .iter_mut()
        .find(|window| normalize_id(&window.id) == target)
    else {
        return false;
    };
    window.collapsed = !window.collapsed;
    true
}

#[must_use]
pub fn render_pip_windows(
    state: &PiPState,
    viewport_width: usize,
    viewport_height: usize,
) -> Vec<PiPRenderWindow> {
    if viewport_width < 10 || viewport_height < 6 {
        return Vec::new();
    }

    let mut stack_tl = 0usize;
    let mut stack_tr = 0usize;
    let mut stack_bl = 0usize;
    let mut stack_br = 0usize;

    let focused = state.focused_window_id.as_deref().map(normalize_id);
    let mut out = Vec::with_capacity(state.windows.len());

    for window in &state.windows {
        let effective_height = if window.collapsed { 3 } else { window.height };
        let width = window.width.min(viewport_width.saturating_sub(2)).max(10);
        let height = effective_height
            .min(viewport_height.saturating_sub(2))
            .max(3);

        let (stack, x, y) = match window.anchor {
            PiPAnchor::TopLeft => {
                let x = 1usize.saturating_add(window.offset_x);
                let y = 1usize
                    .saturating_add(window.offset_y)
                    .saturating_add(stack_tl);
                (stack_tl, x, y)
            }
            PiPAnchor::TopRight => {
                let x = viewport_width
                    .saturating_sub(width.saturating_add(1).saturating_add(window.offset_x));
                let y = 1usize
                    .saturating_add(window.offset_y)
                    .saturating_add(stack_tr);
                (stack_tr, x, y)
            }
            PiPAnchor::BottomLeft => {
                let x = 1usize.saturating_add(window.offset_x);
                let y = viewport_height.saturating_sub(
                    height
                        .saturating_add(1)
                        .saturating_add(window.offset_y)
                        .saturating_add(stack_bl),
                );
                (stack_bl, x, y)
            }
            PiPAnchor::BottomRight => {
                let x = viewport_width
                    .saturating_sub(width.saturating_add(1).saturating_add(window.offset_x));
                let y = viewport_height.saturating_sub(
                    height
                        .saturating_add(1)
                        .saturating_add(window.offset_y)
                        .saturating_add(stack_br),
                );
                (stack_br, x, y)
            }
        };

        let _ = stack;
        match window.anchor {
            PiPAnchor::TopLeft => stack_tl = stack_tl.saturating_add(height + 1),
            PiPAnchor::TopRight => stack_tr = stack_tr.saturating_add(height + 1),
            PiPAnchor::BottomLeft => stack_bl = stack_bl.saturating_add(height + 1),
            PiPAnchor::BottomRight => stack_br = stack_br.saturating_add(height + 1),
        }

        let x = x.min(viewport_width.saturating_sub(width));
        let y = y.min(viewport_height.saturating_sub(height));

        let is_focused = focused
            .as_deref()
            .is_some_and(|value| value == normalize_id(&window.id));
        let lines = render_window_lines(window, width, height, is_focused);

        out.push(PiPRenderWindow {
            id: window.id.clone(),
            title: window.title.clone(),
            anchor: window.anchor,
            x,
            y,
            width,
            height,
            opacity_percent: window.opacity_percent,
            focused: is_focused,
            lines,
        });
    }

    out
}

fn render_window_lines(
    window: &PiPWindow,
    width: usize,
    height: usize,
    focused: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    let focus_prefix = if focused { "*" } else { " " };
    lines.push(fit_width(
        &format!(
            "{} PiP {}% {}",
            focus_prefix, window.opacity_percent, window.title
        ),
        width,
    ));

    if window.collapsed {
        lines.push(fit_width(
            &format!("source:{} [collapsed]", window.source.label()),
            width,
        ));
        lines.push(fit_width("expand to view content", width));
        return lines.into_iter().take(height).collect();
    }

    lines.push(fit_width(
        &format!("source:{}", window.source.label()),
        width,
    ));
    for line in window.lines.iter().take(height.saturating_sub(lines.len())) {
        lines.push(fit_width(line, width));
    }
    lines.into_iter().take(height).collect()
}

fn resolve_title(title: &str, source: &PiPSource) -> String {
    let title = normalize_text(title);
    if title.is_empty() {
        return source.label();
    }
    title
}

fn normalize_lines(lines: &[String]) -> Vec<String> {
    if lines.is_empty() {
        return vec!["(no content)".to_owned()];
    }
    lines
        .iter()
        .map(|line| normalize_text(line))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fit_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let char_count = value.chars().count();
    if char_count <= width {
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
mod tests;
