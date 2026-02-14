use forge_tui::failure_focus::{build_failure_focus, jump_to_probable_root_frame, HighlightRole};
use forge_tui::log_pipeline::{LogPipelineV2, SectionKind, VisibleLine};

#[test]
fn stacktrace_blocks_are_foldable_in_pipeline() {
    let lines = vec![
        "error: task failed".to_owned(),
        "stack backtrace:".to_owned(),
        "   0: std::panicking::begin_panic".to_owned(),
        "   1: forge::runner::execute at src/runner.rs:41".to_owned(),
        "help: rerun with RUST_BACKTRACE=1".to_owned(),
    ];

    let mut pipeline = LogPipelineV2::from_raw_lines(&lines);
    assert_eq!(pipeline.blocks().len(), 3);
    assert_eq!(pipeline.blocks()[1].kind, SectionKind::StackTrace);
    pipeline.fold_all();
    assert!(pipeline.blocks()[1].folded);

    match pipeline.resolve_visible_line(1) {
        Some(VisibleLine::FoldSummary { block_kind, .. }) => {
            assert_eq!(block_kind, SectionKind::StackTrace);
        }
        other => panic!("expected folded stacktrace summary, got {other:?}"),
    }
}

#[test]
fn jump_to_probable_root_frame_prefers_application_frame() {
    let lines = vec![
        "$ cargo test --workspace".to_owned(),
        "thread 'main' panicked at 'boom', src/main.rs:12:5".to_owned(),
        "stack backtrace:".to_owned(),
        "   0: std::panicking::begin_panic".to_owned(),
        "   1: forge::runtime::run at src/runtime.rs:44".to_owned(),
        "   2: forge::main at src/main.rs:12".to_owned(),
        "error: process failed".to_owned(),
    ];

    assert_eq!(jump_to_probable_root_frame(&lines, Some(6)), Some(5));

    let focus = match build_failure_focus(&lines, Some(6)) {
        Some(focus) => focus,
        None => panic!("failure focus should build"),
    };
    assert_eq!(focus.root_frame_line, Some(5));
    assert!(focus
        .highlights
        .iter()
        .any(|item| item.line_index == 5 && item.role == HighlightRole::RootFrame));
    assert!(focus
        .links
        .iter()
        .any(|link| link.line_index == 5 && link.label == "root-frame"));
}
