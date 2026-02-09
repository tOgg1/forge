//! fmail-tui: terminal UI surface for Forge mail workflows.

use forge_ftui_adapter::input::{translate_input, InputEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::{ThemeKind, ThemeSpec};
use forge_ftui_adapter::widgets::{self, TableColumnSpec, WidgetSpec};

pub mod bookmarks;
pub mod live_tail;
pub mod notifications;
pub mod state_help;
pub mod timeline;

pub use bookmarks::{
    apply_bookmarks_input, parse_bookmarks_filter, render_bookmarks_frame, BookmarkEntry,
    BookmarksFilter, BookmarksViewModel,
};
pub use live_tail::{
    apply_live_tail_input, parse_live_tail_filter, render_live_tail_frame, LiveTailFilter,
    LiveTailMessage, LiveTailViewModel, LIVE_TAIL_MAX_MESSAGES,
};
pub use notifications::{
    apply_notifications_input, render_notifications_frame, NotificationItem, NotificationRule,
    NotificationsFocus, NotificationsViewModel, NOTIFICATION_MEMORY_LIMIT,
};
pub use state_help::{
    default_keymap, render_help_frame, Bookmark, KeyBinding, PersistedState, UiPreferences,
};
pub use timeline::{
    apply_timeline_input, parse_timeline_filter, render_timeline_frame, TimelineFilter,
    TimelineMessage, TimelineMode, TimelineViewModel,
};

/// Stable crate label used by bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "fmail-tui"
}

/// fmail TUI default theme comes from the local FrankenTUI adapter abstraction.
#[must_use]
pub fn default_theme() -> ThemeSpec {
    ThemeSpec::for_kind(ThemeKind::HighContrast)
}

/// Build a tiny bootstrap frame via adapter render abstraction.
#[must_use]
pub fn bootstrap_frame() -> RenderFrame {
    let mut frame = RenderFrame::new(
        FrameSize {
            width: 20,
            height: 2,
        },
        default_theme(),
    );
    frame.draw_text(0, 0, "fmail TUI", TextRole::Accent);
    frame.draw_text(0, 1, "mailbox: synced", TextRole::Success);
    frame
}

/// Input mapping is sourced from the adapter event/input abstraction.
#[must_use]
pub fn map_input(event: InputEvent) -> UiAction {
    translate_input(&event)
}

/// Mailbox panel primitives sourced from adapter layer.
#[must_use]
pub fn mailbox_widgets() -> [WidgetSpec; 3] {
    [
        WidgetSpec::fmail_inbox_panel(),
        WidgetSpec::fmail_message_panel(),
        WidgetSpec::fmail_compose_panel(),
    ]
}

/// Mailbox table columns sourced from adapter layer.
#[must_use]
pub fn mailbox_columns() -> [TableColumnSpec; 4] {
    widgets::fmail_inbox_columns()
}

#[cfg(test)]
mod tests {
    use super::{
        bootstrap_frame, crate_label, default_theme, mailbox_columns, mailbox_widgets, map_input,
    };
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, UiAction};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::{StyleToken, ThemeKind};

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "fmail-tui");
    }

    #[test]
    fn uses_adapter_theme_abstraction() {
        let theme = default_theme();
        assert_eq!(theme.kind, ThemeKind::HighContrast);
        assert_eq!(theme.color(StyleToken::Foreground), 231);
    }

    #[test]
    fn uses_adapter_render_abstraction() {
        let frame = bootstrap_frame();
        assert_render_frame_snapshot(
            "fmail_tui_bootstrap_frame",
            &frame,
            "fmail TUI           \nmailbox: synced     ",
        );
    }

    #[test]
    #[ignore]
    fn perf_bootstrap_frame_build() {
        let result = forge_ftui_adapter::perf::measure(10_000, || {
            let _ = bootstrap_frame();
        });
        assert!(result.total.as_nanos() > 0);
    }

    #[test]
    fn uses_adapter_input_abstraction() {
        assert_eq!(
            map_input(InputEvent::Key(KeyEvent::plain(Key::Char('/')))),
            UiAction::Search
        );
        assert_eq!(
            map_input(InputEvent::Key(KeyEvent::plain(Key::Escape))),
            UiAction::Cancel
        );
    }

    #[test]
    fn uses_adapter_widget_primitives_for_fmail_tui() {
        let widgets = mailbox_widgets();
        let snapshot = format!(
            "{}|{}|{}\n{}|{}|{}\n{}|{}|{}",
            widgets[0].id,
            widgets[0].title,
            widgets[0].padding.top,
            widgets[1].id,
            widgets[1].title,
            widgets[1].padding.top,
            widgets[2].id,
            widgets[2].title,
            widgets[2].padding.top,
        );
        assert_eq!(
            snapshot,
            "fmail.inbox|Inbox|1\nfmail.message|Message|0\nfmail.compose|Compose|0"
        );
    }

    #[test]
    fn uses_adapter_mailbox_column_primitives() {
        let columns = mailbox_columns();
        assert_eq!(columns[0].key, "from");
        assert_eq!(columns[1].title, "Subject");
        assert_eq!(columns[3].width, 10);
    }
}
