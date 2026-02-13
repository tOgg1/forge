use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tabwriter::TabWriter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuditEvent {
    pub id: String,
    pub timestamp: String,
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: String,
    pub payload: String,
    pub metadata: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventQuery {
    pub event_type: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub cursor: String,
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventPage {
    pub events: Vec<AuditEvent>,
    pub next_cursor: String,
}

pub trait AuditBackend {
    fn query_events(&self, query: &EventQuery) -> Result<EventPage, String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryAuditBackend {
    events: Vec<AuditEvent>,
}

impl InMemoryAuditBackend {
    pub fn with_events(events: Vec<AuditEvent>) -> Self {
        Self { events }
    }
}

impl AuditBackend for InMemoryAuditBackend {
    fn query_events(&self, query: &EventQuery) -> Result<EventPage, String> {
        let limit = if query.limit <= 0 {
            100
        } else {
            query.limit as usize
        };

        let mut filtered: Vec<AuditEvent> = self
            .events
            .iter()
            .filter(|event| {
                if let Some(t) = &query.event_type {
                    if event.event_type != *t {
                        return false;
                    }
                }
                if let Some(entity_type) = &query.entity_type {
                    if event.entity_type != *entity_type {
                        return false;
                    }
                }
                if let Some(entity_id) = &query.entity_id {
                    if event.entity_id != *entity_id {
                        return false;
                    }
                }

                let event_ts = parse_since(&event.timestamp)
                    .ok()
                    .flatten()
                    .map(|parsed| parsed.epoch_seconds)
                    .unwrap_or_default();

                if let Some(since) = &query.since {
                    let since_ts = parse_since(since)
                        .ok()
                        .flatten()
                        .map(|parsed| parsed.epoch_seconds)
                        .unwrap_or(i64::MIN);
                    if event_ts < since_ts {
                        return false;
                    }
                }

                if let Some(until) = &query.until {
                    let until_ts = parse_since(until)
                        .ok()
                        .flatten()
                        .map(|parsed| parsed.epoch_seconds)
                        .unwrap_or(i64::MAX);
                    if event_ts >= until_ts {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        filtered.sort_by(|left, right| {
            let left_ts = parse_since(&left.timestamp)
                .ok()
                .flatten()
                .map(|parsed| parsed.epoch_seconds)
                .unwrap_or_default();
            let right_ts = parse_since(&right.timestamp)
                .ok()
                .flatten()
                .map(|parsed| parsed.epoch_seconds)
                .unwrap_or_default();
            left_ts.cmp(&right_ts).then_with(|| left.id.cmp(&right.id))
        });

        if !query.cursor.trim().is_empty() {
            let Some(cursor_index) = filtered.iter().position(|event| event.id == query.cursor)
            else {
                return Ok(EventPage::default());
            };
            filtered = filtered.into_iter().skip(cursor_index + 1).collect();
        }

        if filtered.len() > limit {
            let next_cursor = filtered[limit - 1].id.clone();
            filtered.truncate(limit);
            return Ok(EventPage {
                events: filtered,
                next_cursor,
            });
        }

        Ok(EventPage {
            events: filtered,
            next_cursor: String::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SqliteAuditBackend {
    db_path: PathBuf,
}

impl SqliteAuditBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
        }
    }

    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl AuditBackend for SqliteAuditBackend {
    fn query_events(&self, query: &EventQuery) -> Result<EventPage, String> {
        if !self.db_path.exists() {
            return Ok(EventPage::default());
        }

        let db = forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))?;
        let event_repo = forge_db::event_repository::EventRepository::new(&db);

        let db_query = forge_db::event_repository::EventQuery {
            event_type: query.event_type.clone(),
            entity_type: query.entity_type.clone(),
            entity_id: query.entity_id.clone(),
            since: query.since.clone(),
            until: query.until.clone(),
            cursor: query.cursor.clone(),
            limit: query.limit,
        };

        let page = match event_repo.query(db_query) {
            Ok(page) => page,
            Err(err) if err.to_string().contains("no such table: events") => {
                return Ok(EventPage::default());
            }
            Err(err) => return Err(err.to_string()),
        };

        let events = page
            .events
            .into_iter()
            .map(|event| AuditEvent {
                id: event.id,
                timestamp: event.timestamp,
                event_type: event.event_type,
                entity_type: event.entity_type,
                entity_id: event.entity_id,
                payload: event.payload,
                metadata: event
                    .metadata
                    .map(|metadata| metadata.into_iter().collect::<BTreeMap<String, String>>()),
            })
            .collect();

        Ok(EventPage {
            events,
            next_cursor: page.next_cursor,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    event_types_raw: String,
    action_types_raw: String,
    entity_type: String,
    entity_id: String,
    since: String,
    until: String,
    cursor: String,
    limit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedTime {
    canonical: String,
    epoch_seconds: i64,
}

#[derive(Debug, Serialize)]
struct JsonAuditEvent<'a> {
    id: &'a str,
    timestamp: &'a str,
    #[serde(rename = "type")]
    event_type: &'a str,
    entity_type: &'a str,
    entity_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<&'a BTreeMap<String, String>>,
}

pub fn run_for_test(args: &[&str], backend: &dyn AuditBackend) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_with_backend(&owned_args, backend, &mut stdout, &mut stderr);
    CommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

pub fn run_with_backend(
    args: &[String],
    backend: &dyn AuditBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

fn execute(
    args: &[String],
    backend: &dyn AuditBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    if !parsed.event_types_raw.trim().is_empty() && !parsed.action_types_raw.trim().is_empty() {
        return Err("use either --type or --action, not both".to_string());
    }

    let mut raw_types = parsed.event_types_raw.trim().to_string();
    if raw_types.is_empty() {
        raw_types = parsed.action_types_raw.trim().to_string();
    }

    let event_types = parse_event_types(&raw_types)?;

    let since =
        parse_since(&parsed.since).map_err(|err| format!("invalid --since value: {err}"))?;
    let until =
        parse_since(&parsed.until).map_err(|err| format!("invalid --until value: {err}"))?;

    if let (Some(start), Some(end)) = (&since, &until) {
        if start.epoch_seconds > end.epoch_seconds {
            return Err("--since must be before --until".to_string());
        }
    }

    let mut query = EventQuery {
        cursor: parsed.cursor.clone(),
        limit: if parsed.limit <= 0 { 100 } else { parsed.limit },
        ..EventQuery::default()
    };

    if !parsed.entity_type.trim().is_empty() {
        query.entity_type = Some(parsed.entity_type.trim().to_string());
    }
    if !parsed.entity_id.trim().is_empty() {
        query.entity_id = Some(parsed.entity_id.trim().to_string());
    }
    if let Some(start) = since {
        query.since = Some(start.canonical);
    }
    if let Some(end) = until {
        query.until = Some(end.canonical);
    }
    if event_types.len() == 1 {
        query.event_type = Some(event_types[0].clone());
    }

    let page = backend.query_events(&query)?;
    let events = filter_events_by_type(page.events, &event_types);

    if parsed.json || parsed.jsonl {
        if parsed.jsonl {
            for event in &events {
                serde_json::to_writer(&mut *stdout, &to_json_event(event))
                    .map_err(|err| err.to_string())?;
                writeln!(stdout).map_err(|err| err.to_string())?;
            }
        } else {
            let payload: Vec<JsonAuditEvent<'_>> = events.iter().map(to_json_event).collect();
            serde_json::to_writer_pretty(&mut *stdout, &payload).map_err(|err| err.to_string())?;
            writeln!(stdout).map_err(|err| err.to_string())?;
        }
        return Ok(());
    }

    let mut tw = TabWriter::new(&mut *stdout).padding(2);
    writeln!(tw, "TIME\tTYPE\tENTITY\tID").map_err(|err| err.to_string())?;
    for event in &events {
        writeln!(
            tw,
            "{}\t{}\t{}\t{}",
            format_table_time(&event.timestamp),
            event.event_type,
            event.entity_type,
            event.entity_id,
        )
        .map_err(|err| err.to_string())?;
    }
    tw.flush().map_err(|err| err.to_string())?;

    if !page.next_cursor.is_empty() {
        writeln!(stdout).map_err(|err| err.to_string())?;
        writeln!(stdout, "Next cursor: {}", page.next_cursor).map_err(|err| err.to_string())?;
    }

    if events.is_empty() {
        writeln!(stdout, "No events matched the current filters.")
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn to_json_event(event: &AuditEvent) -> JsonAuditEvent<'_> {
    JsonAuditEvent {
        id: &event.id,
        timestamp: &event.timestamp,
        event_type: &event.event_type,
        entity_type: &event.entity_type,
        entity_id: &event.entity_id,
        payload: parse_payload(&event.payload),
        metadata: event.metadata.as_ref(),
    }
}

fn parse_payload(raw: &str) -> Option<serde_json::Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => Some(value),
        Err(_) => Some(serde_json::Value::String(trimmed.to_string())),
    }
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "audit") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut event_types_raw = String::new();
    let mut action_types_raw = String::new();
    let mut entity_type = String::new();
    let mut entity_id = String::new();
    let mut since = String::new();
    let mut until = String::new();
    let mut cursor = String::new();
    let mut limit = 100_i64;
    let mut positionals = Vec::new();

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => return Err(HELP_TEXT.to_string()),
            "--json" => {
                json = true;
                index += 1;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
            }
            "--type" => {
                event_types_raw = take_value(args, index, "--type")?;
                index += 2;
            }
            "--action" => {
                action_types_raw = take_value(args, index, "--action")?;
                index += 2;
            }
            "--entity-type" => {
                entity_type = take_value(args, index, "--entity-type")?;
                index += 2;
            }
            "--entity-id" => {
                entity_id = take_value(args, index, "--entity-id")?;
                index += 2;
            }
            "--since" => {
                since = take_value(args, index, "--since")?;
                index += 2;
            }
            "--until" => {
                until = take_value(args, index, "--until")?;
                index += 2;
            }
            "--cursor" => {
                cursor = take_value(args, index, "--cursor")?;
                index += 2;
            }
            "--limit" => {
                let value = take_value(args, index, "--limit")?;
                limit = value
                    .parse::<i64>()
                    .map_err(|_| format!("error: invalid value '{}' for --limit", value))?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for audit: '{flag}'"));
            }
            value => {
                positionals.push(value.to_string());
                index += 1;
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl are mutually exclusive".to_string());
    }

    if !positionals.is_empty() {
        return Err("error: audit does not accept positional arguments".to_string());
    }

    Ok(ParsedArgs {
        json,
        jsonl,
        event_types_raw,
        action_types_raw,
        entity_type,
        entity_id,
        since,
        until,
        cursor,
        limit,
    })
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

fn parse_event_types(raw: &str) -> Result<Vec<String>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut types = Vec::new();
    for part in trimmed.split(',') {
        let event_type = part.trim();
        if !event_type.is_empty() {
            types.push(event_type.to_string());
        }
    }

    if types.is_empty() {
        return Err("event type filter cannot be empty".to_string());
    }

    Ok(types)
}

fn filter_events_by_type(events: Vec<AuditEvent>, event_types: &[String]) -> Vec<AuditEvent> {
    if event_types.len() <= 1 {
        return events;
    }

    let allowed: BTreeSet<&str> = event_types.iter().map(String::as_str).collect();
    events
        .into_iter()
        .filter(|event| allowed.contains(event.event_type.as_str()))
        .collect()
}

fn parse_since(raw: &str) -> Result<Option<ParsedTime>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let now_epoch = now_epoch_seconds();

    if trimmed.eq_ignore_ascii_case("now") {
        return Ok(Some(ParsedTime {
            canonical: format_epoch_rfc3339(now_epoch),
            epoch_seconds: now_epoch,
        }));
    }

    if let Some(duration) = parse_duration_seconds(trimmed)? {
        let epoch = now_epoch.saturating_sub(duration);
        return Ok(Some(ParsedTime {
            canonical: format_epoch_rfc3339(epoch),
            epoch_seconds: epoch,
        }));
    }

    if let Some(epoch) = parse_timestamp_epoch(trimmed)? {
        return Ok(Some(ParsedTime {
            canonical: format_epoch_rfc3339(epoch),
            epoch_seconds: epoch,
        }));
    }

    Err(format!(
        "invalid time format: \"{}\" (use duration like '1h' or timestamp like '2024-01-15T10:30:00Z')",
        trimmed
    ))
}

fn parse_duration_seconds(raw: &str) -> Result<Option<i64>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if let Some(value) = trimmed.strip_suffix('d') {
        let days: f64 = value
            .parse()
            .map_err(|_| format!("invalid duration: {trimmed}"))?;
        if days < 0.0 {
            return Err(format!("invalid duration: {trimmed}"));
        }
        let seconds = (days * 24.0 * 3600.0).round() as i64;
        return Ok(Some(seconds));
    }

    let (value, scale) = if let Some(v) = trimmed.strip_suffix('h') {
        (v, 3600.0)
    } else if let Some(v) = trimmed.strip_suffix('m') {
        (v, 60.0)
    } else if let Some(v) = trimmed.strip_suffix('s') {
        (v, 1.0)
    } else {
        return Ok(None);
    };

    let number: f64 = value
        .parse()
        .map_err(|_| format!("invalid duration: {trimmed}"))?;
    if number < 0.0 {
        return Err(format!("invalid duration: {trimmed}"));
    }

    Ok(Some((number * scale).round() as i64))
}

fn parse_timestamp_epoch(raw: &str) -> Result<Option<i64>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if trimmed.len() == 10 {
        let (year, month, day) = parse_date(trimmed)?;
        return Ok(Some(date_time_to_epoch(year, month, day, 0, 0, 0, 0)));
    }

    let Some((date_part, time_part)) = trimmed.split_once('T') else {
        return Ok(None);
    };

    let (year, month, day) = parse_date(date_part)?;

    let mut clock = time_part;
    let mut offset_seconds = 0_i64;

    if let Some(stripped) = time_part.strip_suffix('Z') {
        clock = stripped;
    } else if let Some((time, offset)) = split_tz_offset(time_part) {
        clock = time;
        offset_seconds = parse_tz_offset_seconds(offset)?;
    }

    let (hour, minute, second) = parse_clock(clock)?;
    Ok(Some(date_time_to_epoch(
        year,
        month,
        day,
        hour,
        minute,
        second,
        offset_seconds,
    )))
}

fn split_tz_offset(raw: &str) -> Option<(&str, &str)> {
    let bytes = raw.as_bytes();
    for idx in (0..bytes.len()).rev() {
        if (bytes[idx] == b'+' || bytes[idx] == b'-') && idx >= 8 {
            return Some((&raw[..idx], &raw[idx..]));
        }
    }
    None
}

fn parse_tz_offset_seconds(raw: &str) -> Result<i64, String> {
    if raw.len() != 6 {
        return Err(format!("invalid timezone offset: {raw}"));
    }
    let sign = match &raw[0..1] {
        "+" => 1_i64,
        "-" => -1_i64,
        _ => return Err(format!("invalid timezone offset: {raw}")),
    };
    if &raw[3..4] != ":" {
        return Err(format!("invalid timezone offset: {raw}"));
    }
    let hours: i64 = raw[1..3]
        .parse()
        .map_err(|_| format!("invalid timezone offset: {raw}"))?;
    let minutes: i64 = raw[4..6]
        .parse()
        .map_err(|_| format!("invalid timezone offset: {raw}"))?;
    Ok(sign * (hours * 3600 + minutes * 60))
}

fn parse_date(raw: &str) -> Result<(i32, u32, u32), String> {
    if raw.len() != 10 || &raw[4..5] != "-" || &raw[7..8] != "-" {
        return Err(format!("invalid date: {raw}"));
    }
    let year: i32 = raw[0..4]
        .parse()
        .map_err(|_| format!("invalid date: {raw}"))?;
    let month: u32 = raw[5..7]
        .parse()
        .map_err(|_| format!("invalid date: {raw}"))?;
    let day: u32 = raw[8..10]
        .parse()
        .map_err(|_| format!("invalid date: {raw}"))?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(format!("invalid date: {raw}"));
    }
    Ok((year, month, day))
}

fn parse_clock(raw: &str) -> Result<(u32, u32, u32), String> {
    if raw.len() < 8 || &raw[2..3] != ":" || &raw[5..6] != ":" {
        return Err(format!("invalid time: {raw}"));
    }
    let hour: u32 = raw[0..2]
        .parse()
        .map_err(|_| format!("invalid time: {raw}"))?;
    let minute: u32 = raw[3..5]
        .parse()
        .map_err(|_| format!("invalid time: {raw}"))?;
    let second: u32 = raw[6..8]
        .parse()
        .map_err(|_| format!("invalid time: {raw}"))?;
    if hour > 23 || minute > 59 || second > 60 {
        return Err(format!("invalid time: {raw}"));
    }
    Ok((hour, minute, second.min(59)))
}

fn date_time_to_epoch(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    offset_seconds: i64,
) -> i64 {
    let days = civil_to_days(year, month, day);
    days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64 - offset_seconds
}

fn civil_to_days(year: i32, month: u32, day: u32) -> i64 {
    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = (adjusted_year as i64).div_euclid(400);
    let yoe = adjusted_year as i64 - era * 400;
    let month_index = month as i64 + if month > 2 { -3 } else { 9 };
    let doy = (153 * month_index + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_to_civil(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let adjusted_year = if month <= 2 { year + 1 } else { year };
    (adjusted_year, month, day)
}

fn format_epoch_rfc3339(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400);
    let seconds_of_day = epoch.rem_euclid(86_400);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    let (year, month, day) = days_to_civil(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn format_table_time(raw: &str) -> String {
    match parse_since(raw) {
        Ok(Some(parsed)) => {
            let canonical = parsed.canonical;
            if canonical.len() >= 19 {
                let mut display = canonical[0..19].to_string();
                display.replace_range(10..11, " ");
                display
            } else {
                canonical
            }
        }
        _ => raw.to_string(),
    }
}

fn now_epoch_seconds() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(_) => 0,
    }
}

fn resolve_database_path() -> PathBuf {
    crate::runtime_paths::resolve_database_path()
}

const HELP_TEXT: &str = "View the Forge audit log

Usage:
  forge audit [flags]

Examples:
  forge audit --since 1h
  forge audit --type agent.state_changed --entity-type agent
  forge audit --action message.dispatched --limit 200

Flags:
      --type string         filter by event type (comma-separated)
      --action string       alias for --type
      --entity-type string  filter by entity type (node, workspace, agent, queue, account, system)
      --entity-id string    filter by entity ID
      --since string        filter events after a time (duration or timestamp)
      --until string        filter events before a time (duration or timestamp)
      --cursor string       start after this event ID
      --limit int           max number of events to return (default 100)
      --json                output in JSON format
      --jsonl               output in JSON Lines format";

#[cfg(test)]
mod tests {
    use super::{
        parse_event_types, parse_since, run_for_test, AuditEvent, CommandOutput,
        InMemoryAuditBackend, SqliteAuditBackend,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_event_types_rejects_empty_filter() {
        let err = parse_event_types(" , ");
        assert_eq!(err, Err("event type filter cannot be empty".to_string()));
    }

    #[test]
    fn parse_since_handles_rfc3339_and_date() {
        let rfc3339 = parse_since("2026-01-01T00:00:00Z");
        assert!(rfc3339.is_ok());
        assert_eq!(
            rfc3339.ok().flatten().map(|parsed| parsed.canonical),
            Some("2026-01-01T00:00:00Z".to_string())
        );

        let date = parse_since("2026-01-01");
        assert!(date.is_ok());
        assert_eq!(
            date.ok().flatten().map(|parsed| parsed.canonical),
            Some("2026-01-01T00:00:00Z".to_string())
        );
    }

    #[test]
    fn parse_since_rejects_invalid() {
        let err = parse_since("not-a-time");
        assert!(err.is_err());
    }

    #[test]
    fn audit_empty_table_reports_no_matches() {
        let backend = InMemoryAuditBackend::default();
        let out = run_for_test(&["audit"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("TIME"));
        assert!(out.stdout.contains("TYPE"));
        assert!(out.stdout.contains("ENTITY"));
        assert!(out.stdout.contains("ID"));
        assert!(out
            .stdout
            .contains("No events matched the current filters."));
    }

    #[test]
    fn audit_jsonl_writes_one_line_per_event() {
        let backend = InMemoryAuditBackend::with_events(vec![AuditEvent {
            id: "evt-1".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            event_type: "agent.state_changed".to_string(),
            entity_type: "agent".to_string(),
            entity_id: "agent-1".to_string(),
            payload: "{\"ok\":true}".to_string(),
            metadata: None,
        }]);

        let out = run_for_test(&["audit", "--jsonl"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("\"type\":\"agent.state_changed\""));
        assert!(out.stdout.ends_with('\n'));
    }

    fn temp_db_path(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-cli-audit-{tag}-{nanos}-{}-{suffix}.sqlite",
            std::process::id(),
        ))
    }

    #[test]
    fn audit_sqlite_backend_queries_events_with_filters() {
        let db_path = temp_db_path("sqlite-filter");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let repo = forge_db::event_repository::EventRepository::new(&db);
        let mut matching = forge_db::event_repository::Event {
            id: "evt-match".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            event_type: "agent.state_changed".to_string(),
            entity_type: "agent".to_string(),
            entity_id: "agent-1".to_string(),
            payload: r#"{"state":"running"}"#.to_string(),
            metadata: None,
        };
        repo.create(&mut matching)
            .unwrap_or_else(|err| panic!("create matching event: {err}"));
        let mut non_matching = forge_db::event_repository::Event {
            id: "evt-other".to_string(),
            timestamp: "2026-01-01T00:00:10Z".to_string(),
            event_type: "message.dispatched".to_string(),
            entity_type: "queue".to_string(),
            entity_id: "queue-1".to_string(),
            payload: "{}".to_string(),
            metadata: None,
        };
        repo.create(&mut non_matching)
            .unwrap_or_else(|err| panic!("create non-matching event: {err}"));

        let backend = SqliteAuditBackend::new(db_path.clone());
        let out = run_for_test(
            &[
                "audit",
                "--json",
                "--type",
                "agent.state_changed",
                "--entity-type",
                "agent",
            ],
            &backend,
        );
        assert_success(&out);
        assert!(out.stdout.contains("\"id\": \"evt-match\""));
        assert!(!out.stdout.contains("\"id\": \"evt-other\""));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn audit_sqlite_backend_missing_db_reports_no_matches() {
        let db_path = temp_db_path("sqlite-missing");
        let _ = std::fs::remove_file(&db_path);

        let backend = SqliteAuditBackend::new(db_path);
        let out = run_for_test(&["audit"], &backend);
        assert_success(&out);
        assert!(out
            .stdout
            .contains("No events matched the current filters."));
    }

    fn assert_success(out: &CommandOutput) {
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
    }
}
