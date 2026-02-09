use chrono::{DateTime, Utc};
use forge_loop::runner_metadata::{
    attach_loop_pid, loop_pid, LOOP_PID_KEY, LOOP_STOP_CONFIG_KEY, LOOP_STOP_STATE_KEY,
};
use forge_loop::runtime_limits::{RuntimeMetaValue, RuntimeMetadata};
use std::collections::BTreeMap;

#[test]
fn runner_metadata_lifecycle_scenario_matches_go_shape() {
    let mut metadata = RuntimeMetadata::new();
    metadata.insert(
        LOOP_STOP_CONFIG_KEY.to_string(),
        RuntimeMetaValue::Text("{\"quant\":{\"cmd\":\"echo ok\"}}".to_string()),
    );
    metadata.insert(
        LOOP_STOP_STATE_KEY.to_string(),
        RuntimeMetaValue::Object({
            let mut state = BTreeMap::new();
            state.insert("main_iteration_count".to_string(), RuntimeMetaValue::Int(9));
            state
        }),
    );
    metadata.insert("iteration_count".to_string(), RuntimeMetaValue::Int(7));
    metadata.insert(
        LOOP_PID_KEY.to_string(),
        RuntimeMetaValue::Text("999".to_string()),
    );

    attach_loop_pid(&mut metadata, 4242, parse_time("2026-02-09T18:05:00Z"));

    assert_eq!(loop_pid(Some(&metadata)), Some(4242));
    assert_eq!(
        metadata.get("started_at"),
        Some(&RuntimeMetaValue::Text(
            "2026-02-09T18:05:00+00:00".to_string(),
        ))
    );
    assert_eq!(
        metadata.get("iteration_count"),
        Some(&RuntimeMetaValue::Int(0))
    );
    assert_eq!(
        metadata.get(LOOP_STOP_STATE_KEY),
        Some(&RuntimeMetaValue::Object(BTreeMap::new()))
    );
    assert_eq!(
        metadata.get(LOOP_STOP_CONFIG_KEY),
        Some(&RuntimeMetaValue::Text(
            "{\"quant\":{\"cmd\":\"echo ok\"}}".to_string(),
        ))
    );
}

fn parse_time(value: &str) -> DateTime<Utc> {
    match DateTime::parse_from_rfc3339(value) {
        Ok(timestamp) => timestamp.with_timezone(&Utc),
        Err(err) => panic!("invalid timestamp {value}: {err}"),
    }
}
