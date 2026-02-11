use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::runtime_limits::{
    set_loop_iteration_count, set_loop_started_at, RuntimeMetaValue, RuntimeMetadata,
};

pub const LOOP_PID_KEY: &str = "pid";
pub const LOOP_STOP_CONFIG_KEY: &str = "stop_config";
pub const LOOP_STOP_STATE_KEY: &str = "stop_state";

pub fn attach_loop_pid(metadata: &mut RuntimeMetadata, pid: i32, started_at: DateTime<Utc>) {
    metadata.insert(
        LOOP_PID_KEY.to_string(),
        RuntimeMetaValue::Int(i64::from(pid)),
    );
    set_loop_started_at(metadata, started_at);
    set_loop_iteration_count(metadata, 0);
    reset_stop_state(metadata);
}

pub fn reset_stop_state(metadata: &mut RuntimeMetadata) {
    metadata.insert(
        LOOP_STOP_STATE_KEY.to_string(),
        RuntimeMetaValue::Object(BTreeMap::new()),
    );
}

pub fn loop_pid(metadata: Option<&RuntimeMetadata>) -> Option<i32> {
    let metadata = metadata?;
    let value = metadata.get(LOOP_PID_KEY)?;
    match value {
        RuntimeMetaValue::Int(value) => Some(*value as i32),
        RuntimeMetaValue::Float(value) => Some(*value as i32),
        RuntimeMetaValue::Text(value) => value.parse::<i32>().ok(),
        RuntimeMetaValue::Timestamp(_) | RuntimeMetaValue::Object(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        attach_loop_pid, loop_pid, reset_stop_state, LOOP_PID_KEY, LOOP_STOP_CONFIG_KEY,
        LOOP_STOP_STATE_KEY,
    };
    use crate::runtime_limits::{RuntimeMetaValue, RuntimeMetadata};
    use chrono::{DateTime, Utc};
    use std::collections::BTreeMap;

    #[test]
    fn attach_loop_pid_sets_runtime_fields_and_resets_stop_state() {
        let mut metadata = RuntimeMetadata::new();
        metadata.insert(
            LOOP_STOP_CONFIG_KEY.to_string(),
            RuntimeMetaValue::Text("{\"qual\":{\"every_n\":1}}".to_string()),
        );
        metadata.insert(
            LOOP_STOP_STATE_KEY.to_string(),
            RuntimeMetaValue::Text("{\"main_iteration_count\":8}".to_string()),
        );
        metadata.insert("iteration_count".to_string(), RuntimeMetaValue::Int(42));

        let started_at = parse_time("2026-02-09T18:00:00Z");
        attach_loop_pid(&mut metadata, 12345, started_at);

        assert_eq!(
            metadata.get(LOOP_PID_KEY),
            Some(&RuntimeMetaValue::Int(12345))
        );
        assert_eq!(
            metadata.get("started_at"),
            Some(&RuntimeMetaValue::Text(
                "2026-02-09T18:00:00+00:00".to_string(),
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
                "{\"qual\":{\"every_n\":1}}".to_string(),
            ))
        );
    }

    #[test]
    fn reset_stop_state_overwrites_existing_value() {
        let mut metadata = RuntimeMetadata::new();
        metadata.insert(
            LOOP_STOP_STATE_KEY.to_string(),
            RuntimeMetaValue::Text("stale".to_string()),
        );

        reset_stop_state(&mut metadata);

        assert_eq!(
            metadata.get(LOOP_STOP_STATE_KEY),
            Some(&RuntimeMetaValue::Object(BTreeMap::new()))
        );
    }

    #[test]
    fn loop_pid_parses_int_float_and_string() {
        let mut metadata = RuntimeMetadata::new();
        metadata.insert(LOOP_PID_KEY.to_string(), RuntimeMetaValue::Int(10));
        assert_eq!(loop_pid(Some(&metadata)), Some(10));

        metadata.insert(LOOP_PID_KEY.to_string(), RuntimeMetaValue::Float(11.9));
        assert_eq!(loop_pid(Some(&metadata)), Some(11));

        metadata.insert(
            LOOP_PID_KEY.to_string(),
            RuntimeMetaValue::Text("12".to_string()),
        );
        assert_eq!(loop_pid(Some(&metadata)), Some(12));
    }

    #[test]
    fn loop_pid_returns_none_for_invalid_or_missing_values() {
        let mut metadata = RuntimeMetadata::new();
        assert_eq!(loop_pid(Some(&metadata)), None);

        metadata.insert(
            LOOP_PID_KEY.to_string(),
            RuntimeMetaValue::Text("invalid".to_string()),
        );
        assert_eq!(loop_pid(Some(&metadata)), None);

        metadata.insert(
            LOOP_PID_KEY.to_string(),
            RuntimeMetaValue::Object(BTreeMap::new()),
        );
        assert_eq!(loop_pid(Some(&metadata)), None);
        assert_eq!(loop_pid(None), None);
    }

    fn parse_time(value: &str) -> DateTime<Utc> {
        match DateTime::parse_from_rfc3339(value) {
            Ok(timestamp) => timestamp.with_timezone(&Utc),
            Err(err) => panic!("invalid timestamp {value}: {err}"),
        }
    }
}
