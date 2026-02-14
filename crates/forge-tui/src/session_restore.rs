//! Session restore snapshots with privacy-safe persistence and delta digests.

use std::collections::BTreeSet;

const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRestorePolicy {
    pub restore_enabled: bool,
    pub persist_enabled: bool,
    pub persist_filter_query: bool,
    pub persist_selected_run: bool,
}

impl Default for SessionRestorePolicy {
    fn default() -> Self {
        Self {
            restore_enabled: true,
            persist_enabled: true,
            persist_filter_query: false,
            persist_selected_run: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneSelection {
    pub pane_id: String,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionContext {
    pub selected_loop_id: Option<String>,
    pub selected_run_id: Option<String>,
    pub log_scroll: usize,
    pub tab_id: Option<String>,
    pub layout_id: Option<String>,
    pub filter_state: Option<String>,
    pub filter_query: Option<String>,
    pub panes: Vec<PaneSelection>,
    pub pinned_loop_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedSessionSnapshot {
    pub schema_version: u32,
    pub saved_at_epoch_s: i64,
    pub selected_loop_id: Option<String>,
    pub selected_run_id: Option<String>,
    pub log_scroll: usize,
    pub tab_id: Option<String>,
    pub layout_id: Option<String>,
    pub filter_state: Option<String>,
    pub filter_query: Option<String>,
    pub filter_query_digest: Option<String>,
    pub panes: Vec<PaneSelection>,
    pub pinned_loop_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestoreUniverse {
    pub loop_ids: Vec<String>,
    pub tab_ids: Vec<String>,
    pub layout_ids: Vec<String>,
    pub pane_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestoredSession {
    pub context: SessionContext,
    pub notices: Vec<String>,
    pub from_snapshot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionDeltaDigest {
    pub headline: String,
    pub change_count: usize,
    pub lines: Vec<String>,
}

#[must_use]
pub fn snapshot_session_context(
    context: &SessionContext,
    policy: &SessionRestorePolicy,
    saved_at_epoch_s: i64,
) -> Option<PersistedSessionSnapshot> {
    if !policy.persist_enabled {
        return None;
    }

    let query = normalize_optional(context.filter_query.as_deref());
    let query_digest = query.as_deref().map(stable_digest);
    let persisted_query = if policy.persist_filter_query {
        query
    } else {
        None
    };

    Some(PersistedSessionSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        saved_at_epoch_s: saved_at_epoch_s.max(0),
        selected_loop_id: normalize_optional(context.selected_loop_id.as_deref()),
        selected_run_id: if policy.persist_selected_run {
            normalize_optional(context.selected_run_id.as_deref())
        } else {
            None
        },
        log_scroll: context.log_scroll,
        tab_id: normalize_optional(context.tab_id.as_deref()),
        layout_id: normalize_optional(context.layout_id.as_deref()),
        filter_state: normalize_optional(context.filter_state.as_deref()),
        filter_query: persisted_query,
        filter_query_digest: query_digest,
        panes: normalize_panes(&context.panes),
        pinned_loop_ids: normalize_id_list(&context.pinned_loop_ids),
    })
}

#[must_use]
pub fn restore_session_context(
    snapshot: Option<&PersistedSessionSnapshot>,
    universe: &RestoreUniverse,
    policy: &SessionRestorePolicy,
) -> RestoredSession {
    if !policy.restore_enabled {
        return RestoredSession {
            context: SessionContext::default(),
            notices: vec!["session restore disabled by user policy".to_owned()],
            from_snapshot: false,
        };
    }

    let Some(snapshot) = snapshot else {
        return RestoredSession {
            context: SessionContext::default(),
            notices: vec!["no previous session snapshot".to_owned()],
            from_snapshot: false,
        };
    };

    let mut notices = Vec::new();
    let loop_ids = normalized_allowed(&universe.loop_ids);
    let tab_ids = normalized_allowed(&universe.tab_ids);
    let layout_ids = normalized_allowed(&universe.layout_ids);
    let pane_ids = normalized_allowed(&universe.pane_ids);

    let selected_loop_id = retain_if_allowed(snapshot.selected_loop_id.as_deref(), &loop_ids);
    if snapshot.selected_loop_id.is_some() && selected_loop_id.is_none() {
        notices.push("selected loop no longer available; restored as none".to_owned());
    }

    let tab_id = retain_if_allowed(snapshot.tab_id.as_deref(), &tab_ids)
        .or_else(|| tab_ids.iter().next().cloned());
    if snapshot.tab_id.is_some() && snapshot.tab_id != tab_id {
        notices.push("stored tab unavailable; restored to default tab".to_owned());
    }

    let layout_id = retain_if_allowed(snapshot.layout_id.as_deref(), &layout_ids)
        .or_else(|| layout_ids.iter().next().cloned());
    if snapshot.layout_id.is_some() && snapshot.layout_id != layout_id {
        notices.push("stored layout unavailable; restored to default layout".to_owned());
    }

    let filter_state = normalize_optional(snapshot.filter_state.as_deref()).or_else(|| {
        if snapshot.filter_query_digest.is_some() {
            Some("all".to_owned())
        } else {
            None
        }
    });

    let filter_query = normalize_optional(snapshot.filter_query.as_deref());
    if filter_query.is_none() && snapshot.filter_query_digest.is_some() {
        notices.push("filter query omitted by privacy-safe storage policy".to_owned());
    }

    let mut panes = Vec::new();
    let mut focused_seen = false;
    for pane in normalize_panes(&snapshot.panes) {
        if !pane_ids.is_empty() && !pane_ids.contains(&pane.pane_id) {
            continue;
        }
        let focused = pane.focused && !focused_seen;
        if focused {
            focused_seen = true;
        }
        panes.push(PaneSelection {
            pane_id: pane.pane_id,
            focused,
        });
    }
    if panes.is_empty() {
        if let Some(first) = pane_ids.iter().next() {
            panes.push(PaneSelection {
                pane_id: first.clone(),
                focused: true,
            });
        }
    } else if !panes.iter().any(|pane| pane.focused) {
        if let Some(first) = panes.first_mut() {
            first.focused = true;
        }
    }
    if snapshot.panes.len() != panes.len() {
        notices.push("some panes were unavailable and not restored".to_owned());
    }

    let pinned_loop_ids = snapshot
        .pinned_loop_ids
        .iter()
        .map(|id| normalize_id(id))
        .filter(|id| !id.is_empty() && (loop_ids.is_empty() || loop_ids.contains(id)))
        .collect::<Vec<_>>();

    RestoredSession {
        context: SessionContext {
            selected_loop_id,
            selected_run_id: normalize_optional(snapshot.selected_run_id.as_deref()),
            log_scroll: snapshot.log_scroll,
            tab_id,
            layout_id,
            filter_state,
            filter_query,
            panes,
            pinned_loop_ids,
        },
        notices,
        from_snapshot: true,
    }
}

#[must_use]
pub fn build_delta_digest(
    previous: Option<&PersistedSessionSnapshot>,
    current: &PersistedSessionSnapshot,
) -> SessionDeltaDigest {
    let Some(previous) = previous else {
        return SessionDeltaDigest {
            headline: "first session snapshot captured".to_owned(),
            change_count: 1,
            lines: vec!["baseline context recorded".to_owned()],
        };
    };

    let mut lines = Vec::new();
    push_optional_change(
        "selected loop",
        previous.selected_loop_id.as_deref(),
        current.selected_loop_id.as_deref(),
        &mut lines,
    );
    push_optional_change(
        "selected run",
        previous.selected_run_id.as_deref(),
        current.selected_run_id.as_deref(),
        &mut lines,
    );
    if previous.log_scroll != current.log_scroll {
        lines.push(format!(
            "log scroll changed: {} -> {}",
            previous.log_scroll, current.log_scroll
        ));
    }
    push_optional_change(
        "tab",
        previous.tab_id.as_deref(),
        current.tab_id.as_deref(),
        &mut lines,
    );
    push_optional_change(
        "layout",
        previous.layout_id.as_deref(),
        current.layout_id.as_deref(),
        &mut lines,
    );
    push_optional_change(
        "filter state",
        previous.filter_state.as_deref(),
        current.filter_state.as_deref(),
        &mut lines,
    );

    let previous_query_digest = effective_query_digest(previous);
    let current_query_digest = effective_query_digest(current);
    if previous_query_digest != current_query_digest {
        lines.push("filter query changed (privacy-safe digest delta)".to_owned());
    }

    let previous_panes = pane_signature(&previous.panes);
    let current_panes = pane_signature(&current.panes);
    if previous_panes != current_panes {
        lines.push(format!(
            "pane set changed: {} -> {}",
            render_panes(&previous.panes),
            render_panes(&current.panes)
        ));
    }

    let previous_pins = normalize_id_list(&previous.pinned_loop_ids);
    let current_pins = normalize_id_list(&current.pinned_loop_ids);
    if previous_pins != current_pins {
        let added = current_pins
            .iter()
            .filter(|id| !previous_pins.contains(id))
            .count();
        let removed = previous_pins
            .iter()
            .filter(|id| !current_pins.contains(id))
            .count();
        lines.push(format!("pinned loops changed: +{added} -{removed}"));
    }

    if lines.is_empty() {
        return SessionDeltaDigest {
            headline: "no context changes since last session".to_owned(),
            change_count: 0,
            lines,
        };
    }

    SessionDeltaDigest {
        headline: format!("{} context changes since last session", lines.len()),
        change_count: lines.len(),
        lines,
    }
}

fn retain_if_allowed(value: Option<&str>, allowed: &BTreeSet<String>) -> Option<String> {
    let normalized = normalize_optional(value)?;
    if allowed.is_empty() || allowed.contains(&normalized) {
        Some(normalized)
    } else {
        None
    }
}

fn normalized_allowed(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .map(|value| normalize_id(value))
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalize_panes(panes: &[PaneSelection]) -> Vec<PaneSelection> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for pane in panes {
        let pane_id = normalize_id(&pane.pane_id);
        if pane_id.is_empty() || seen.contains(&pane_id) {
            continue;
        }
        seen.insert(pane_id.clone());
        normalized.push(PaneSelection {
            pane_id,
            focused: pane.focused,
        });
    }
    normalized
}

fn normalize_id_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| normalize_id(value))
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn effective_query_digest(snapshot: &PersistedSessionSnapshot) -> Option<String> {
    if let Some(digest) = normalize_optional(snapshot.filter_query_digest.as_deref()) {
        return Some(digest);
    }
    let query = normalize_optional(snapshot.filter_query.as_deref())?;
    Some(stable_digest(&query))
}

fn stable_digest(value: &str) -> String {
    let mut hash = 1469598103934665603_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211_u64);
    }
    format!("{hash:016x}")
}

fn pane_signature(panes: &[PaneSelection]) -> Vec<String> {
    normalize_panes(panes)
        .iter()
        .map(|pane| {
            if pane.focused {
                format!("{}*", pane.pane_id)
            } else {
                pane.pane_id.clone()
            }
        })
        .collect()
}

fn render_panes(panes: &[PaneSelection]) -> String {
    let rendered = pane_signature(panes);
    if rendered.is_empty() {
        "none".to_owned()
    } else {
        rendered.join(",")
    }
}

fn push_optional_change(
    label: &str,
    previous: Option<&str>,
    current: Option<&str>,
    lines: &mut Vec<String>,
) {
    let previous = normalize_optional(previous);
    let current = normalize_optional(current);
    if previous != current {
        lines.push(format!(
            "{label} changed: {} -> {}",
            display_value(previous.as_deref()),
            display_value(current.as_deref())
        ));
    }
}

fn display_value(value: Option<&str>) -> String {
    value.map_or_else(|| "none".to_owned(), ToOwned::to_owned)
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
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
        build_delta_digest, restore_session_context, snapshot_session_context, PaneSelection,
        PersistedSessionSnapshot, RestoreUniverse, SessionContext, SessionRestorePolicy,
    };

    fn sample_context() -> SessionContext {
        SessionContext {
            selected_loop_id: Some("Loop-A".to_owned()),
            selected_run_id: Some("run-9".to_owned()),
            log_scroll: 12,
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

    #[test]
    fn snapshot_respects_storage_opt_out() {
        let context = sample_context();
        let policy = SessionRestorePolicy {
            persist_enabled: false,
            ..SessionRestorePolicy::default()
        };
        assert_eq!(snapshot_session_context(&context, &policy, 1700), None);
    }

    #[test]
    fn snapshot_redacts_filter_query_by_default() {
        let context = sample_context();
        let snapshot = match snapshot_session_context(
            &context,
            &SessionRestorePolicy::default(),
            1_700_000_000,
        ) {
            Some(snapshot) => snapshot,
            None => panic!("expected snapshot"),
        };
        assert_eq!(snapshot.filter_query, None);
        assert!(snapshot.filter_query_digest.is_some());
        assert_eq!(snapshot.selected_loop_id.as_deref(), Some("loop-a"));
    }

    #[test]
    fn snapshot_can_persist_filter_query_when_allowed() {
        let context = sample_context();
        let policy = SessionRestorePolicy {
            persist_filter_query: true,
            ..SessionRestorePolicy::default()
        };
        let snapshot = match snapshot_session_context(&context, &policy, 100) {
            Some(snapshot) => snapshot,
            None => panic!("expected snapshot"),
        };
        assert_eq!(snapshot.filter_query.as_deref(), Some("agent timeout"));
        assert!(snapshot.filter_query_digest.is_some());
    }

    #[test]
    fn restore_applies_availability_fallbacks_and_notices() {
        let snapshot = PersistedSessionSnapshot {
            schema_version: 1,
            saved_at_epoch_s: 10,
            selected_loop_id: Some("loop-z".to_owned()),
            selected_run_id: Some("run-2".to_owned()),
            log_scroll: 44,
            tab_id: Some("inbox".to_owned()),
            layout_id: Some("night".to_owned()),
            filter_state: Some("error".to_owned()),
            filter_query: None,
            filter_query_digest: Some("abc".to_owned()),
            panes: vec![
                PaneSelection {
                    pane_id: "overview".to_owned(),
                    focused: false,
                },
                PaneSelection {
                    pane_id: "runs".to_owned(),
                    focused: false,
                },
            ],
            pinned_loop_ids: vec!["loop-z".to_owned(), "loop-a".to_owned()],
        };
        let universe = RestoreUniverse {
            loop_ids: vec!["loop-a".to_owned(), "loop-b".to_owned()],
            tab_ids: vec!["overview".to_owned(), "runs".to_owned()],
            layout_ids: vec!["ops".to_owned()],
            pane_ids: vec!["overview".to_owned(), "logs".to_owned()],
        };

        let restored =
            restore_session_context(Some(&snapshot), &universe, &SessionRestorePolicy::default());

        assert_eq!(restored.context.selected_loop_id, None);
        assert_eq!(restored.context.log_scroll, 44);
        assert_eq!(restored.context.tab_id.as_deref(), Some("overview"));
        assert_eq!(restored.context.layout_id.as_deref(), Some("ops"));
        assert_eq!(restored.context.panes.len(), 1);
        assert_eq!(restored.context.panes[0].pane_id, "overview");
        assert!(restored.context.panes[0].focused);
        assert_eq!(restored.context.pinned_loop_ids, vec!["loop-a".to_owned()]);
        assert!(restored
            .notices
            .iter()
            .any(|msg| msg.contains("privacy-safe")));
    }

    #[test]
    fn restore_disabled_returns_empty_session() {
        let policy = SessionRestorePolicy {
            restore_enabled: false,
            ..SessionRestorePolicy::default()
        };
        let restored = restore_session_context(None, &RestoreUniverse::default(), &policy);
        assert_eq!(restored.context, SessionContext::default());
        assert!(!restored.from_snapshot);
        assert!(restored.notices.iter().any(|msg| msg.contains("disabled")));
    }

    #[test]
    fn delta_digest_reports_context_changes() {
        let previous = PersistedSessionSnapshot {
            schema_version: 1,
            saved_at_epoch_s: 10,
            selected_loop_id: Some("loop-a".to_owned()),
            selected_run_id: Some("run-1".to_owned()),
            log_scroll: 6,
            tab_id: Some("overview".to_owned()),
            layout_id: Some("ops".to_owned()),
            filter_state: Some("running".to_owned()),
            filter_query: None,
            filter_query_digest: Some("abc".to_owned()),
            panes: vec![PaneSelection {
                pane_id: "overview".to_owned(),
                focused: true,
            }],
            pinned_loop_ids: vec!["loop-a".to_owned()],
        };
        let current = PersistedSessionSnapshot {
            schema_version: 1,
            saved_at_epoch_s: 20,
            selected_loop_id: Some("loop-b".to_owned()),
            selected_run_id: Some("run-2".to_owned()),
            log_scroll: 0,
            tab_id: Some("runs".to_owned()),
            layout_id: Some("review".to_owned()),
            filter_state: Some("error".to_owned()),
            filter_query: None,
            filter_query_digest: Some("xyz".to_owned()),
            panes: vec![
                PaneSelection {
                    pane_id: "runs".to_owned(),
                    focused: true,
                },
                PaneSelection {
                    pane_id: "logs".to_owned(),
                    focused: false,
                },
            ],
            pinned_loop_ids: vec!["loop-b".to_owned(), "loop-c".to_owned()],
        };

        let digest = build_delta_digest(Some(&previous), &current);
        assert!(digest.change_count >= 6);
        assert!(digest
            .lines
            .iter()
            .any(|line| line.contains("filter query changed")));
    }

    #[test]
    fn delta_digest_stable_when_unchanged() {
        let snapshot = PersistedSessionSnapshot {
            schema_version: 1,
            saved_at_epoch_s: 10,
            selected_loop_id: Some("loop-a".to_owned()),
            selected_run_id: Some("run-1".to_owned()),
            log_scroll: 3,
            tab_id: Some("overview".to_owned()),
            layout_id: Some("ops".to_owned()),
            filter_state: Some("running".to_owned()),
            filter_query: None,
            filter_query_digest: Some("abc".to_owned()),
            panes: vec![PaneSelection {
                pane_id: "overview".to_owned(),
                focused: true,
            }],
            pinned_loop_ids: vec!["loop-a".to_owned()],
        };

        let digest = build_delta_digest(Some(&snapshot), &snapshot);
        assert_eq!(digest.change_count, 0);
        assert_eq!(digest.lines, Vec::<String>::new());
    }
}
