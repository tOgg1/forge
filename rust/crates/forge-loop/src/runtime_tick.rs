use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::runtime_limits::{
    loop_iteration_count, loop_limit_reason, loop_started_at, set_loop_iteration_count,
    set_loop_started_at, RuntimeMetadata,
};
use crate::wait_until::{clear_wait_until, set_wait_until};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickOutcome {
    Continue,
    WaitUntil(DateTime<Utc>),
    Stop { reason: String },
}

/// tick_limits_and_wait applies the loop "limits + profile wait" semantics from Go's loop runner.
///
/// - Ensures `started_at` is set when max_runtime > 0 and metadata is missing.
/// - Stops when max_iterations or max_runtime reached.
/// - If `profile_wait_until_epoch` is set, transitions to waiting until that timestamp.
/// - Otherwise clears `wait_until` metadata.
pub fn tick_limits_and_wait(
    metadata: &mut RuntimeMetadata,
    max_iterations: i32,
    max_runtime: Duration,
    now: DateTime<Utc>,
    profile_wait_until_epoch: Option<i64>,
) -> TickOutcome {
    let started_at = loop_started_at(Some(metadata));
    if max_runtime > Duration::ZERO && started_at.is_none() {
        set_loop_started_at(metadata, now);
    }

    let iteration_count = loop_iteration_count(Some(metadata));
    let reason = loop_limit_reason(
        max_iterations,
        iteration_count,
        max_runtime,
        loop_started_at(Some(metadata)),
        now,
    );
    if let Some(reason) = reason {
        return TickOutcome::Stop { reason };
    }

    if let Some(epoch) = profile_wait_until_epoch {
        // If epoch is in the past, do not treat it as a wait request.
        if let Some(until) = DateTime::<Utc>::from_timestamp(epoch, 0) {
            if until > now {
                set_wait_until(metadata, until);
                return TickOutcome::WaitUntil(until);
            }
        }
    }

    clear_wait_until(metadata);
    TickOutcome::Continue
}

pub fn increment_iteration_count(metadata: &mut RuntimeMetadata) -> i32 {
    let current = loop_iteration_count(Some(metadata));
    let next = current.saturating_add(1);
    set_loop_iteration_count(metadata, next);
    next
}

#[cfg(test)]
mod tests {
    use super::{increment_iteration_count, tick_limits_and_wait, TickOutcome};
    use crate::runtime_limits::{RuntimeMetaValue, RuntimeMetadata};
    use crate::wait_until::{wait_until, WAIT_UNTIL_KEY};
    use chrono::{TimeZone, Utc};
    use std::time::Duration;

    #[test]
    fn tick_sets_started_at_when_runtime_limit_enabled() {
        let now = Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap();
        let mut meta = RuntimeMetadata::new();
        let outcome = tick_limits_and_wait(&mut meta, 0, Duration::from_secs(10), now, None);
        assert_eq!(outcome, TickOutcome::Continue);
        assert!(meta.contains_key("started_at"));
    }

    #[test]
    fn tick_stops_when_iteration_limit_reached() {
        let now = Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap();
        let mut meta = RuntimeMetadata::new();
        meta.insert("iteration_count".to_string(), RuntimeMetaValue::Int(2));
        let outcome = tick_limits_and_wait(&mut meta, 2, Duration::ZERO, now, None);
        assert_eq!(
            outcome,
            TickOutcome::Stop {
                reason: "max iterations reached (2)".to_string()
            }
        );
    }

    #[test]
    fn tick_sets_wait_until_when_profile_wait_requested() {
        let now = Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap();
        let until = Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 5).unwrap();
        let mut meta = RuntimeMetadata::new();
        let outcome =
            tick_limits_and_wait(&mut meta, 0, Duration::ZERO, now, Some(until.timestamp()));
        assert_eq!(outcome, TickOutcome::WaitUntil(until));
        assert_eq!(wait_until(Some(&meta)), Some(until));
        assert!(meta.contains_key(WAIT_UNTIL_KEY));
    }

    #[test]
    fn tick_ignores_past_wait_until_and_clears_key() {
        let now = Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap();
        let past = Utc.with_ymd_and_hms(2026, 2, 10, 11, 59, 0).unwrap();
        let mut meta = RuntimeMetadata::new();
        meta.insert(
            WAIT_UNTIL_KEY.to_string(),
            RuntimeMetaValue::Text("2026-02-10T12:00:00Z".to_string()),
        );
        let outcome =
            tick_limits_and_wait(&mut meta, 0, Duration::ZERO, now, Some(past.timestamp()));
        assert_eq!(outcome, TickOutcome::Continue);
        assert!(!meta.contains_key(WAIT_UNTIL_KEY));
    }

    #[test]
    fn increment_iteration_count_saturates_and_persists() {
        let mut meta = RuntimeMetadata::new();
        assert_eq!(increment_iteration_count(&mut meta), 1);
        assert_eq!(increment_iteration_count(&mut meta), 2);
    }
}
