use forge_tui::error_correlation::{correlate_errors, CorrelationConfig, ErrorSignal};

#[test]
fn clusters_stack_signature_across_six_loops() {
    let mut signals = Vec::new();
    for index in 0..6 {
        signals.push(ErrorSignal {
            loop_id: format!("loop-{index}"),
            loop_name: format!("worker-{index}"),
            message: format!("error: panic in worker {index}"),
            stack_signature: Some("forge::runner::execute -> forge::pool::dispatch".to_owned()),
            observed_at_epoch_s: 1_700_000_000 + index as i64,
        });
    }

    let clusters = correlate_errors(
        &signals,
        CorrelationConfig {
            temporal_window_s: 30,
            similarity_threshold_pct: 60,
        },
    );
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].loop_count, 6);
    assert!(clusters[0].confidence_pct >= 80);
}

#[test]
fn splits_when_temporal_window_is_exceeded() {
    let signals = vec![
        ErrorSignal {
            loop_id: "loop-a".to_owned(),
            loop_name: "A".to_owned(),
            message: "error: timeout waiting for daemon".to_owned(),
            stack_signature: Some("wait_for_daemon".to_owned()),
            observed_at_epoch_s: 100,
        },
        ErrorSignal {
            loop_id: "loop-b".to_owned(),
            loop_name: "B".to_owned(),
            message: "error: timeout waiting for daemon".to_owned(),
            stack_signature: Some("wait_for_daemon".to_owned()),
            observed_at_epoch_s: 500,
        },
    ];

    let clusters = correlate_errors(
        &signals,
        CorrelationConfig {
            temporal_window_s: 60,
            similarity_threshold_pct: 60,
        },
    );
    assert_eq!(clusters.len(), 2);
}
