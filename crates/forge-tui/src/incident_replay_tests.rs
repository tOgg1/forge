use super::{
    build_incident_replay_snapshot, detect_replay_hotspots, reconstruct_timeline,
    IncidentReplaySnapshot, RecordedIncidentEvent, ReplayAnnotation, ReplayControls,
    ReplayEventKind, ReplaySeverity, ReplaySpeed,
};

fn sample_events() -> Vec<RecordedIncidentEvent> {
    vec![
        RecordedIncidentEvent {
            event_id: "evt-1".to_owned(),
            timestamp_ms: 1_000,
            kind: ReplayEventKind::Log,
            severity: ReplaySeverity::Info,
            source: "loop-a".to_owned(),
            summary: "worker started".to_owned(),
        },
        RecordedIncidentEvent {
            event_id: "evt-2".to_owned(),
            timestamp_ms: 2_000,
            kind: ReplayEventKind::Task,
            severity: ReplaySeverity::Warn,
            source: "loop-a".to_owned(),
            summary: "queue age rising".to_owned(),
        },
        RecordedIncidentEvent {
            event_id: "evt-3".to_owned(),
            timestamp_ms: 3_000,
            kind: ReplayEventKind::Alert,
            severity: ReplaySeverity::Critical,
            source: "loop-a".to_owned(),
            summary: "incident declared".to_owned(),
        },
        RecordedIncidentEvent {
            event_id: "evt-4".to_owned(),
            timestamp_ms: 4_000,
            kind: ReplayEventKind::System,
            severity: ReplaySeverity::Error,
            source: "loop-a".to_owned(),
            summary: "safe-stop executed".to_owned(),
        },
    ]
}

#[test]
fn replay_controls_follow_event_range() {
    let controls = ReplayControls::for_events(&sample_events());
    assert_eq!(controls.range_start_ms, 1_000);
    assert_eq!(controls.range_end_ms, 4_000);
    assert_eq!(controls.cursor_ms, 1_000);
    assert!(!controls.playing);
}

#[test]
fn seek_step_and_advance_controls_work() {
    let events = sample_events();
    let mut controls = ReplayControls::for_events(&events);
    controls.seek_ratio(0.5);
    assert_eq!(controls.cursor_ms, 2_500);

    assert!(controls.step_prev_event(&events));
    assert_eq!(controls.cursor_ms, 2_000);
    assert!(controls.step_next_event(&events));
    assert_eq!(controls.cursor_ms, 3_000);
    assert!(controls.step_next_event(&events));
    assert_eq!(controls.cursor_ms, 4_000);
    assert!(!controls.step_next_event(&events));

    controls.cursor_ms = 2_000;
    controls.playing = true;
    controls.speed = ReplaySpeed::X10;
    controls.advance_playback(100);
    assert_eq!(controls.cursor_ms, 3_000);
    controls.advance_playback(200);
    assert_eq!(controls.cursor_ms, 4_000);
    assert!(!controls.playing);
}

#[test]
fn snapshot_sorts_deduplicates_and_filters_visible_state() {
    let mut events = sample_events();
    events.push(RecordedIncidentEvent {
        event_id: "evt-2".to_owned(),
        timestamp_ms: 2_500,
        kind: ReplayEventKind::Task,
        severity: ReplaySeverity::Warn,
        source: "loop-a".to_owned(),
        summary: "duplicate id".to_owned(),
    });

    let annotations = vec![
        ReplayAnnotation {
            annotation_id: "ann-1".to_owned(),
            timestamp_ms: 1_500,
            author: "ops".to_owned(),
            body: "first symptom".to_owned(),
            tags: vec!["root-cause".to_owned(), "root-cause".to_owned()],
        },
        ReplayAnnotation {
            annotation_id: "ann-2".to_owned(),
            timestamp_ms: 3_500,
            author: "ops".to_owned(),
            body: "escalation".to_owned(),
            tags: vec!["p1".to_owned()],
        },
    ];

    let mut controls = ReplayControls::for_events(&events);
    controls.cursor_ms = 3_000;
    let snapshot = build_incident_replay_snapshot(&events, &annotations, controls, 4);
    assert_eq!(snapshot.total_events, 4);
    assert_eq!(snapshot.dropped_duplicate_events, 1);
    assert_eq!(snapshot.visible_events.len(), 3);
    assert_eq!(snapshot.visible_events[0].event_id, "evt-1");
    assert_eq!(snapshot.visible_events[2].event_id, "evt-3");
    assert_eq!(snapshot.visible_annotations.len(), 1);
    assert_eq!(snapshot.visible_annotations[0].annotation_id, "ann-1");
    assert_eq!(snapshot.visible_annotations[0].tags, vec!["root-cause"]);
}

#[test]
fn timeline_reconstruction_and_hotspots_flag_error_windows() {
    let timeline = reconstruct_timeline(&sample_events(), 4);
    assert_eq!(timeline.buckets.len(), 4);
    assert_eq!(timeline.max_error_count, 1);
    let hotspots = detect_replay_hotspots(&timeline);
    assert!(!hotspots.is_empty());
    assert!(hotspots.iter().any(|hotspot| hotspot.error_count > 0));
}

#[test]
fn empty_input_produces_empty_snapshot() {
    let snapshot = build_incident_replay_snapshot(&[], &[], ReplayControls::new(0, 1), 8);
    assert_eq!(
        snapshot,
        IncidentReplaySnapshot {
            controls: Some(ReplayControls::new(0, 1)),
            timeline: reconstruct_timeline(&[], 8),
            hotspots: Vec::new(),
            visible_events: Vec::new(),
            visible_annotations: Vec::new(),
            total_events: 0,
            dropped_duplicate_events: 0,
        }
    );
}
