use std::time::Duration;

use chrono::{DateTime, Utc};

use super::types::{MAX_EVENT_LINE_LENGTH, MAX_PENDING_BYTES};

pub fn split_lines(buffer: &[u8]) -> (Vec<String>, Vec<u8>) {
    if buffer.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (idx, &b) in buffer.iter().enumerate() {
        if b == b'\n' {
            let part = &buffer[start..idx];
            let line = String::from_utf8_lossy(part)
                .trim_end_matches('\r')
                .to_string();
            lines.push(line);
            start = idx + 1;
        }
    }
    if start < buffer.len() {
        return (lines, buffer[start..].to_vec());
    }
    (lines, Vec::new())
}

pub fn contains_non_whitespace(data: &[u8]) -> bool {
    data.iter()
        .any(|b| !matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
}

pub fn truncate_text(value: &str, max: usize) -> (String, bool) {
    if max == 0 || value.len() <= max {
        return (value.to_string(), false);
    }
    (value[..max].to_string(), true)
}

pub fn truncate_lines(lines: &[String], max: usize) -> Vec<String> {
    lines
        .iter()
        .map(|line| truncate_text(line, max).0)
        .collect()
}

pub fn cap_pending_bytes(mut pending: Vec<u8>) -> Vec<u8> {
    if pending.len() <= MAX_PENDING_BYTES {
        return pending;
    }
    let drain = pending.len() - MAX_PENDING_BYTES;
    pending.drain(0..drain);
    pending
}

pub fn parse_go_duration_to_nanos(raw: &str) -> Result<i64, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(format!("time: invalid duration {:?}", raw));
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
            return Err(format!("time: invalid duration {:?}", raw));
        }
        let value: f64 = s[start..i]
            .parse()
            .map_err(|_| format!("time: invalid duration {:?}", raw))?;

        let rest = &s[i..];
        let (mult, adv) = if rest.starts_with("ns") {
            (1.0, 2usize)
        } else if rest.starts_with("us") {
            (1_000.0, 2usize)
        } else if rest.starts_with("µs") {
            (1_000.0, "µs".len())
        } else if rest.starts_with("ms") {
            (1_000_000.0, 2usize)
        } else if rest.starts_with('s') {
            (1_000_000_000.0, 1usize)
        } else if rest.starts_with('m') {
            (60.0 * 1_000_000_000.0, 1usize)
        } else if rest.starts_with('h') {
            (3600.0 * 1_000_000_000.0, 1usize)
        } else {
            return Err(format!("time: invalid duration {:?}", raw));
        };
        i += adv;
        total += value * mult;
    }

    if !parsed_any || !total.is_finite() {
        return Err(format!("time: invalid duration {:?}", raw));
    }

    let nanos = (total * sign).round();
    if nanos > (i64::MAX as f64) {
        return Err(format!("time: invalid duration {:?}", raw));
    }
    Ok(nanos as i64)
}

pub fn parse_positive_duration(value: &str) -> Result<Duration, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("duration is required".to_string());
    }
    let nanos = parse_go_duration_to_nanos(trimmed)?;
    if nanos <= 0 {
        return Err("duration must be positive".to_string());
    }
    Ok(Duration::from_nanos(nanos as u64))
}

pub fn parse_cooldown(
    until: &str,
    duration: &str,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, String> {
    let until_trimmed = until.trim();
    if !until_trimmed.is_empty() {
        if let Ok(ts) = DateTime::parse_from_rfc3339(until_trimmed) {
            return Ok(ts.with_timezone(&Utc));
        }
    }
    let duration_trimmed = duration.trim();
    if !duration_trimmed.is_empty() {
        let dur = parse_positive_duration(duration_trimmed)?;
        return Ok(now + chrono::Duration::from_std(dur).unwrap_or_default());
    }
    Err("cooldown requires until or duration".to_string())
}

pub fn format_idle_for(dur: Duration) -> String {
    // Best-effort; Go uses time.Duration.String().
    format!("{:.0?}", dur)
}

#[allow(dead_code)]
pub fn preview_line(line: &str) -> (String, Option<bool>) {
    let (preview, truncated) = truncate_text(line, MAX_EVENT_LINE_LENGTH);
    (preview, if truncated { Some(true) } else { None })
}

#[cfg(test)]
mod tests {
    use super::{parse_go_duration_to_nanos, parse_positive_duration, split_lines};

    #[test]
    fn split_lines_returns_remainder_when_no_trailing_newline() {
        let (lines, rem) = split_lines(b"a\nb");
        assert_eq!(lines, vec!["a".to_string()]);
        assert_eq!(rem, b"b".to_vec());
    }

    #[test]
    fn parse_positive_duration_rejects_zero() {
        assert!(parse_positive_duration("0s").is_err());
    }

    #[test]
    fn parse_go_duration_basic() {
        assert_eq!(parse_go_duration_to_nanos("1s"), Ok(1_000_000_000));
        assert_eq!(parse_go_duration_to_nanos("1m"), Ok(60_000_000_000));
        assert!(parse_go_duration_to_nanos("bad").is_err());
    }
}
