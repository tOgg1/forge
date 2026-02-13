use forge_tui::app::{LoopView, RunView};
use forge_tui::hero_widgets::hero_widget_lines;

#[test]
fn throughput_widget_shows_rate_meter_with_trends() {
    let loops = vec![
        LoopView {
            id: "loop-a".to_owned(),
            state: "running".to_owned(),
            ..Default::default()
        },
        LoopView {
            id: "loop-b".to_owned(),
            state: "running".to_owned(),
            ..Default::default()
        },
    ];
    let runs = vec![
        RunView {
            id: "run-1".to_owned(),
            status: "success".to_owned(),
            started_at: "2026-02-13T12:00:01Z".to_owned(),
            output_lines: vec!["ok".to_owned(), "done".to_owned()],
            ..Default::default()
        },
        RunView {
            id: "run-2".to_owned(),
            status: "success".to_owned(),
            started_at: "2026-02-13T12:00:35Z".to_owned(),
            output_lines: vec!["message".to_owned()],
            ..Default::default()
        },
        RunView {
            id: "run-3".to_owned(),
            status: "error".to_owned(),
            started_at: "2026-02-13T12:01:04Z".to_owned(),
            output_lines: vec![
                "error".to_owned(),
                "stack".to_owned(),
                "line".to_owned(),
                "tail".to_owned(),
            ],
            ..Default::default()
        },
        RunView {
            id: "run-4".to_owned(),
            status: "running".to_owned(),
            started_at: "2026-02-13T12:01:41Z".to_owned(),
            output_lines: vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
            ..Default::default()
        },
    ];

    let lines = hero_widget_lines(&loops, &runs, 120);
    let text = lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("r/m 2→"));
    assert!(text.contains("m/m 7↑"));
    assert!(text.contains("e/m 1↑"));
}
