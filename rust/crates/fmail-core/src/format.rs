use chrono::{DateTime, Duration, Utc};

const ACTIVE_WINDOW: Duration = Duration::minutes(1);

pub fn is_active(now: DateTime<Utc>, t: DateTime<Utc>) -> bool {
    now.signed_duration_since(t) <= ACTIVE_WINDOW
}

pub fn format_last_seen(now: DateTime<Utc>, mut t: DateTime<Utc>) -> String {
    if is_zero_time(t) {
        return "-".to_string();
    }
    if is_active(now, t) {
        return "active".to_string();
    }
    if t > now {
        t = now;
    }
    format_relative(now, t)
}

fn format_relative(now: DateTime<Utc>, t: DateTime<Utc>) -> String {
    if is_zero_time(t) {
        return "-".to_string();
    }
    let diff = now.signed_duration_since(t);
    if diff < Duration::minutes(1) {
        return "just now".to_string();
    }
    if diff < Duration::hours(1) {
        return format!("{}m ago", diff.num_minutes());
    }
    if diff < Duration::hours(24) {
        return format!("{}h ago", diff.num_hours());
    }
    format!("{}d ago", diff.num_hours() / 24)
}

fn is_zero_time(t: DateTime<Utc>) -> bool {
    // Go time.Time zero value serializes to year 1. Treat that as "unset".
    t.year() == 1
        && t.month() == 1
        && t.day() == 1
        && t.hour() == 0
        && t.minute() == 0
        && t.second() == 0
}

use chrono::Datelike;
use chrono::Timelike;
