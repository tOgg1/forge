//! Crash-safe state snapshot persistence + recovery for Forge TUI.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, Value};

use crate::session_restore::{
    snapshot_session_context, PaneSelection, PersistedSessionSnapshot, SessionContext,
    SessionRestorePolicy,
};

pub const CRASH_SAFE_STATE_SCHEMA_VERSION: u32 = 1;

static TEMP_SUFFIX_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecoverySource {
    #[default]
    None,
    Primary,
    Backup,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrashRecoveryOutcome {
    pub snapshot: Option<PersistedSessionSnapshot>,
    pub source: RecoverySource,
    pub warnings: Vec<String>,
}

pub fn persist_context_snapshot(
    path: &Path,
    context: &SessionContext,
    policy: &SessionRestorePolicy,
    saved_at_epoch_s: i64,
) -> Result<(), String> {
    let Some(snapshot) = snapshot_session_context(context, policy, saved_at_epoch_s) else {
        return Ok(());
    };
    persist_snapshot(path, &snapshot)
}

pub fn persist_snapshot(path: &Path, snapshot: &PersistedSessionSnapshot) -> Result<(), String> {
    let serialized = serialize_snapshot_store(snapshot)?;
    if let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create snapshot directory {}: {err}", parent.display()))?;
    }

    if path.exists() {
        let backup = backup_path(path);
        fs::copy(path, &backup).map_err(|err| {
            format!(
                "copy snapshot {} -> {}: {err}",
                path.display(),
                backup.display()
            )
        })?;
    }

    let temp_path = temp_path(path);
    write_file_atomic(&temp_path, serialized.as_bytes())?;
    if let Err(err) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "rename snapshot {} -> {}: {err}",
            temp_path.display(),
            path.display()
        ));
    }

    sync_parent_dir(path);
    Ok(())
}

#[must_use]
pub fn recover_snapshot(path: &Path) -> CrashRecoveryOutcome {
    let mut warnings = Vec::new();

    if let Some(snapshot) = try_load_snapshot(path, "primary snapshot", &mut warnings) {
        return CrashRecoveryOutcome {
            snapshot: Some(snapshot),
            source: RecoverySource::Primary,
            warnings,
        };
    }

    let backup = backup_path(path);
    if let Some(snapshot) = try_load_snapshot(&backup, "backup snapshot", &mut warnings) {
        warnings.push("recovered session from backup snapshot".to_owned());
        return CrashRecoveryOutcome {
            snapshot: Some(snapshot),
            source: RecoverySource::Backup,
            warnings,
        };
    }

    if warnings.is_empty() {
        warnings.push("no crash-safe snapshot found".to_owned());
    }

    CrashRecoveryOutcome {
        snapshot: None,
        source: RecoverySource::None,
        warnings,
    }
}

fn try_load_snapshot(
    path: &Path,
    label: &str,
    warnings: &mut Vec<String>,
) -> Option<PersistedSessionSnapshot> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == ErrorKind::NotFound => return None,
        Err(err) => {
            warnings.push(format!("{label} unreadable; ignored ({err})"));
            return None;
        }
    };

    match parse_snapshot_store(&raw) {
        Ok((snapshot, parse_warnings)) => {
            for warning in parse_warnings {
                warnings.push(format!("{label}: {warning}"));
            }
            Some(snapshot)
        }
        Err(err) => {
            warnings.push(format!("{label} invalid; ignored ({err})"));
            None
        }
    }
}

fn serialize_snapshot_store(snapshot: &PersistedSessionSnapshot) -> Result<String, String> {
    let snapshot_value = snapshot_to_value(snapshot);
    let digest = snapshot_digest(&snapshot_value)?;
    let mut root = Map::new();
    root.insert(
        "schema_version".to_owned(),
        Value::from(CRASH_SAFE_STATE_SCHEMA_VERSION),
    );
    root.insert("snapshot".to_owned(), snapshot_value);
    root.insert("snapshot_digest".to_owned(), Value::from(digest));

    serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|err| format!("serialize crash-safe snapshot: {err}"))
}

fn parse_snapshot_store(raw: &str) -> Result<(PersistedSessionSnapshot, Vec<String>), String> {
    let value = serde_json::from_str::<Value>(raw).map_err(|err| format!("invalid json: {err}"))?;
    let Some(obj) = value.as_object() else {
        return Err("root must be an object".to_owned());
    };

    let mut warnings = Vec::new();
    let schema_version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(CRASH_SAFE_STATE_SCHEMA_VERSION as u64) as u32;
    if schema_version != CRASH_SAFE_STATE_SCHEMA_VERSION {
        warnings.push(format!(
            "unknown schema_version={schema_version}; attempting best-effort parse"
        ));
    }

    let snapshot_value = obj
        .get("snapshot")
        .cloned()
        .unwrap_or_else(|| Value::Object(obj.clone()));
    let snapshot = parse_snapshot_value(&snapshot_value, &mut warnings)?;
    if let Some(expected_digest) =
        normalize_optional(obj.get("snapshot_digest").and_then(Value::as_str))
    {
        let actual_digest = snapshot_digest(&snapshot_to_value(&snapshot))?;
        if expected_digest != actual_digest {
            return Err(format!(
                "snapshot_digest mismatch (expected={expected_digest}, actual={actual_digest})"
            ));
        }
    } else {
        warnings.push("snapshot_digest missing; accepted best-effort snapshot".to_owned());
    }

    Ok((snapshot, warnings))
}

fn parse_snapshot_value(
    value: &Value,
    warnings: &mut Vec<String>,
) -> Result<PersistedSessionSnapshot, String> {
    let Some(obj) = value.as_object() else {
        return Err("snapshot must be an object".to_owned());
    };

    let schema_version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(1) as u32;
    let saved_at_epoch_s = obj
        .get("saved_at_epoch_s")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);

    let panes = parse_panes(obj.get("panes"), warnings);
    let pinned_loop_ids = parse_id_list(obj.get("pinned_loop_ids"), warnings);

    Ok(PersistedSessionSnapshot {
        schema_version,
        saved_at_epoch_s,
        selected_loop_id: normalize_optional(obj.get("selected_loop_id").and_then(Value::as_str)),
        selected_run_id: normalize_optional(obj.get("selected_run_id").and_then(Value::as_str)),
        log_scroll: obj.get("log_scroll").and_then(Value::as_u64).unwrap_or(0) as usize,
        tab_id: normalize_optional(obj.get("tab_id").and_then(Value::as_str)),
        layout_id: normalize_optional(obj.get("layout_id").and_then(Value::as_str)),
        filter_state: normalize_optional(obj.get("filter_state").and_then(Value::as_str)),
        filter_query: normalize_optional(obj.get("filter_query").and_then(Value::as_str)),
        filter_query_digest: normalize_optional(
            obj.get("filter_query_digest").and_then(Value::as_str),
        ),
        panes,
        pinned_loop_ids,
    })
}

fn parse_panes(value: Option<&Value>, warnings: &mut Vec<String>) -> Vec<PaneSelection> {
    let Some(values) = value.and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut panes = Vec::new();
    let mut seen = BTreeSet::new();
    let mut focused_seen = false;

    for (index, item) in values.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            warnings.push(format!("panes[{index}] ignored (not object)"));
            continue;
        };
        let pane_id =
            normalize_optional(obj.get("pane_id").and_then(Value::as_str)).unwrap_or_default();
        if pane_id.is_empty() {
            warnings.push(format!("panes[{index}] ignored (empty pane_id)"));
            continue;
        }
        if !seen.insert(pane_id.clone()) {
            warnings.push(format!(
                "panes[{index}] ignored (duplicate pane_id={pane_id})"
            ));
            continue;
        }

        let requested_focus = obj.get("focused").and_then(Value::as_bool).unwrap_or(false);
        let focused = requested_focus && !focused_seen;
        if focused {
            focused_seen = true;
        }

        panes.push(PaneSelection { pane_id, focused });
    }

    if !panes.is_empty() && !panes.iter().any(|pane| pane.focused) {
        if let Some(first) = panes.first_mut() {
            first.focused = true;
        }
    }

    panes
}

fn parse_id_list(value: Option<&Value>, warnings: &mut Vec<String>) -> Vec<String> {
    let Some(values) = value.and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut normalized = BTreeSet::new();
    for (index, item) in values.iter().enumerate() {
        let Some(raw) = item.as_str() else {
            warnings.push(format!("pinned_loop_ids[{index}] ignored (not string)"));
            continue;
        };
        if let Some(id) = normalize_optional(Some(raw)) {
            normalized.insert(id);
        } else {
            warnings.push(format!("pinned_loop_ids[{index}] ignored (empty id)"));
        }
    }

    normalized.into_iter().collect()
}

fn snapshot_to_value(snapshot: &PersistedSessionSnapshot) -> Value {
    let mut root = Map::new();
    root.insert(
        "schema_version".to_owned(),
        Value::from(snapshot.schema_version),
    );
    root.insert(
        "saved_at_epoch_s".to_owned(),
        Value::from(snapshot.saved_at_epoch_s.max(0)),
    );
    root.insert(
        "selected_loop_id".to_owned(),
        optional_value(snapshot.selected_loop_id.as_deref()),
    );
    root.insert(
        "selected_run_id".to_owned(),
        optional_value(snapshot.selected_run_id.as_deref()),
    );
    root.insert(
        "log_scroll".to_owned(),
        Value::from(snapshot.log_scroll as u64),
    );
    root.insert(
        "tab_id".to_owned(),
        optional_value(snapshot.tab_id.as_deref()),
    );
    root.insert(
        "layout_id".to_owned(),
        optional_value(snapshot.layout_id.as_deref()),
    );
    root.insert(
        "filter_state".to_owned(),
        optional_value(snapshot.filter_state.as_deref()),
    );
    root.insert(
        "filter_query".to_owned(),
        optional_value(snapshot.filter_query.as_deref()),
    );
    root.insert(
        "filter_query_digest".to_owned(),
        optional_value(snapshot.filter_query_digest.as_deref()),
    );
    root.insert(
        "panes".to_owned(),
        Value::Array(
            snapshot
                .panes
                .iter()
                .map(|pane| {
                    let mut item = Map::new();
                    item.insert("pane_id".to_owned(), Value::from(pane.pane_id.clone()));
                    item.insert("focused".to_owned(), Value::from(pane.focused));
                    Value::Object(item)
                })
                .collect(),
        ),
    );
    root.insert(
        "pinned_loop_ids".to_owned(),
        Value::Array(
            snapshot
                .pinned_loop_ids
                .iter()
                .map(|id| Value::from(id.clone()))
                .collect(),
        ),
    );
    Value::Object(root)
}

fn optional_value(value: Option<&str>) -> Value {
    match normalize_optional(value) {
        Some(value) => Value::from(value),
        None => Value::Null,
    }
}

fn snapshot_digest(snapshot_value: &Value) -> Result<String, String> {
    let compact = serde_json::to_string(snapshot_value)
        .map_err(|err| format!("serialize snapshot payload for digest: {err}"))?;
    Ok(stable_digest(&compact))
}

fn stable_digest(value: &str) -> String {
    let mut hash = 1469598103934665603_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211_u64);
    }
    format!("{hash:016x}")
}

fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|err| format!("open {}: {err}", path.display()))?;
    file.write_all(bytes)
        .map_err(|err| format!("write {}: {err}", path.display()))?;
    file.sync_all()
        .map_err(|err| format!("sync {}: {err}", path.display()))?;
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path_with_suffix(path, ".bak")
}

fn temp_path(path: &Path) -> PathBuf {
    let pid = std::process::id();
    let suffix = TEMP_SUFFIX_COUNTER.fetch_add(1, Ordering::Relaxed);
    path_with_suffix(path, &format!(".tmp-{pid}-{suffix}"))
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

fn sync_parent_dir(path: &Path) {
    let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) else {
        return;
    };
    if let Ok(dir) = OpenOptions::new().read(true).open(parent) {
        let _ = dir.sync_all();
    }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{
        persist_context_snapshot, persist_snapshot, recover_snapshot, CrashRecoveryOutcome,
        RecoverySource,
    };
    use crate::session_restore::{
        snapshot_session_context, PaneSelection, SessionContext, SessionRestorePolicy,
    };
    use serde_json::Value;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn persist_and_recover_round_trip_uses_primary() {
        let path = temp_path("round-trip");
        let snapshot = sample_snapshot("loop-a", 100);

        persist_snapshot(&path, &snapshot).unwrap_or_else(|err| panic!("persist: {err}"));
        let recovered = recover_snapshot(&path);

        assert_eq!(recovered.source, RecoverySource::Primary);
        assert_eq!(
            recovered
                .snapshot
                .as_ref()
                .and_then(|item| item.selected_loop_id.as_deref()),
            Some("loop-a")
        );

        cleanup(&path);
    }

    #[test]
    fn recovery_falls_back_to_backup_when_primary_is_corrupt() {
        let path = temp_path("backup-fallback");
        let first = sample_snapshot("loop-a", 100);
        let second = sample_snapshot("loop-b", 200);

        persist_snapshot(&path, &first).unwrap_or_else(|err| panic!("persist first: {err}"));
        persist_snapshot(&path, &second).unwrap_or_else(|err| panic!("persist second: {err}"));
        fs::write(&path, "{not-json").unwrap_or_else(|err| panic!("corrupt primary: {err}"));

        let recovered = recover_snapshot(&path);
        assert_eq!(recovered.source, RecoverySource::Backup);
        assert_eq!(
            recovered
                .snapshot
                .as_ref()
                .and_then(|item| item.selected_loop_id.as_deref()),
            Some("loop-a")
        );
        assert!(recovered
            .warnings
            .iter()
            .any(|line| line.contains("primary snapshot invalid")));

        cleanup(&path);
    }

    #[test]
    fn recovery_rejects_digest_mismatch_and_uses_backup() {
        let path = temp_path("digest-mismatch");
        let first = sample_snapshot("loop-a", 100);
        let second = sample_snapshot("loop-b", 200);

        persist_snapshot(&path, &first).unwrap_or_else(|err| panic!("persist first: {err}"));
        persist_snapshot(&path, &second).unwrap_or_else(|err| panic!("persist second: {err}"));

        let raw = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read primary: {err}"));
        let mut value = serde_json::from_str::<Value>(&raw)
            .unwrap_or_else(|err| panic!("parse primary json: {err}"));
        let Some(obj) = value.as_object_mut() else {
            panic!("expected object");
        };
        obj.insert("snapshot_digest".to_owned(), Value::from("deadbeef"));
        let mutated =
            serde_json::to_string_pretty(&value).unwrap_or_else(|err| panic!("serialize: {err}"));
        fs::write(&path, mutated).unwrap_or_else(|err| panic!("mutate primary: {err}"));

        let recovered = recover_snapshot(&path);
        assert_eq!(recovered.source, RecoverySource::Backup);
        assert_eq!(
            recovered
                .snapshot
                .as_ref()
                .and_then(|item| item.selected_loop_id.as_deref()),
            Some("loop-a")
        );
        assert!(recovered
            .warnings
            .iter()
            .any(|line| line.contains("snapshot_digest mismatch")));

        cleanup(&path);
    }

    #[test]
    fn persist_context_snapshot_respects_policy_opt_out() {
        let path = temp_path("policy-opt-out");
        let context = sample_context("loop-a");
        let policy = SessionRestorePolicy {
            persist_enabled: false,
            ..SessionRestorePolicy::default()
        };
        persist_context_snapshot(&path, &context, &policy, 500)
            .unwrap_or_else(|err| panic!("persist context: {err}"));
        let recovered = recover_snapshot(&path);
        assert_eq!(
            recovered,
            CrashRecoveryOutcome {
                snapshot: None,
                source: RecoverySource::None,
                warnings: vec!["no crash-safe snapshot found".to_owned()],
            }
        );
        cleanup(&path);
    }

    #[test]
    fn persist_context_snapshot_round_trip() {
        let path = temp_path("context-round-trip");
        let context = sample_context("Loop-A");
        let policy = SessionRestorePolicy::default();

        persist_context_snapshot(&path, &context, &policy, 1234)
            .unwrap_or_else(|err| panic!("persist context: {err}"));
        let recovered = recover_snapshot(&path);

        assert_eq!(recovered.source, RecoverySource::Primary);
        assert_eq!(
            recovered
                .snapshot
                .as_ref()
                .and_then(|item| item.selected_loop_id.as_deref()),
            Some("loop-a")
        );
        assert_eq!(
            recovered
                .snapshot
                .as_ref()
                .and_then(|item| item.selected_run_id.as_deref()),
            Some("run-9")
        );
        assert_eq!(
            recovered.snapshot.as_ref().map(|item| item.log_scroll),
            Some(21)
        );

        cleanup(&path);
    }

    fn sample_snapshot(
        loop_id: &str,
        saved_at_epoch_s: i64,
    ) -> crate::session_restore::PersistedSessionSnapshot {
        let context = sample_context(loop_id);
        snapshot_session_context(&context, &SessionRestorePolicy::default(), saved_at_epoch_s)
            .unwrap_or_else(|| panic!("snapshot"))
    }

    fn sample_context(loop_id: &str) -> SessionContext {
        SessionContext {
            selected_loop_id: Some(loop_id.to_owned()),
            selected_run_id: Some("run-9".to_owned()),
            log_scroll: 21,
            tab_id: Some("overview".to_owned()),
            layout_id: Some("ops".to_owned()),
            filter_state: Some("running".to_owned()),
            filter_query: Some("agent timeout".to_owned()),
            panes: vec![
                PaneSelection {
                    pane_id: "overview".to_owned(),
                    focused: true,
                },
                PaneSelection {
                    pane_id: "logs".to_owned(),
                    focused: false,
                },
            ],
            pinned_loop_ids: vec!["loop-a".to_owned(), "loop-b".to_owned()],
        }
    }

    fn temp_path(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-tui-crash-safe-{tag}-{pid}-{nanos}-{seq}.json"
        ))
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path);
        let mut backup = path.as_os_str().to_os_string();
        backup.push(".bak");
        let _ = fs::remove_file(PathBuf::from(backup));
    }
}
