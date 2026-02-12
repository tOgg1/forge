//! forge-tui: terminal user interface surface for Forge operators.

use forge_ftui_adapter::input::{translate_input, InputEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::{ThemeKind, ThemeSpec};
use forge_ftui_adapter::widgets::{self, TableColumnSpec, WidgetSpec};

pub mod actions;
pub mod activity_stream;
pub mod analytics_dashboard;
pub mod analytics_fact_model;
pub mod app;
pub mod blocker_graph;
pub mod bulk_action_planner;
pub mod command_palette;
pub mod communication_quality;
pub mod crash_safe_state;
pub mod daily_summary;
pub mod emergency_safe_stop;
pub mod extension_actions;
pub mod extension_api;
pub mod extension_event_bus;
pub mod extension_package_manager;
pub mod extension_reference;
pub mod extension_sandbox;
pub mod failure_focus;
pub mod filter;
pub mod fleet_selection;
pub mod global_search_index;
pub mod help_overlay;
pub mod keyboard_macro;
pub mod keymap;
pub mod lane_model;
pub mod layout_presets;
pub mod layouts;
pub mod log_anchors;
pub mod log_compare;
pub mod log_query;
pub mod logs_tab;
pub mod loop_health_score;
pub mod multi_logs;
pub mod navigation_graph;
pub mod overview_tab;
pub mod performance_gates;
pub mod polling_pipeline;
pub mod readiness_board;
pub mod runs_tab;
pub mod session_restore;
pub mod stale_takeover;
pub mod status_strip;
pub mod swarm_dogpile;
pub mod swarm_governor;
pub mod swarm_stop_monitor;
pub mod swarm_templates;
pub mod swarm_wind_down;
pub mod task_notes;
pub mod task_recommendation;
pub mod theme;
pub mod timeline_scrubber;
/// Stable crate label used by bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "forge-tui"
}

/// Forge TUI default theme comes from the local FrankenTUI adapter abstraction.
#[must_use]
pub fn default_theme() -> ThemeSpec {
    ThemeSpec::for_kind(ThemeKind::Dark)
}

/// Map terminal color capability to adapter theme tokens.
#[must_use]
pub fn theme_for_capability(capability: theme::TerminalColorCapability) -> ThemeSpec {
    match capability {
        theme::TerminalColorCapability::Ansi16 => ThemeSpec::for_kind(ThemeKind::HighContrast),
        theme::TerminalColorCapability::Ansi256 | theme::TerminalColorCapability::TrueColor => {
            ThemeSpec::for_kind(ThemeKind::Dark)
        }
    }
}

/// Resolve runtime theme from current terminal capability hints.
#[must_use]
pub fn detected_theme() -> ThemeSpec {
    let capability = theme::detect_terminal_color_capability();
    theme_for_capability(capability)
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
    frame.draw_text(0, 0, "Forge TUI", TextRole::Accent);
    frame.draw_text(0, 1, "status: ready", TextRole::Primary);
    frame
}

/// Loop dashboard panel primitives sourced from adapter layer.
#[must_use]
pub fn loop_dashboard_widgets() -> [WidgetSpec; 3] {
    [
        WidgetSpec::loop_status_panel(),
        WidgetSpec::loop_queue_panel(),
        WidgetSpec::loop_log_panel(),
    ]
}

/// Queue table columns sourced from adapter layer.
#[must_use]
pub fn loop_queue_columns() -> [TableColumnSpec; 4] {
    widgets::loop_queue_columns()
}

/// Input mapping is sourced from the adapter event/input abstraction.
#[must_use]
pub fn map_input(event: InputEvent) -> UiAction {
    translate_input(&event)
}

#[cfg(test)]
mod tests {
    use super::{
        bootstrap_frame, crate_label, default_theme, loop_dashboard_widgets, loop_queue_columns,
        map_input, theme_for_capability,
    };
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers, UiAction};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::{StyleToken, ThemeKind};

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-tui");
    }

    #[test]
    fn uses_adapter_theme_abstraction() {
        let theme = default_theme();
        assert_eq!(theme.kind, ThemeKind::Dark);
        assert_eq!(theme.color(StyleToken::Accent), 39);
    }

    #[test]
    fn ansi16_uses_high_contrast_theme_tokens() {
        let theme = theme_for_capability(super::theme::TerminalColorCapability::Ansi16);
        assert_eq!(theme.kind, ThemeKind::HighContrast);
    }

    #[test]
    fn uses_adapter_render_abstraction() {
        let frame = bootstrap_frame();
        assert_render_frame_snapshot(
            "forge_tui_bootstrap_frame",
            &frame,
            "Forge TUI           \nstatus: ready       ",
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
    fn uses_adapter_widget_primitives_for_loop_tui() {
        let widgets = loop_dashboard_widgets();
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
            "loop.status|Loop Status|1\nloop.queue|Queue|0\nloop.logs|Recent Logs|0"
        );
    }

    #[test]
    fn uses_adapter_queue_column_primitives() {
        let columns = loop_queue_columns();
        assert_eq!(columns[0].key, "id");
        assert_eq!(columns[1].title, "Status");
        assert_eq!(columns[3].width, 10);
    }

    #[test]
    fn uses_adapter_input_abstraction() {
        assert_eq!(
            map_input(InputEvent::Key(KeyEvent::plain(Key::Up))),
            UiAction::MoveUp
        );
        assert_eq!(
            map_input(InputEvent::Key(KeyEvent {
                key: Key::Char('c'),
                modifiers: Modifiers {
                    shift: false,
                    ctrl: true,
                    alt: false,
                },
            })),
            UiAction::Compose
        );
    }
}
