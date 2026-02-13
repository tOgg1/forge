//! Guard pane rendering so one failing pane does not crash the full TUI.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};

use forge_ftui_adapter::render::{FrameSize, Rect, RenderFrame};
use forge_ftui_adapter::style::ThemeSpec;
use forge_ftui_adapter::widgets::BorderStyle;

use crate::theme::ResolvedPalette;

/// Render a pane inside a panic boundary.
///
/// If pane rendering panics, a local fallback panel is rendered instead.
#[must_use]
pub fn render_panel_with_boundary<F>(
    panel_name: &str,
    size: FrameSize,
    theme: ThemeSpec,
    pal: &ResolvedPalette,
    render: F,
) -> RenderFrame
where
    F: FnOnce() -> RenderFrame,
{
    match catch_unwind(AssertUnwindSafe(render)) {
        Ok(frame) => frame,
        Err(payload) => {
            let detail = panic_payload_message(payload);
            render_panel_error_fallback(panel_name, size, theme, pal, &detail)
        }
    }
}

fn render_panel_error_fallback(
    panel_name: &str,
    size: FrameSize,
    theme: ThemeSpec,
    pal: &ResolvedPalette,
    detail: &str,
) -> RenderFrame {
    let mut frame = RenderFrame::new(size, theme);
    if size.width == 0 || size.height == 0 {
        return frame;
    }

    frame.fill_bg(
        Rect {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        },
        pal.background,
    );

    if size.width < 4 || size.height < 4 {
        frame.draw_styled_text(
            0,
            0,
            &trim_to_width("pane render failed", size.width),
            pal.error,
            pal.background,
            true,
        );
        return frame;
    }

    let title = trim_to_width(
        &format!("{panel_name} unavailable"),
        size.width.saturating_sub(2),
    );
    let inner = frame.draw_panel(
        Rect {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        },
        &title,
        BorderStyle::Rounded,
        pal.error,
        pal.panel,
    );

    if inner.width == 0 || inner.height == 0 {
        return frame;
    }

    let lines = [
        ("Pane renderer panicked.", pal.error, true),
        ("App still running with fallback.", pal.text_muted, false),
        (&format!("cause: {detail}"), pal.text_muted, false),
    ];
    for (row, (line, fg, bold)) in lines.into_iter().enumerate() {
        if row >= inner.height {
            break;
        }
        frame.draw_styled_text(
            inner.x,
            inner.y + row,
            &trim_to_width(line, inner.width),
            fg,
            pal.panel,
            bold,
        );
    }

    frame
}

fn panic_payload_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => normalize_message(*message),
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => normalize_message((*message).to_owned()),
            Err(_) => "unknown panic payload".to_owned(),
        },
    }
}

fn normalize_message(message: String) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        "unknown panic payload".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn trim_to_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if value.len() <= width {
        return value.to_owned();
    }
    value.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::render_panel_with_boundary;
    use crate::theme::{
        resolve_palette_colors, resolve_palette_for_capability, TerminalColorCapability,
    };
    use forge_ftui_adapter::render::FrameSize;

    fn test_theme() -> forge_ftui_adapter::style::ThemeSpec {
        crate::theme_for_capability(TerminalColorCapability::TrueColor)
    }

    #[test]
    fn returns_rendered_panel_when_no_panic() {
        let palette = resolve_palette_for_capability("default", TerminalColorCapability::TrueColor);
        let pal = resolve_palette_colors(&palette);
        let frame = render_panel_with_boundary(
            "Logs",
            FrameSize {
                width: 48,
                height: 8,
            },
            test_theme(),
            &pal,
            || {
                let mut frame = forge_ftui_adapter::render::RenderFrame::new(
                    FrameSize {
                        width: 48,
                        height: 8,
                    },
                    test_theme(),
                );
                frame.draw_text(0, 0, "ok", forge_ftui_adapter::render::TextRole::Primary);
                frame
            },
        );
        assert!(frame.snapshot().contains("ok"));
    }

    #[test]
    fn renders_fallback_when_panel_panics() {
        let palette = resolve_palette_for_capability("default", TerminalColorCapability::TrueColor);
        let pal = resolve_palette_colors(&palette);
        let frame = render_panel_with_boundary(
            "Overview",
            FrameSize {
                width: 60,
                height: 10,
            },
            test_theme(),
            &pal,
            || panic!("boom panel"),
        );
        let snapshot = frame.snapshot();
        assert!(snapshot.contains("Overview unavailable"), "{snapshot}");
        assert!(snapshot.contains("Pane renderer panicked."), "{snapshot}");
        assert!(snapshot.contains("cause: boom panel"), "{snapshot}");
    }

    #[test]
    fn tiny_panels_fallback_without_panel_border() {
        let palette = resolve_palette_for_capability("default", TerminalColorCapability::TrueColor);
        let pal = resolve_palette_colors(&palette);
        let frame = render_panel_with_boundary(
            "Runs",
            FrameSize {
                width: 3,
                height: 2,
            },
            test_theme(),
            &pal,
            || panic!("x"),
        );
        assert!(frame.snapshot().contains("pan"), "{}", frame.snapshot());
    }
}
