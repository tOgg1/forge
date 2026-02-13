#![allow(clippy::unwrap_used)]

#[test]
fn runtime_source_enforces_single_root_frankentui_path() {
    let source = include_str!("../src/bin/forge-tui.rs");

    assert!(
        source.contains("run_frankentui_bootstrap()"),
        "expected interactive runtime path to invoke run_frankentui_bootstrap"
    );
    assert!(
        source.contains("ci_non_tty_snapshot_mode_enabled"),
        "expected CI-gated non-interactive snapshot path"
    );

    assert!(
        !source.contains("FORGE_TUI_DEV_SNAPSHOT_FALLBACK"),
        "runtime source reintroduced deprecated snapshot fallback env hook"
    );
    assert!(
        !source.contains("run_interactive_snapshot("),
        "runtime source reintroduced interactive snapshot fallback path"
    );
    assert!(
        !source.contains("dev_snapshot_fallback_enabled("),
        "runtime source reintroduced fallback helper"
    );
}
