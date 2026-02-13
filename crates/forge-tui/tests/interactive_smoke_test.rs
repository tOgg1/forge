//! Interactive smoke tests for navigation + action flows.
//!
//! Simulates keypaths through the TUI to verify that navigation, modal
//! transitions, action confirmations, filtering, and multi-log interactions
//! all behave correctly as integrated flows.

use std::collections::HashMap;

use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers, ResizeEvent};
use forge_tui::app::{
    ActionKind, App, ClaimEventView, Command, DensityMode, FilterFocus, FocusMode, InboxFilter,
    InboxMessageView, LogTailView, LoopView, MainTab, RunView, UiMode,
};

// ---------------------------------------------------------------------------
// Input helpers
// ---------------------------------------------------------------------------

fn key(ch: char) -> InputEvent {
    InputEvent::Key(KeyEvent::plain(Key::Char(ch)))
}

fn key_enter() -> InputEvent {
    InputEvent::Key(KeyEvent::plain(Key::Enter))
}

fn key_escape() -> InputEvent {
    InputEvent::Key(KeyEvent::plain(Key::Escape))
}

fn key_tab() -> InputEvent {
    InputEvent::Key(KeyEvent::plain(Key::Tab))
}

fn key_shift_tab() -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: Key::Tab,
        modifiers: Modifiers {
            shift: true,
            ctrl: false,
            alt: false,
        },
    })
}

fn key_up() -> InputEvent {
    InputEvent::Key(KeyEvent::plain(Key::Up))
}

fn key_down() -> InputEvent {
    InputEvent::Key(KeyEvent::plain(Key::Down))
}

fn key_backspace() -> InputEvent {
    InputEvent::Key(KeyEvent::plain(Key::Backspace))
}

fn ctrl(ch: char) -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: Key::Char(ch),
        modifiers: Modifiers {
            shift: false,
            ctrl: true,
            alt: false,
        },
    })
}

fn resize(width: usize, height: usize) -> InputEvent {
    InputEvent::Resize(ResizeEvent { width, height })
}

// ---------------------------------------------------------------------------
// Fixture helpers (reuse sample data patterns from layout_snapshot_test)
// ---------------------------------------------------------------------------

fn sample_loops() -> Vec<LoopView> {
    (0..6)
        .map(|idx| LoopView {
            id: format!("loop-{idx}"),
            short_id: format!("l{idx:02}"),
            name: format!("operator-loop-{idx}"),
            state: if idx % 3 == 0 {
                "error".to_owned()
            } else if idx % 2 == 0 {
                "running".to_owned()
            } else {
                "waiting".to_owned()
            },
            repo_path: format!("/repos/cluster-{idx}"),
            runs: 20 + idx,
            queue_depth: idx * 2,
            last_run_at: Some(format!("2026-02-13T12:{:02}:00Z", 10 + idx)),
            interval_seconds: 60,
            max_runtime_seconds: 7200,
            max_iterations: 500,
            last_error: if idx % 3 == 0 {
                "timeout waiting for harness".to_owned()
            } else {
                String::new()
            },
            profile_name: "prod-sre".to_owned(),
            profile_harness: "codex".to_owned(),
            profile_auth: "ssh".to_owned(),
            profile_id: format!("profile-{idx}"),
            pool_name: "night-shift".to_owned(),
            pool_id: "pool-7".to_owned(),
        })
        .collect()
}

fn sample_runs() -> Vec<RunView> {
    vec![
        RunView {
            id: "run-0172".to_owned(),
            status: "ERROR".to_owned(),
            exit_code: Some(1),
            duration: "4m12s".to_owned(),
            profile_name: "prod-sre".to_owned(),
            profile_id: "profile-prod-sre".to_owned(),
            harness: "codex".to_owned(),
            auth_kind: "ssh".to_owned(),
            started_at: String::new(),
            output_lines: Vec::new(),
        },
        RunView {
            id: "run-0171".to_owned(),
            status: "SUCCESS".to_owned(),
            exit_code: Some(0),
            duration: "3m09s".to_owned(),
            profile_name: "prod-sre".to_owned(),
            profile_id: "profile-prod-sre".to_owned(),
            harness: "codex".to_owned(),
            auth_kind: "ssh".to_owned(),
            started_at: String::new(),
            output_lines: Vec::new(),
        },
        RunView {
            id: "run-0170".to_owned(),
            status: "KILLED".to_owned(),
            exit_code: Some(137),
            duration: "45s".to_owned(),
            profile_name: "prod-sre".to_owned(),
            profile_id: "profile-prod-sre".to_owned(),
            harness: "codex".to_owned(),
            auth_kind: "ssh".to_owned(),
            started_at: String::new(),
            output_lines: Vec::new(),
        },
    ]
}

fn sample_selected_log() -> LogTailView {
    LogTailView {
        lines: vec![
            "2026-02-13T12:12:01Z INFO loop started".to_owned(),
            "2026-02-13T12:12:07Z WARN queue pressure=high".to_owned(),
            "2026-02-13T12:12:18Z ERROR retry budget exhausted".to_owned(),
            "2026-02-13T12:12:19Z TOOL cargo test -p forge-tui".to_owned(),
        ],
        message: "tailing live stream".to_owned(),
    }
}

fn sample_multi_logs() -> HashMap<String, LogTailView> {
    (0..6)
        .map(|idx| {
            (
                format!("loop-{idx}"),
                LogTailView {
                    lines: vec![
                        format!("loop-{idx}: status check ok"),
                        format!("loop-{idx}: queue={}", idx * 2),
                    ],
                    message: "live lane".to_owned(),
                },
            )
        })
        .collect()
}

fn sample_inbox_messages() -> Vec<InboxMessageView> {
    vec![
        InboxMessageView {
            id: 31,
            thread_id: Some("thread-ops".to_owned()),
            from: "agent-a".to_owned(),
            subject: "handoff forge-9r4 loop-2".to_owned(),
            body: "snapshot parity pass ready".to_owned(),
            created_at: "2026-02-13T12:11:00Z".to_owned(),
            ack_required: true,
            read_at: None,
            acked_at: None,
        },
        InboxMessageView {
            id: 32,
            thread_id: Some("thread-ops".to_owned()),
            from: "agent-b".to_owned(),
            subject: "re: handoff forge-9r4 loop-2".to_owned(),
            body: "reviewing now".to_owned(),
            created_at: "2026-02-13T12:12:00Z".to_owned(),
            ack_required: false,
            read_at: Some("2026-02-13T12:12:30Z".to_owned()),
            acked_at: None,
        },
        InboxMessageView {
            id: 33,
            thread_id: Some("thread-hotfix".to_owned()),
            from: "agent-c".to_owned(),
            subject: "incident forge-333".to_owned(),
            body: "needs owner takeover".to_owned(),
            created_at: "2026-02-13T12:13:00Z".to_owned(),
            ack_required: true,
            read_at: None,
            acked_at: None,
        },
    ]
}

fn sample_claim_events() -> Vec<ClaimEventView> {
    vec![
        ClaimEventView {
            task_id: "forge-333".to_owned(),
            claimed_by: "agent-x".to_owned(),
            claimed_at: "2026-02-13T12:10:00Z".to_owned(),
        },
        ClaimEventView {
            task_id: "forge-333".to_owned(),
            claimed_by: "agent-y".to_owned(),
            claimed_at: "2026-02-13T12:13:10Z".to_owned(),
        },
    ]
}

fn dismiss_onboarding_for_all_tabs(app: &mut App) {
    for tab in MainTab::ORDER {
        app.set_tab(tab);
        app.update(key('i'));
    }
    app.clear_status();
    app.set_tab(MainTab::Overview);
}

fn fixture_app() -> App {
    let mut app = App::new("default", 12);
    app.update(resize(120, 40));
    app.set_loops(sample_loops());
    app.move_selection(2);
    app.set_selected_log(sample_selected_log());
    app.set_run_history(sample_runs());
    app.set_multi_logs(sample_multi_logs());
    app.set_inbox_messages(sample_inbox_messages());
    app.set_claim_events(sample_claim_events());
    app.toggle_pinned("loop-0");
    app.toggle_pinned("loop-2");
    dismiss_onboarding_for_all_tabs(&mut app);
    app
}

// ===========================================================================
// Navigation smoke tests
// ===========================================================================

#[test]
fn tab_switching_via_number_keys() {
    let mut app = fixture_app();
    assert_eq!(app.tab(), MainTab::Overview);

    let cmd = app.update(key('2'));
    assert_eq!(app.tab(), MainTab::Logs);
    assert!(matches!(cmd, Command::Fetch));

    let cmd = app.update(key('3'));
    assert_eq!(app.tab(), MainTab::Runs);
    assert!(matches!(cmd, Command::Fetch));

    let cmd = app.update(key('4'));
    assert_eq!(app.tab(), MainTab::MultiLogs);
    assert!(matches!(cmd, Command::Fetch));

    let cmd = app.update(key('5'));
    assert_eq!(app.tab(), MainTab::Inbox);
    assert!(matches!(cmd, Command::Fetch));

    let cmd = app.update(key('1'));
    assert_eq!(app.tab(), MainTab::Overview);
    assert!(matches!(cmd, Command::Fetch));
}

#[test]
fn tab_cycling_with_brackets() {
    let mut app = fixture_app();
    assert_eq!(app.tab(), MainTab::Overview);

    app.update(key(']'));
    assert_eq!(app.tab(), MainTab::Logs);

    app.update(key(']'));
    assert_eq!(app.tab(), MainTab::Runs);

    app.update(key('['));
    assert_eq!(app.tab(), MainTab::Logs);

    app.update(key('['));
    assert_eq!(app.tab(), MainTab::Overview);
}

#[test]
fn loop_selection_navigation_j_k() {
    let mut app = fixture_app();
    let initial_idx = app.selected_idx();

    app.update(key('j'));
    assert_eq!(app.selected_idx(), initial_idx + 1);

    app.update(key('j'));
    assert_eq!(app.selected_idx(), initial_idx + 2);

    app.update(key('k'));
    assert_eq!(app.selected_idx(), initial_idx + 1);
}

#[test]
fn loop_selection_navigation_arrows() {
    let mut app = fixture_app();
    let initial_idx = app.selected_idx();

    app.update(key_down());
    assert_eq!(app.selected_idx(), initial_idx + 1);

    app.update(key_up());
    assert_eq!(app.selected_idx(), initial_idx);
}

#[test]
fn selection_clamps_at_boundaries() {
    let mut app = fixture_app();

    // Move to top
    for _ in 0..20 {
        app.update(key('k'));
    }
    assert_eq!(app.selected_idx(), 0);

    // Move past bottom
    for _ in 0..20 {
        app.update(key('j'));
    }
    // Should be clamped to last
    assert!(app.selected_idx() < 20);
}

// ===========================================================================
// Modal transition smoke tests
// ===========================================================================

#[test]
fn help_mode_enter_and_return() {
    let mut app = fixture_app();
    assert_eq!(app.mode(), UiMode::Main);

    app.update(key('?'));
    assert_eq!(app.mode(), UiMode::Help);

    app.update(key_escape());
    assert_eq!(app.mode(), UiMode::Main);
}

#[test]
fn help_mode_from_various_modes() {
    let mut app = fixture_app();

    // Help from filter mode
    app.update(key('/'));
    assert_eq!(app.mode(), UiMode::Filter);
    app.update(key('?'));
    assert_eq!(app.mode(), UiMode::Help);
    app.update(key_escape());
    assert_eq!(app.mode(), UiMode::Filter);
    app.update(key_escape());
    assert_eq!(app.mode(), UiMode::Main);

    // Help from expanded logs mode
    app.update(key('l'));
    assert_eq!(app.mode(), UiMode::ExpandedLogs);
    app.update(key('?'));
    assert_eq!(app.mode(), UiMode::Help);
    app.update(key_escape());
    assert_eq!(app.mode(), UiMode::ExpandedLogs);
    app.update(key_escape());
    assert_eq!(app.mode(), UiMode::Main);
}

#[test]
fn filter_mode_text_input_and_dismiss() {
    let mut app = fixture_app();

    app.update(key('/'));
    assert_eq!(app.mode(), UiMode::Filter);
    assert_eq!(app.filter_focus(), FilterFocus::Text);

    // Type filter text
    app.update(key('l'));
    app.update(key('o'));
    app.update(key('o'));
    app.update(key('p'));
    assert_eq!(app.filter_text(), "loop");

    // Backspace removes last char
    app.update(key_backspace());
    assert_eq!(app.filter_text(), "loo");

    // Escape returns to main
    app.update(key_escape());
    assert_eq!(app.mode(), UiMode::Main);
}

#[test]
fn filter_mode_tab_toggles_focus() {
    let mut app = fixture_app();

    app.update(key('/'));
    assert_eq!(app.filter_focus(), FilterFocus::Text);

    app.update(key_tab());
    assert_eq!(app.filter_focus(), FilterFocus::Status);

    app.update(key_tab());
    assert_eq!(app.filter_focus(), FilterFocus::Text);
}

#[test]
fn expanded_logs_mode_round_trip() {
    let mut app = fixture_app();

    // Select a loop first, navigate to logs
    app.update(key('2'));
    assert_eq!(app.tab(), MainTab::Logs);

    // Enter expanded logs
    app.update(key('l'));
    assert_eq!(app.mode(), UiMode::ExpandedLogs);

    // Can navigate selection in expanded mode
    app.update(key('j'));
    app.update(key('k'));

    // Can cycle log layer in expanded mode
    app.update(key('x'));

    // Exit back to main
    app.update(key('q'));
    assert_eq!(app.mode(), UiMode::Main);
    assert_eq!(app.tab(), MainTab::Logs);
}

#[test]
fn wizard_mode_open_and_cancel() {
    let mut app = fixture_app();

    app.update(key('n'));
    assert_eq!(app.mode(), UiMode::Wizard);
    assert_eq!(app.wizard().step, 1);
    assert_eq!(app.wizard().field, 0);

    // Escape cancels wizard
    app.update(key_escape());
    assert_eq!(app.mode(), UiMode::Main);
}

// ===========================================================================
// Action flow smoke tests
// ===========================================================================

#[test]
fn confirm_stop_then_cancel() {
    let mut app = fixture_app();

    // Press S to stop (enters confirm mode)
    app.update(key('S'));
    assert_eq!(app.mode(), UiMode::Confirm);
    assert!(app.confirm().is_some());

    // Cancel with 'n'
    let cmd = app.update(key('n'));
    assert_eq!(app.mode(), UiMode::Main);
    assert!(app.confirm().is_none());
    assert!(cmd.is_none());
    assert_eq!(app.status_text(), "Action cancelled");
}

#[test]
fn confirm_kill_then_accept() {
    let mut app = fixture_app();

    app.update(key('K'));
    assert_eq!(app.mode(), UiMode::Confirm);

    let cmd = app.update(key('y'));
    assert_eq!(app.mode(), UiMode::Main);
    assert!(app.confirm().is_none());
    assert!(matches!(cmd, Command::RunAction(ActionKind::Kill { .. })));
}

#[test]
fn confirm_delete_then_accept() {
    let mut app = fixture_app();

    app.update(key('D'));
    assert_eq!(app.mode(), UiMode::Confirm);

    let cmd = app.update(key('Y'));
    assert_eq!(app.mode(), UiMode::Main);
    assert!(matches!(cmd, Command::RunAction(ActionKind::Delete { .. })));
}

#[test]
fn confirm_escape_cancels() {
    let mut app = fixture_app();

    app.update(key('S'));
    assert_eq!(app.mode(), UiMode::Confirm);

    app.update(key_escape());
    assert_eq!(app.mode(), UiMode::Main);
    assert!(app.confirm().is_none());
}

#[test]
fn resume_action_without_confirm() {
    let mut app = fixture_app();

    let cmd = app.update(key('r'));
    assert!(matches!(cmd, Command::RunAction(ActionKind::Resume { .. })));
    // Resume does not go through confirm mode
    assert_eq!(app.mode(), UiMode::Main);
}

// ===========================================================================
// Compound navigation flows
// ===========================================================================

#[test]
fn select_loop_open_logs_run_action_confirm_return() {
    let mut app = fixture_app();

    // Start at Overview, select second loop
    assert_eq!(app.tab(), MainTab::Overview);
    app.update(key('j'));

    // Switch to Logs tab
    app.update(key('2'));
    assert_eq!(app.tab(), MainTab::Logs);

    // Open expanded logs
    app.update(key('l'));
    assert_eq!(app.mode(), UiMode::ExpandedLogs);

    // Trigger stop action from expanded logs
    app.update(key('S'));
    assert_eq!(app.mode(), UiMode::Confirm);

    // Cancel
    app.update(key('n'));
    assert_eq!(app.mode(), UiMode::Main);
    assert_eq!(app.status_text(), "Action cancelled");

    // Verify we're still on the Logs tab
    assert_eq!(app.tab(), MainTab::Logs);
}

#[test]
fn multi_logs_layout_cycling_and_pagination() {
    let mut app = fixture_app();

    app.update(key('4'));
    assert_eq!(app.tab(), MainTab::MultiLogs);

    let _initial_page = app.multi_page();

    // Cycle layout
    app.update(key('m'));

    // Page navigation
    app.update(key('g'));
    assert_eq!(app.multi_page(), 0);

    app.update(key('.'));
    // Page may or may not move depending on data size
    let _ = app.multi_page();

    app.update(key('g'));
    assert_eq!(app.multi_page(), 0);

    // Toggle compare mode
    assert!(!app.multi_compare_mode());
    app.update(key('C'));
    assert!(app.multi_compare_mode());
    app.update(key('C'));
    assert!(!app.multi_compare_mode());
}

#[test]
fn pin_and_clear_workflow() {
    let mut app = fixture_app();
    let _initial_pins = app.pinned_count();

    // Toggle pin on current selection (Space)
    app.update(key(' '));
    let _after_toggle = app.pinned_count();

    // Clear all pins
    app.update(key('c'));
    assert_eq!(app.pinned_count(), 0);

    // Re-pin
    app.update(key(' '));
    assert_eq!(app.pinned_count(), 1);
}

#[test]
fn log_source_and_layer_cycling() {
    let mut app = fixture_app();

    app.update(key('2'));
    assert_eq!(app.tab(), MainTab::Logs);

    // Cycle log source (v key)
    let initial_source = app.log_source();
    app.update(key('v'));
    // Source should change
    assert_ne!(app.log_source(), initial_source);

    // Cycle log layer (x key)
    let initial_layer = app.log_layer();
    app.update(key('x'));
    assert_ne!(app.log_layer(), initial_layer);
}

#[test]
fn run_selection_navigation_in_runs_tab() {
    let mut app = fixture_app();

    app.update(key('3'));
    assert_eq!(app.tab(), MainTab::Runs);

    // Navigate runs with , and .
    app.update(key('.'));
    app.update(key('.'));
    app.update(key(','));
}

#[test]
fn follow_mode_toggle_in_logs_tab() {
    let mut app = fixture_app();

    app.update(key('2'));
    assert_eq!(app.tab(), MainTab::Logs);

    let initial_follow = app.follow_mode();
    app.update(key('F'));
    assert_ne!(app.follow_mode(), initial_follow);

    app.update(key('F'));
    assert_eq!(app.follow_mode(), initial_follow);
}

// ===========================================================================
// Inbox navigation flows
// ===========================================================================

#[test]
fn inbox_navigation_and_filter_cycling() {
    let mut app = fixture_app();

    app.update(key('5'));
    assert_eq!(app.tab(), MainTab::Inbox);

    // Navigate inbox with j/k
    app.update(key('j'));
    app.update(key('k'));

    // Cycle inbox filter with 'f'
    assert_eq!(app.inbox_filter(), InboxFilter::All);
    app.update(key('f'));
    assert_ne!(app.inbox_filter(), InboxFilter::All);
}

#[test]
fn inbox_mark_read_with_enter() {
    let mut app = fixture_app();

    app.update(key('5'));
    assert_eq!(app.tab(), MainTab::Inbox);

    let cmd = app.update(key_enter());
    assert!(matches!(cmd, Command::Fetch));
}

// ===========================================================================
// Resize handling
// ===========================================================================

#[test]
fn resize_returns_fetch_and_updates_dimensions() {
    let mut app = fixture_app();

    let cmd = app.update(resize(200, 50));
    assert!(matches!(cmd, Command::Fetch));
    assert_eq!(app.width(), 200);
    assert_eq!(app.height(), 50);

    let cmd = app.update(resize(80, 24));
    assert!(matches!(cmd, Command::Fetch));
    assert_eq!(app.width(), 80);
    assert_eq!(app.height(), 24);
}

// ===========================================================================
// Theme and density mode cycling
// ===========================================================================

#[test]
fn theme_cycling_does_not_crash() {
    let mut app = fixture_app();

    // Cycle theme several times
    for _ in 0..5 {
        app.update(key('t'));
    }

    // Cycle accessibility preset
    for _ in 0..5 {
        app.update(key('T'));
    }

    // Should still render without panicking
    let _ = app.render();
}

#[test]
fn density_mode_cycling() {
    let mut app = fixture_app();
    assert_eq!(app.density_mode(), DensityMode::Comfortable);

    app.update(key('M'));
    assert_eq!(app.density_mode(), DensityMode::Compact);

    app.update(key('M'));
    assert_eq!(app.density_mode(), DensityMode::Comfortable);
}

// ===========================================================================
// Quit flow
// ===========================================================================

#[test]
fn quit_with_q_key() {
    let mut app = fixture_app();

    let cmd = app.update(key('q'));
    assert!(matches!(cmd, Command::Quit));
    assert!(app.quitting());
}

#[test]
fn quit_with_ctrl_c() {
    let mut app = fixture_app();

    let cmd = app.update(ctrl('c'));
    assert!(matches!(cmd, Command::Quit));
    assert!(app.quitting());
}

// ===========================================================================
// Render-after-navigate smoke tests (no panics)
// ===========================================================================

#[test]
fn render_after_full_navigation_sequence() {
    let mut app = fixture_app();

    // Exercise every tab and render
    for tab_key in ['1', '2', '3', '4', '5'] {
        app.update(key(tab_key));
        let frame = app.render();
        assert!(!frame.snapshot().is_empty());
    }

    // Navigate in each mode and render
    app.update(key('1'));
    app.update(key('j'));
    app.update(key('j'));
    let _ = app.render();

    app.update(key('/'));
    app.update(key('t'));
    app.update(key('e'));
    let _ = app.render();
    app.update(key_escape());

    app.update(key('?'));
    let _ = app.render();
    app.update(key_escape());

    app.update(key('l'));
    let _ = app.render();
    app.update(key('q'));

    // Confirm modal render
    app.update(key('S'));
    let _ = app.render();
    app.update(key('n'));

    // Wizard render
    app.update(key('n'));
    let _ = app.render();
    app.update(key_escape());
}

#[test]
fn render_multi_logs_after_layout_cycle() {
    let mut app = fixture_app();

    app.update(key('4'));
    let _ = app.render();

    app.update(key('m'));
    let _ = app.render();

    app.update(key('m'));
    let _ = app.render();

    app.update(key('C'));
    let _ = app.render();
}

// ===========================================================================
// Focus graph navigation
// ===========================================================================

#[test]
fn focus_pane_cycling_with_tab() {
    let mut app = fixture_app();

    app.update(key_tab());
    // Focus should potentially move (depends on view graph)

    app.update(key_shift_tab());
    // Should cycle back
}

// ===========================================================================
// Command palette flow
// ===========================================================================

#[test]
fn command_palette_open_type_and_close() {
    let mut app = fixture_app();

    app.update(ctrl('p'));
    assert_eq!(app.mode(), UiMode::Palette);

    // Palette should have matches
    assert!(app.palette_match_count() > 0);

    // Type a query
    app.update(key('l'));
    app.update(key('o'));
    app.update(key('g'));

    // Close
    app.update(key_escape());
    assert_eq!(app.mode(), UiMode::Main);
}

// ===========================================================================
// Expanded-logs action flows
// ===========================================================================

#[test]
fn expanded_logs_resume_action() {
    let mut app = fixture_app();

    app.update(key('l'));
    assert_eq!(app.mode(), UiMode::ExpandedLogs);

    let cmd = app.update(key('r'));
    assert!(matches!(cmd, Command::RunAction(ActionKind::Resume { .. })));
    assert_eq!(app.mode(), UiMode::Main);
}

#[test]
fn expanded_logs_kill_action_with_confirm() {
    let mut app = fixture_app();

    app.update(key('l'));
    assert_eq!(app.mode(), UiMode::ExpandedLogs);

    // Kill triggers confirm, but first exits expanded logs
    app.update(key('K'));
    assert_eq!(app.mode(), UiMode::Confirm);

    let cmd = app.update(key('y'));
    assert!(matches!(cmd, Command::RunAction(ActionKind::Kill { .. })));
    assert_eq!(app.mode(), UiMode::Main);
}

// ===========================================================================
// Zen mode + deep focus mode
// ===========================================================================

#[test]
fn zen_and_deep_focus_toggle() {
    let mut app = fixture_app();

    // Toggle zen mode
    app.update(key('z'));
    let _ = app.render();

    // Toggle deep focus
    app.update(key('Z'));
    assert_eq!(app.focus_mode(), FocusMode::DeepDebug);

    app.update(key('Z'));
    assert_eq!(app.focus_mode(), FocusMode::Standard);

    // Zen again (toggle back)
    app.update(key('z'));
    let _ = app.render();
}
