use std::time::Duration;

const DEFAULT_MAX_RETRIES: i32 = 3;
const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_secs(5);
const DEFAULT_COOLDOWN_DURATION: Duration = Duration::from_secs(5 * 60);

/// retry_backoff matches Go `Scheduler.retryBackoff`.
///
/// backoff = base * 2^(attempt-1), attempt<=1 => base.
pub fn retry_backoff(attempt: i32, base: Duration) -> Duration {
    let base = if base.is_zero() {
        DEFAULT_RETRY_BACKOFF
    } else {
        base
    };
    if attempt <= 1 {
        return base;
    }
    let mut backoff = base;
    for _ in 1..attempt {
        backoff = backoff.saturating_mul(2);
    }
    backoff
}

/// retry_after_from_evidence matches Go `retryAfterFromEvidence`.
///
/// Evidence entries look like `retry_after=45s` or `retry_after=30` (seconds).
/// Returns zero duration when not present or invalid.
pub fn retry_after_from_evidence(evidence: &[String]) -> Duration {
    for entry in evidence {
        let trimmed = entry.trim();
        let raw = match trimmed.strip_prefix("retry_after=") {
            Some(v) => v.trim(),
            None => continue,
        };
        if raw.is_empty() {
            continue;
        }
        if let Ok(nanos) = parse_go_duration_to_nanos(raw) {
            if nanos > 0 {
                return Duration::from_nanos(nanos as u64);
            }
            continue;
        }
        if let Ok(seconds) = raw.parse::<i64>() {
            if seconds > 0 {
                return Duration::from_secs(seconds as u64);
            }
        }
    }
    Duration::ZERO
}

/// rate_limit_pause_duration matches Go `Scheduler.rateLimitPauseDuration`.
pub fn rate_limit_pause_duration(evidence: &[String], configured_default: Duration) -> Duration {
    let from_evidence = retry_after_from_evidence(evidence);
    if from_evidence > Duration::ZERO {
        return from_evidence;
    }
    if configured_default > Duration::ZERO {
        return configured_default;
    }
    DEFAULT_COOLDOWN_DURATION
}

/// handle_dispatch_failure_attempts models the `attempts`/`max_retries` semantics from Go.
///
/// Returns `(next_attempts, should_retry)` where `next_attempts = prev_attempts+1`.
pub fn handle_dispatch_failure_attempts(prev_attempts: i32, max_retries: i32) -> (i32, bool) {
    let attempts = prev_attempts.saturating_add(1);
    let max_retries = if max_retries < 0 {
        0
    } else if max_retries == 0 {
        DEFAULT_MAX_RETRIES
    } else {
        max_retries
    };
    (attempts, attempts <= max_retries)
}

// Go duration parsing subset: ns/us/ms/s/m/h with optional floats and composites.
// Ported from `forge-runner` to avoid cross-crate dependency.
fn parse_go_duration_to_nanos(raw: &str) -> Result<i64, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(format!("time: invalid duration {raw:?}"));
    }
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let mut sign = 1.0;
    if s.starts_with('-') {
        sign = -1.0;
        i = 1;
    } else if s.starts_with('+') {
        i = 1;
    }

    let mut total: f64 = 0.0;
    let mut parsed_any = false;

    while i < bytes.len() {
        parsed_any = true;
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        if i == start {
            return Err(format!("time: invalid duration {raw:?}"));
        }
        let value: f64 = s[start..i]
            .parse()
            .map_err(|_| format!("time: invalid duration {raw:?}"))?;

        let rest = &s[i..];
        let (mult, adv) = if rest.starts_with("ns") {
            (1.0, 2usize)
        } else if rest.starts_with("us") {
            (1_000.0, 2usize)
        } else if rest.starts_with("ms") {
            (1_000_000.0, 2usize)
        } else if rest.starts_with('s') {
            (1_000_000_000.0, 1usize)
        } else if rest.starts_with('m') {
            (60.0 * 1_000_000_000.0, 1usize)
        } else if rest.starts_with('h') {
            (3600.0 * 1_000_000_000.0, 1usize)
        } else {
            return Err(format!("time: invalid duration {raw:?}"));
        };
        i += adv;
        total += value * mult;
    }

    if !parsed_any || !total.is_finite() {
        return Err(format!("time: invalid duration {raw:?}"));
    }

    let nanos = (total * sign).round();
    if nanos > (i64::MAX as f64) {
        return Err(format!("time: invalid duration {raw:?}"));
    }
    Ok(nanos as i64)
}

#[cfg(test)]
mod tests {
    use super::{
        handle_dispatch_failure_attempts, rate_limit_pause_duration, retry_after_from_evidence,
        retry_backoff,
    };
    use std::time::Duration;

    #[test]
    fn retry_backoff_doubles_each_attempt() {
        let base = Duration::from_secs(1);
        assert_eq!(retry_backoff(1, base), base);
        assert_eq!(retry_backoff(2, base), Duration::from_secs(2));
        assert_eq!(retry_backoff(3, base), Duration::from_secs(4));
    }

    #[test]
    fn retry_after_from_evidence_prefers_first_valid_entry() {
        let evidence = vec![
            "nope".to_string(),
            "retry_after=bad".to_string(),
            "retry_after=45s".to_string(),
            "retry_after=10s".to_string(),
        ];
        assert_eq!(
            retry_after_from_evidence(&evidence),
            Duration::from_secs(45)
        );
    }

    #[test]
    fn retry_after_from_evidence_accepts_numeric_seconds() {
        let evidence = vec!["retry_after=30".to_string()];
        assert_eq!(
            retry_after_from_evidence(&evidence),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn rate_limit_pause_duration_prefers_evidence_then_config_then_default() {
        let evidence = vec!["retry_after=12s".to_string()];
        assert_eq!(
            rate_limit_pause_duration(&evidence, Duration::from_secs(99)),
            Duration::from_secs(12)
        );

        let empty: Vec<String> = Vec::new();
        assert_eq!(
            rate_limit_pause_duration(&empty, Duration::from_secs(33)),
            Duration::from_secs(33)
        );
        assert_eq!(
            rate_limit_pause_duration(&empty, Duration::ZERO),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn handle_dispatch_failure_attempts_matches_go_semantics() {
        assert_eq!(handle_dispatch_failure_attempts(0, 3), (1, true));
        assert_eq!(handle_dispatch_failure_attempts(2, 3), (3, true));
        assert_eq!(handle_dispatch_failure_attempts(3, 3), (4, false));

        // max_retries < 0 => treated as 0 (no retries)
        assert_eq!(handle_dispatch_failure_attempts(0, -1), (1, false));

        // max_retries == 0 => default
        assert_eq!(handle_dispatch_failure_attempts(3, 0), (4, false));
    }
}
