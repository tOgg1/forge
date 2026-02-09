use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeMetaValue {
    Int(i64),
    Float(f64),
    Text(String),
    Timestamp(DateTime<Utc>),
}

pub type RuntimeMetadata = BTreeMap<String, RuntimeMetaValue>;

pub fn loop_iteration_count(metadata: Option<&RuntimeMetadata>) -> i32 {
    let Some(metadata) = metadata else {
        return 0;
    };
    let Some(value) = metadata.get("iteration_count") else {
        return 0;
    };
    match value {
        RuntimeMetaValue::Int(v) => *v as i32,
        RuntimeMetaValue::Float(v) => *v as i32,
        RuntimeMetaValue::Text(v) => v.parse::<i32>().unwrap_or(0),
        RuntimeMetaValue::Timestamp(_) => 0,
    }
}

pub fn set_loop_iteration_count(metadata: &mut RuntimeMetadata, count: i32) {
    metadata.insert(
        "iteration_count".to_string(),
        RuntimeMetaValue::Int(count as i64),
    );
}

pub fn loop_started_at(metadata: Option<&RuntimeMetadata>) -> Option<DateTime<Utc>> {
    let metadata = metadata?;
    let value = metadata.get("started_at")?;
    match value {
        RuntimeMetaValue::Timestamp(value) => Some(*value),
        RuntimeMetaValue::Text(value) => DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|dt| dt.with_timezone(&Utc)),
        RuntimeMetaValue::Int(_) | RuntimeMetaValue::Float(_) => None,
    }
}

pub fn set_loop_started_at(metadata: &mut RuntimeMetadata, started_at: DateTime<Utc>) {
    metadata.insert(
        "started_at".to_string(),
        RuntimeMetaValue::Text(started_at.to_rfc3339()),
    );
}

pub fn loop_limit_reason(
    max_iterations: i32,
    iteration_count: i32,
    max_runtime: Duration,
    started_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<String> {
    if max_iterations > 0 && iteration_count >= max_iterations {
        return Some(format!("max iterations reached ({max_iterations})"));
    }
    if max_runtime > Duration::ZERO {
        if let Some(started_at) = started_at {
            let max_runtime_chrono = match chrono::Duration::from_std(max_runtime) {
                Ok(value) => value,
                Err(_) => return None,
            };
            if now.signed_duration_since(started_at) >= max_runtime_chrono {
                return Some(format!(
                    "max runtime reached ({})",
                    format_duration_go_like(max_runtime)
                ));
            }
        }
    }
    None
}

fn format_duration_go_like(duration: Duration) -> String {
    let secs = duration.as_secs();
    let nanos = duration.subsec_nanos();
    if secs >= 60 && nanos == 0 {
        let mins = secs / 60;
        let rem = secs % 60;
        return format!("{mins}m{rem}s");
    }
    if secs > 0 && nanos == 0 {
        return format!("{secs}s");
    }
    if secs == 0 && nanos > 0 && nanos % 1_000_000 == 0 {
        return format!("{}ms", nanos / 1_000_000);
    }
    format!("{secs}s")
}

#[cfg(test)]
mod tests {
    use super::{
        loop_iteration_count, loop_limit_reason, loop_started_at, set_loop_iteration_count,
        set_loop_started_at, RuntimeMetaValue, RuntimeMetadata,
    };
    use chrono::{DateTime, Duration as ChronoDuration, Utc};
    use std::time::Duration;

    #[test]
    fn iteration_count_parses_int_float_string() {
        let mut metadata = RuntimeMetadata::new();
        metadata.insert("iteration_count".to_string(), RuntimeMetaValue::Int(7));
        assert_eq!(loop_iteration_count(Some(&metadata)), 7);
        metadata.insert("iteration_count".to_string(), RuntimeMetaValue::Float(9.8));
        assert_eq!(loop_iteration_count(Some(&metadata)), 9);
        metadata.insert(
            "iteration_count".to_string(),
            RuntimeMetaValue::Text("11".to_string()),
        );
        assert_eq!(loop_iteration_count(Some(&metadata)), 11);
        metadata.insert(
            "iteration_count".to_string(),
            RuntimeMetaValue::Text("x".to_string()),
        );
        assert_eq!(loop_iteration_count(Some(&metadata)), 0);
    }

    #[test]
    fn set_loop_iteration_count_writes_int_value() {
        let mut metadata = RuntimeMetadata::new();
        set_loop_iteration_count(&mut metadata, 3);
        assert_eq!(
            metadata.get("iteration_count"),
            Some(&RuntimeMetaValue::Int(3))
        );
    }

    #[test]
    fn started_at_parses_timestamp_and_rfc3339_text() {
        let now = now_utc("2026-02-09T17:00:00Z");
        let mut metadata = RuntimeMetadata::new();
        metadata.insert("started_at".to_string(), RuntimeMetaValue::Timestamp(now));
        assert_eq!(loop_started_at(Some(&metadata)), Some(now));

        metadata.insert(
            "started_at".to_string(),
            RuntimeMetaValue::Text("2026-02-09T17:01:00Z".to_string()),
        );
        assert_eq!(
            loop_started_at(Some(&metadata)),
            Some(now_utc("2026-02-09T17:01:00Z"))
        );
    }

    #[test]
    fn set_loop_started_at_writes_rfc3339_text() {
        let mut metadata = RuntimeMetadata::new();
        let started_at = now_utc("2026-02-09T17:00:00Z");
        set_loop_started_at(&mut metadata, started_at);
        assert_eq!(
            metadata.get("started_at"),
            Some(&RuntimeMetaValue::Text(
                "2026-02-09T17:00:00+00:00".to_string()
            ))
        );
    }

    #[test]
    fn loop_limit_reason_prefers_iteration_limit() {
        let now = now_utc("2026-02-09T17:00:00Z");
        let reason = loop_limit_reason(2, 2, Duration::from_secs(10), Some(now), now);
        assert_eq!(reason.as_deref(), Some("max iterations reached (2)"));
    }

    #[test]
    fn loop_limit_reason_runtime_when_elapsed_is_ge_limit() {
        let start = now_utc("2026-02-09T17:00:00Z");
        let now = start + ChronoDuration::seconds(5);
        let reason = loop_limit_reason(0, 0, Duration::from_secs(5), Some(start), now);
        assert_eq!(reason.as_deref(), Some("max runtime reached (5s)"));
    }

    #[test]
    fn loop_limit_reason_none_when_not_reached_or_missing_started_at() {
        let start = now_utc("2026-02-09T17:00:00Z");
        let now = start + ChronoDuration::seconds(4);
        assert_eq!(
            loop_limit_reason(3, 2, Duration::from_secs(5), Some(start), now),
            None
        );
        assert_eq!(
            loop_limit_reason(0, 0, Duration::from_secs(5), None, now),
            None
        );
    }

    fn now_utc(value: &str) -> DateTime<Utc> {
        match DateTime::parse_from_rfc3339(value) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(err) => panic!("invalid test timestamp {value}: {err}"),
        }
    }
}
