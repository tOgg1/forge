//! Tmux-aware integration helpers for pane-native operator workflows.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TmuxContext {
    pub in_tmux: bool,
    pub socket_path: Option<String>,
    pub session_name: Option<String>,
    pub window_id: Option<String>,
    pub pane_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneDirection {
    Left,
    Right,
    Up,
    Down,
}

impl PaneDirection {
    #[must_use]
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Left => "{left}",
            Self::Right => "{right}",
            Self::Up => "{up}",
            Self::Down => "{down}",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOrientation {
    Horizontal,
    Vertical,
}

impl SplitOrientation {
    #[must_use]
    pub fn flag(self) -> &'static str {
        match self {
            Self::Horizontal => "-h",
            Self::Vertical => "-v",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxCommandPlan {
    pub supported: bool,
    pub keypress_count: u8,
    pub commands: Vec<String>,
    pub fallback_message: String,
    pub notes: Vec<String>,
}

#[must_use]
pub fn detect_tmux_context(env: &[(String, String)]) -> TmuxContext {
    let get = |name: &str| {
        env.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };

    let tmux_env = get("TMUX");
    let pane_id = get("TMUX_PANE");
    let in_tmux = tmux_env.is_some() || pane_id.is_some();

    let socket_path = tmux_env
        .as_deref()
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    TmuxContext {
        in_tmux,
        socket_path,
        session_name: get("TMUX_SESSION"),
        window_id: get("TMUX_WINDOW_ID"),
        pane_id,
    }
}

#[must_use]
pub fn build_send_log_to_adjacent_pane_plan(
    context: &TmuxContext,
    direction: PaneDirection,
    log_lines: &[String],
) -> TmuxCommandPlan {
    if !context.in_tmux {
        return unsupported_plan("Not running inside tmux; cannot send to adjacent pane");
    }
    let Some(pane_id) = context.pane_id.as_deref() else {
        return unsupported_plan("TMUX_PANE missing; cannot resolve adjacent pane target");
    };

    let payload = if log_lines.is_empty() {
        "(no log lines)".to_owned()
    } else {
        log_lines
            .iter()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .take(40)
            .collect::<Vec<_>>()
            .join("\\n")
    };
    let escaped = shell_single_quote(&payload);
    let target = format!("{}{}", pane_id, direction.suffix());

    TmuxCommandPlan {
        supported: true,
        keypress_count: 1,
        commands: vec![format!(
            "tmux send-keys -t '{}' -l '{}' Enter",
            target, escaped
        )],
        fallback_message: String::new(),
        notes: vec![
            "sends selected log payload to adjacent pane".to_owned(),
            "one-key action in tmux context".to_owned(),
        ],
    }
}

#[must_use]
pub fn build_open_run_details_split_plan(
    context: &TmuxContext,
    run_id: &str,
    orientation: SplitOrientation,
    split_percent: u8,
) -> TmuxCommandPlan {
    if !context.in_tmux {
        return unsupported_plan("Not running inside tmux; cannot open split pane");
    }

    let run_id = run_id.trim();
    if run_id.is_empty() {
        return unsupported_plan("run id is empty");
    }

    let percent = split_percent.clamp(20, 80);
    let escaped_run = shell_single_quote(run_id);

    TmuxCommandPlan {
        supported: true,
        keypress_count: 2,
        commands: vec![
            format!("tmux split-window {} -p {}", orientation.flag(), percent),
            format!(
                "tmux send-keys -t '{{last}}' -l 'forge workflow logs {}' Enter",
                escaped_run
            ),
        ],
        fallback_message: String::new(),
        notes: vec![
            "open run details in adjacent split in <=2 keypresses".to_owned(),
            format!(
                "split orientation={} size={}pct",
                orientation.flag(),
                percent
            ),
        ],
    }
}

#[must_use]
pub fn build_share_clipboard_via_tmux_buffer_plan(
    context: &TmuxContext,
    text: &str,
) -> TmuxCommandPlan {
    if !context.in_tmux {
        return unsupported_plan("Not running inside tmux; clipboard share unavailable");
    }

    let payload = text.trim();
    if payload.is_empty() {
        return unsupported_plan("clipboard payload is empty");
    }
    let escaped = shell_single_quote(payload);

    TmuxCommandPlan {
        supported: true,
        keypress_count: 1,
        commands: vec![
            format!("tmux set-buffer -- '{}'", escaped),
            "tmux display-message 'Copied payload into tmux buffer'".to_owned(),
        ],
        fallback_message: String::new(),
        notes: vec![
            "shares payload through tmux buffer".to_owned(),
            "operator can paste in any pane".to_owned(),
        ],
    }
}

#[must_use]
pub fn render_tmux_plan_lines(
    plan: &TmuxCommandPlan,
    width: usize,
    max_lines: usize,
) -> Vec<String> {
    if width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    lines.push(fit_width(
        &format!(
            "tmux integration: {} keypresses={} commands={}",
            if plan.supported {
                "supported"
            } else {
                "fallback"
            },
            plan.keypress_count,
            plan.commands.len()
        ),
        width,
    ));

    if !plan.fallback_message.is_empty() {
        lines.push(fit_width(
            &format!("fallback: {}", plan.fallback_message),
            width,
        ));
    }

    for command in &plan.commands {
        if lines.len() >= max_lines {
            break;
        }
        lines.push(fit_width(&format!("cmd: {}", command), width));
    }

    for note in &plan.notes {
        if lines.len() >= max_lines {
            break;
        }
        lines.push(fit_width(&format!("note: {}", note), width));
    }

    lines.into_iter().take(max_lines).collect()
}

fn unsupported_plan(message: &str) -> TmuxCommandPlan {
    TmuxCommandPlan {
        supported: false,
        keypress_count: 0,
        commands: Vec::new(),
        fallback_message: message.to_owned(),
        notes: vec!["graceful degradation to non-tmux workflow".to_owned()],
    }
}

fn shell_single_quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "'\\''")
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
mod tests {
    use super::{
        build_open_run_details_split_plan, build_send_log_to_adjacent_pane_plan,
        build_share_clipboard_via_tmux_buffer_plan, detect_tmux_context, render_tmux_plan_lines,
        PaneDirection, SplitOrientation,
    };

    #[test]
    fn detect_tmux_context_from_env() {
        let env = vec![
            ("TMUX".to_owned(), "/tmp/tmux-1000/default,123,0".to_owned()),
            ("TMUX_PANE".to_owned(), "%7".to_owned()),
            ("TMUX_SESSION".to_owned(), "forge-main".to_owned()),
        ];

        let context = detect_tmux_context(&env);
        assert!(context.in_tmux);
        assert_eq!(
            context.socket_path.as_deref(),
            Some("/tmp/tmux-1000/default")
        );
        assert_eq!(context.pane_id.as_deref(), Some("%7"));
        assert_eq!(context.session_name.as_deref(), Some("forge-main"));
    }

    #[test]
    fn detect_tmux_context_handles_absent_env() {
        let context = detect_tmux_context(&[]);
        assert!(!context.in_tmux);
        assert!(context.pane_id.is_none());
    }

    #[test]
    fn send_log_plan_uses_adjacent_target_and_is_one_keypress() {
        let context = detect_tmux_context(&[
            ("TMUX".to_owned(), "sock,1,0".to_owned()),
            ("TMUX_PANE".to_owned(), "%3".to_owned()),
        ]);
        let plan = build_send_log_to_adjacent_pane_plan(
            &context,
            PaneDirection::Right,
            &["error: timeout".to_owned(), "retrying".to_owned()],
        );

        assert!(plan.supported);
        assert_eq!(plan.keypress_count, 1);
        assert_eq!(plan.commands.len(), 1);
        assert!(plan.commands[0].contains("%3{right}"));
        assert!(plan.commands[0].contains("error: timeout"));
    }

    #[test]
    fn open_run_details_plan_meets_two_keypress_target() {
        let context = detect_tmux_context(&[("TMUX_PANE".to_owned(), "%1".to_owned())]);
        let plan =
            build_open_run_details_split_plan(&context, "run-42", SplitOrientation::Horizontal, 45);

        assert!(plan.supported);
        assert_eq!(plan.keypress_count, 2);
        assert_eq!(plan.commands.len(), 2);
        assert!(plan.commands[0].contains("split-window -h -p 45"));
        assert!(plan.commands[1].contains("forge workflow logs run-42"));
    }

    #[test]
    fn clipboard_share_gracefully_degrades_outside_tmux() {
        let context = detect_tmux_context(&[]);
        let plan = build_share_clipboard_via_tmux_buffer_plan(&context, "hello");

        assert!(!plan.supported);
        assert!(plan.fallback_message.contains("Not running inside tmux"));

        let lines = render_tmux_plan_lines(&plan, 120, 4);
        assert!(lines.iter().any(|line| line.contains("fallback")));
    }
}
