//! Go-compatible-ish duration parsing helpers used by fmail flags.
#![allow(dead_code)]

use std::time::Duration;

/// Parse a Go-style duration string into signed seconds.
///
/// Supports:
/// - Optional leading sign (`+`/`-`)
/// - Component chains (e.g. `1h30m`, `2m5.5s`, `100ms`)
/// - Units: `ns`, `us`, `µs`, `μs`, `ms`, `s`, `m`, `h`
/// - Bare zero (`0`)
pub(crate) fn parse_go_duration_seconds(raw: &str) -> Result<f64, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty duration".to_string());
    }

    let (negative, mut rest) = if let Some(value) = trimmed.strip_prefix('-') {
        (true, value)
    } else if let Some(value) = trimmed.strip_prefix('+') {
        (false, value)
    } else {
        (false, trimmed)
    };

    if rest.is_empty() {
        return Err("empty duration".to_string());
    }

    if rest == "0" {
        return Ok(0.0);
    }

    let mut total_seconds = 0.0f64;
    while !rest.is_empty() {
        let num_len = number_prefix_len(rest);
        if num_len == 0 {
            return Err("invalid duration value".to_string());
        }

        let number_raw = &rest[..num_len];
        let value = number_raw
            .parse::<f64>()
            .map_err(|_| "invalid duration value".to_string())?;
        if !value.is_finite() {
            return Err("invalid duration value".to_string());
        }

        rest = &rest[num_len..];
        let (unit, scale_seconds) = duration_unit(rest)?;
        total_seconds += value * scale_seconds;
        rest = &rest[unit.len()..];
    }

    if negative {
        total_seconds = -total_seconds;
    }
    Ok(total_seconds)
}

/// Parse a non-negative Go-style duration into `std::time::Duration`.
pub(crate) fn parse_go_duration(raw: &str) -> Result<Duration, String> {
    let seconds = parse_go_duration_seconds(raw)?;
    if seconds < 0.0 {
        return Err("duration must be non-negative".to_string());
    }
    Duration::try_from_secs_f64(seconds).map_err(|_| "duration out of range".to_string())
}

fn number_prefix_len(input: &str) -> usize {
    let mut bytes = 0usize;
    let mut saw_digit = false;
    let mut saw_dot = false;

    for ch in input.chars() {
        if ch.is_ascii_digit() {
            saw_digit = true;
            bytes += ch.len_utf8();
            continue;
        }
        if ch == '.' && !saw_dot {
            saw_dot = true;
            bytes += ch.len_utf8();
            continue;
        }
        break;
    }

    if saw_digit {
        bytes
    } else {
        0
    }
}

fn duration_unit(input: &str) -> Result<(&'static str, f64), String> {
    if input.starts_with("ns") {
        return Ok(("ns", 1e-9));
    }
    if input.starts_with("us") {
        return Ok(("us", 1e-6));
    }
    if input.starts_with("µs") {
        return Ok(("µs", 1e-6));
    }
    if input.starts_with("μs") {
        return Ok(("μs", 1e-6));
    }
    if input.starts_with("ms") {
        return Ok(("ms", 1e-3));
    }
    if input.starts_with('s') {
        return Ok(("s", 1.0));
    }
    if input.starts_with('m') {
        return Ok(("m", 60.0));
    }
    if input.starts_with('h') {
        return Ok(("h", 3600.0));
    }
    Err("missing duration unit".to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_component_chain() {
        let value = parse_go_duration_seconds("1h30m5.5s").unwrap();
        assert!((value - 5405.5).abs() < 0.000_001);
    }

    #[test]
    fn parses_small_units() {
        let micros = parse_go_duration_seconds("250us").unwrap();
        assert!((micros - 0.00025).abs() < 0.000_000_001);

        let micros_mu = parse_go_duration_seconds("250µs").unwrap();
        assert!((micros_mu - 0.00025).abs() < 0.000_000_001);

        let micros_greek = parse_go_duration_seconds("250μs").unwrap();
        assert!((micros_greek - 0.00025).abs() < 0.000_000_001);
    }

    #[test]
    fn supports_signed_values() {
        let neg = parse_go_duration_seconds("-1.5h").unwrap();
        assert!((neg + 5400.0).abs() < 0.000_001);
        assert_eq!(parse_go_duration_seconds("0").unwrap(), 0.0);
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(parse_go_duration_seconds("").is_err());
        assert!(parse_go_duration_seconds("2").is_err());
        assert!(parse_go_duration_seconds("abc").is_err());
        assert!(parse_go_duration_seconds("1d").is_err());
    }

    #[test]
    fn parse_go_duration_rejects_negative() {
        let err = parse_go_duration("-1s").unwrap_err();
        assert!(err.contains("non-negative"), "{err}");
    }
}
