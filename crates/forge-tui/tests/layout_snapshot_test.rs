use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, ResizeEvent};
use forge_tui::app::{
    App, ClaimEventView, InboxMessageView, LogTailView, LoopView, MainTab, RunView,
};

#[derive(Clone, Copy)]
struct Viewport {
    width: usize,
    height: usize,
}

const VIEWPORTS: [Viewport; 3] = [
    Viewport {
        width: 80,
        height: 24,
    },
    Viewport {
        width: 120,
        height: 40,
    },
    Viewport {
        width: 200,
        height: 50,
    },
];

const SNAPSHOT_TABS: [MainTab; 5] = [
    MainTab::Overview,
    MainTab::Logs,
    MainTab::Runs,
    MainTab::MultiLogs,
    MainTab::Inbox,
];

fn key(ch: char) -> InputEvent {
    InputEvent::Key(KeyEvent::plain(Key::Char(ch)))
}

fn resize(width: usize, height: usize) -> InputEvent {
    InputEvent::Resize(ResizeEvent { width, height })
}

fn tab_slug(tab: MainTab) -> &'static str {
    match tab {
        MainTab::Overview => "overview",
        MainTab::Logs => "logs",
        MainTab::Runs => "runs",
        MainTab::MultiLogs => "multi_logs",
        MainTab::Inbox => "inbox",
    }
}

fn snapshot_name(tab: MainTab, viewport: Viewport) -> String {
    format!("{}_{}x{}", tab_slug(tab), viewport.width, viewport.height)
}

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
            harness: "codex".to_owned(),
            auth_kind: "ssh".to_owned(),
        },
        RunView {
            id: "run-0171".to_owned(),
            status: "SUCCESS".to_owned(),
            exit_code: Some(0),
            duration: "3m09s".to_owned(),
            profile_name: "prod-sre".to_owned(),
            harness: "codex".to_owned(),
            auth_kind: "ssh".to_owned(),
        },
        RunView {
            id: "run-0170".to_owned(),
            status: "KILLED".to_owned(),
            exit_code: Some(137),
            duration: "45s".to_owned(),
            profile_name: "prod-sre".to_owned(),
            harness: "codex".to_owned(),
            auth_kind: "ssh".to_owned(),
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
                        format!("loop-{idx}: deploying worker shard"),
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
        ClaimEventView {
            task_id: "forge-9r4".to_owned(),
            claimed_by: "agent-z".to_owned(),
            claimed_at: "2026-02-13T12:09:00Z".to_owned(),
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

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("layout")
        .join(format!("{name}.txt"))
}

fn assert_layout_snapshot(name: &str, snapshot: &str) {
    let path = golden_path(name);

    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("create dir {}: {err}", parent.display()));
        }
        fs::write(&path, snapshot).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "read {}: {err}; regenerate with UPDATE_GOLDENS=1 cargo test -p forge-tui --test layout_snapshot_test",
            path.display()
        )
    });

    assert_eq!(
        snapshot,
        expected,
        "layout snapshot mismatch ({name})\nexpected file: {}",
        path.display()
    );
}

#[test]
fn key_layout_snapshots_across_breakpoints() {
    for viewport in VIEWPORTS {
        for tab in SNAPSHOT_TABS {
            let mut app = fixture_app();
            app.set_tab(tab);
            app.update(resize(viewport.width, viewport.height));
            let snapshot = app.render().snapshot();
            let name = snapshot_name(tab, viewport);
            assert_layout_snapshot(&name, &snapshot);
        }
    }
}
