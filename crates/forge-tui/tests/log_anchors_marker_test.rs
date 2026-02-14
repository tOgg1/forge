use forge_tui::log_anchors::{
    add_log_anchor, export_anchor_bundle_json, import_anchor_bundle_json, resolve_anchor_target,
    LogAnchorDraft, LogAnchorFilter, LogAnchorStore,
};

fn draft(loop_id: &str, source: &str, line: usize, marker: &str) -> LogAnchorDraft {
    LogAnchorDraft {
        marker: marker.to_owned(),
        loop_id: loop_id.to_owned(),
        log_source: source.to_owned(),
        line_index: line,
        timestamp: "2026-02-12T12:00:00Z".to_owned(),
        excerpt: "tool failed".to_owned(),
        annotation: String::new(),
        tags: vec!["handoff".to_owned()],
        created_by: "agent-a".to_owned(),
        created_at: "2026-02-12T12:00:00Z".to_owned(),
    }
}

#[test]
fn marker_lookup_and_suffixing_work() {
    let mut store = LogAnchorStore::default();
    let first = match add_log_anchor(&mut store, draft("loop-1", "live", 42, "hotspot")) {
        Ok(anchor) => anchor,
        Err(err) => panic!("first anchor should insert: {err}"),
    };
    let second = match add_log_anchor(&mut store, draft("loop-1", "live", 43, "hotspot")) {
        Ok(anchor) => anchor,
        Err(err) => panic!("second anchor should insert: {err}"),
    };

    assert_ne!(first, second);
    assert_eq!(
        resolve_anchor_target(&store, "hotspot"),
        Some(("live".to_owned(), 42))
    );
    assert_eq!(
        resolve_anchor_target(&store, "hotspot-2"),
        Some(("live".to_owned(), 43))
    );
}

#[test]
fn marker_survives_export_import_round_trip() {
    let mut source = LogAnchorStore::default();
    if let Err(err) = add_log_anchor(&mut source, draft("loop-7", "live", 88, "root-cause")) {
        panic!("add source anchor should succeed: {err}");
    }
    let json = export_anchor_bundle_json(&source, &LogAnchorFilter::default());

    let mut target = LogAnchorStore::default();
    let outcome = import_anchor_bundle_json(&mut target, &json);
    assert_eq!(outcome.imported, 1);
    assert!(target.get_by_marker("root-cause").is_some());
}
