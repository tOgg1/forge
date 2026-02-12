use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

/// Render the loop TUI help overlay.
///
/// Parity target: `model.renderHelpDialog` in `internal/looptui/looptui.go`.
#[must_use]
pub fn render_help_overlay(width: usize, height: usize, theme: ThemeSpec) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    for (row, line) in help_lines().iter().enumerate() {
        if row >= height {
            break;
        }
        let role = match row {
            0 => TextRole::Accent,
            2 | 8 | 12 | 18 => TextRole::Muted,
            _ => TextRole::Primary,
        };
        frame.draw_text(0, row, &truncate(line, width), role);
    }

    frame
}

#[must_use]
pub fn help_lines() -> Vec<&'static str> {
    vec![
        "Forge TUI Help",
        "",
        "Global:",
        "  q quit | ? toggle help | ]/[ tab cycle | 1..4 jump tabs | t theme | z zen",
        "  j/k or arrows move loop | / filter | l expanded logs | n new loop wizard",
        "  S/K/D stop/kill/delete | r resume | space pin/unpin | c clear pins",
        "  ctrl+f universal search | ctrl+p command palette",
        "",
        "Search (ctrl+f):",
        "  type to search across loops, runs, logs | tab/arrows cycle results",
        "  ctrl+n/ctrl+p next/prev match | enter jump to source | esc close",
        "",
        "Logs + Runs:",
        "  v source cycle (live/latest-run/selected-run)",
        "  x semantic layer cycle (raw/events/errors/tools/diff)",
        "  ,/. previous/next run",
        "  pgup/pgdn/home/end/u/d scroll log output",
        "",
        "Multi Logs:",
        "  m cycle layouts (1x1 -> 4x4)",
        "  ,/. previous/next page | home/end first/last page",
        "",
        "Press q, esc, or ? to close help.",
    ]
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
    use super::{help_lines, render_help_overlay};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    #[test]
    fn help_lines_include_critical_bindings() {
        let joined = help_lines().join("\n");
        assert!(joined.contains("]/[ tab cycle"));
        assert!(joined.contains("1..4 jump tabs"));
        assert!(joined.contains("pgup/pgdn"));
        assert!(joined.contains("Press q, esc, or ?"));
    }

    #[test]
    fn help_overlay_snapshot() {
        let frame = render_help_overlay(64, 10, ThemeSpec::default());
        assert_render_frame_snapshot(
            "forge_tui_help_overlay",
            &frame,
            "Forge TUI Help                                                  \n                                                                \nGlobal:                                                         \n  q quit | ? toggle help | ]/[ tab cycle | 1..4 jump tabs | t t…\n  j/k or arrows move loop | / filter | l expanded logs | n new …\n  S/K/D stop/kill/delete | r resume | space pin/unpin | c clear…\n  ctrl+f universal search | ctrl+p command palette              \n                                                                \nSearch (ctrl+f):                                                \n  type to search across loops, runs, logs | tab/arrows cycle re…",
        );
    }
}
