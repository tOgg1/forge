//! Log-anchor bookmarks + lightweight annotations with handoff export/import.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

pub const LOG_ANCHOR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogAnchor {
    pub anchor_id: String,
    pub marker: String,
    pub loop_id: String,
    pub log_source: String,
    pub line_index: usize,
    pub timestamp: String,
    pub excerpt: String,
    pub annotation: String,
    pub tags: Vec<String>,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogAnchorDraft {
    pub marker: String,
    pub loop_id: String,
    pub log_source: String,
    pub line_index: usize,
    pub timestamp: String,
    pub excerpt: String,
    pub annotation: String,
    pub tags: Vec<String>,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogAnchorFilter {
    pub marker: String,
    pub loop_id: String,
    pub log_source: String,
    pub text: String,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogAnchorStore {
    anchors: Vec<LogAnchor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportAnchorsOutcome {
    pub imported: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

impl LogAnchorStore {
    #[must_use]
    pub fn anchors(&self) -> &[LogAnchor] {
        &self.anchors
    }

    #[must_use]
    pub fn get(&self, anchor_id: &str) -> Option<&LogAnchor> {
        let anchor_id = normalize_required(anchor_id);
        self.anchors
            .iter()
            .find(|anchor| anchor.anchor_id == anchor_id)
    }

    #[must_use]
    pub fn contains(&self, anchor_id: &str) -> bool {
        self.get(anchor_id).is_some()
    }

    #[must_use]
    pub fn get_by_marker(&self, marker: &str) -> Option<&LogAnchor> {
        let marker = normalize_marker(marker);
        if marker.is_empty() {
            return None;
        }
        self.anchors.iter().find(|anchor| anchor.marker == marker)
    }
}

pub fn add_log_anchor(store: &mut LogAnchorStore, draft: LogAnchorDraft) -> Result<String, String> {
    let loop_id = normalize_required(&draft.loop_id);
    if loop_id.is_empty() {
        return Err("loop_id is required".to_owned());
    }

    let log_source = normalize_log_source(&draft.log_source);
    let excerpt = normalize_excerpt(&draft.excerpt);
    if excerpt.is_empty() {
        return Err("excerpt is required".to_owned());
    }

    let created_by = normalize_required(&draft.created_by);
    if created_by.is_empty() {
        return Err("created_by is required".to_owned());
    }

    let created_at = normalize_required(&draft.created_at);
    if created_at.is_empty() {
        return Err("created_at is required".to_owned());
    }

    let marker = next_marker(
        &normalize_marker(&draft.marker),
        draft.line_index,
        &store.anchors,
    );

    let base_id = format!(
        "{}:{}:{}",
        slugify(&loop_id),
        slugify(&log_source),
        draft.line_index
    );
    let anchor_id = next_anchor_id(&base_id, &store.anchors);

    let anchor = LogAnchor {
        anchor_id: anchor_id.clone(),
        marker,
        loop_id,
        log_source,
        line_index: draft.line_index,
        timestamp: draft.timestamp.trim().to_owned(),
        excerpt,
        annotation: draft.annotation.trim().to_owned(),
        tags: normalize_tags(&draft.tags),
        created_by,
        created_at,
    };

    store.anchors.push(anchor);
    sort_anchors(&mut store.anchors);
    Ok(anchor_id)
}

pub fn annotate_log_anchor(
    store: &mut LogAnchorStore,
    anchor_id: &str,
    annotation: &str,
) -> Result<(), String> {
    let anchor_id = normalize_required(anchor_id);
    let Some(anchor) = store
        .anchors
        .iter_mut()
        .find(|anchor| anchor.anchor_id == anchor_id)
    else {
        return Err(format!("anchor '{}' not found", anchor_id));
    };

    anchor.annotation = annotation.trim().to_owned();
    Ok(())
}

pub fn remove_log_anchor(store: &mut LogAnchorStore, anchor_id: &str) -> Result<(), String> {
    let anchor_id = normalize_required(anchor_id);
    let Some(index) = store
        .anchors
        .iter()
        .position(|anchor| anchor.anchor_id == anchor_id)
    else {
        return Err(format!("anchor '{}' not found", anchor_id));
    };
    store.anchors.remove(index);
    Ok(())
}

#[must_use]
pub fn list_log_anchors(store: &LogAnchorStore, filter: &LogAnchorFilter) -> Vec<LogAnchor> {
    let marker = normalize_marker(&filter.marker);
    let loop_id = normalize_required(&filter.loop_id);
    let source = normalize_required(&filter.log_source).to_ascii_lowercase();
    let text = normalize_required(&filter.text).to_ascii_lowercase();
    let tag = normalize_required(&filter.tag).to_ascii_lowercase();

    store
        .anchors
        .iter()
        .filter(|anchor| {
            if !marker.is_empty() && !anchor.marker.eq_ignore_ascii_case(&marker) {
                return false;
            }
            if !loop_id.is_empty() && !anchor.loop_id.eq_ignore_ascii_case(&loop_id) {
                return false;
            }
            if !source.is_empty() && !anchor.log_source.eq_ignore_ascii_case(&source) {
                return false;
            }
            if !tag.is_empty()
                && !anchor
                    .tags
                    .iter()
                    .any(|anchor_tag| anchor_tag.eq_ignore_ascii_case(&tag))
            {
                return false;
            }
            if text.is_empty() {
                return true;
            }

            let blob = format!(
                "{} {} {} {} {}",
                anchor.excerpt.to_ascii_lowercase(),
                anchor.annotation.to_ascii_lowercase(),
                anchor.marker.to_ascii_lowercase(),
                anchor.loop_id.to_ascii_lowercase(),
                anchor.tags.join(" ").to_ascii_lowercase(),
            );
            blob.contains(&text)
        })
        .cloned()
        .collect()
}

#[must_use]
pub fn resolve_anchor_target(
    store: &LogAnchorStore,
    marker_or_id: &str,
) -> Option<(String, usize)> {
    let query = normalize_required(marker_or_id);
    if query.is_empty() {
        return None;
    }
    if let Some(anchor) = store.get(&query) {
        return Some((anchor.log_source.clone(), anchor.line_index));
    }
    let marker = normalize_marker(&query);
    let anchor = store.get_by_marker(&marker)?;
    Some((anchor.log_source.clone(), anchor.line_index))
}

#[must_use]
pub fn export_anchor_bundle_json(store: &LogAnchorStore, filter: &LogAnchorFilter) -> String {
    let mut anchors = list_log_anchors(store, filter);
    sort_anchors_for_export(&mut anchors);

    let mut root = Map::new();
    root.insert(
        "schema_version".to_owned(),
        Value::from(LOG_ANCHOR_SCHEMA_VERSION),
    );
    root.insert(
        "anchors".to_owned(),
        Value::Array(
            anchors
                .iter()
                .map(|anchor| {
                    let mut item = Map::new();
                    item.insert(
                        "anchor_id".to_owned(),
                        Value::from(anchor.anchor_id.clone()),
                    );
                    item.insert("marker".to_owned(), Value::from(anchor.marker.clone()));
                    item.insert("loop_id".to_owned(), Value::from(anchor.loop_id.clone()));
                    item.insert(
                        "log_source".to_owned(),
                        Value::from(anchor.log_source.clone()),
                    );
                    item.insert("line_index".to_owned(), Value::from(anchor.line_index));
                    item.insert(
                        "timestamp".to_owned(),
                        Value::from(anchor.timestamp.clone()),
                    );
                    item.insert("excerpt".to_owned(), Value::from(anchor.excerpt.clone()));
                    item.insert(
                        "annotation".to_owned(),
                        Value::from(anchor.annotation.clone()),
                    );
                    item.insert(
                        "tags".to_owned(),
                        Value::Array(
                            anchor
                                .tags
                                .iter()
                                .map(|tag| Value::from(tag.clone()))
                                .collect(),
                        ),
                    );
                    item.insert(
                        "created_by".to_owned(),
                        Value::from(anchor.created_by.clone()),
                    );
                    item.insert(
                        "created_at".to_owned(),
                        Value::from(anchor.created_at.clone()),
                    );
                    Value::Object(item)
                })
                .collect(),
        ),
    );

    match serde_json::to_string_pretty(&Value::Object(root)) {
        Ok(json) => json,
        Err(_) => "{}".to_owned(),
    }
}

pub fn import_anchor_bundle_json(store: &mut LogAnchorStore, raw: &str) -> ImportAnchorsOutcome {
    let mut warnings = Vec::new();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ImportAnchorsOutcome {
            imported: 0,
            skipped: 0,
            warnings: vec!["empty anchor bundle".to_owned()],
        };
    }

    let parsed = serde_json::from_str::<Value>(trimmed);
    let value = match parsed {
        Ok(value) => value,
        Err(err) => {
            return ImportAnchorsOutcome {
                imported: 0,
                skipped: 0,
                warnings: vec![format!("invalid json anchor bundle ({err})")],
            };
        }
    };

    let Some(obj) = value.as_object() else {
        return ImportAnchorsOutcome {
            imported: 0,
            skipped: 0,
            warnings: vec!["anchor bundle must be a json object".to_owned()],
        };
    };

    let schema_version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(LOG_ANCHOR_SCHEMA_VERSION as u64) as u32;
    if schema_version != LOG_ANCHOR_SCHEMA_VERSION {
        warnings.push(format!(
            "unknown schema_version={schema_version}; attempted best-effort import"
        ));
    }

    let Some(items) = obj.get("anchors").and_then(Value::as_array) else {
        return ImportAnchorsOutcome {
            imported: 0,
            skipped: 0,
            warnings: vec!["anchor bundle missing anchors array".to_owned()],
        };
    };

    let mut imported = 0usize;
    let mut skipped = 0usize;

    for (index, item) in items.iter().enumerate() {
        let Some(anchor) = parse_import_anchor(item, index, &mut warnings) else {
            skipped += 1;
            continue;
        };

        if store.contains(&anchor.anchor_id) {
            skipped += 1;
            warnings.push(format!(
                "anchor '{}' already exists; skipped",
                anchor.anchor_id
            ));
            continue;
        }

        store.anchors.push(anchor);
        imported += 1;
    }

    sort_anchors(&mut store.anchors);
    ImportAnchorsOutcome {
        imported,
        skipped,
        warnings,
    }
}

#[must_use]
pub fn export_anchor_handoff_markdown(store: &LogAnchorStore, filter: &LogAnchorFilter) -> String {
    let anchors = list_log_anchors(store, filter);
    let mut lines = Vec::new();
    lines.push("# log anchors handoff".to_owned());
    lines.push(String::new());
    lines.push(format!("anchors: {}", anchors.len()));
    lines.push(String::new());

    for anchor in anchors {
        let timestamp = if anchor.timestamp.trim().is_empty() {
            "-".to_owned()
        } else {
            anchor.timestamp.trim().to_owned()
        };
        lines.push(format!(
            "- {} marker={} loop={} src={} line={} ts={}",
            anchor.anchor_id,
            anchor.marker,
            anchor.loop_id,
            anchor.log_source,
            anchor.line_index,
            timestamp
        ));
        lines.push(format!("  excerpt: {}", anchor.excerpt.trim()));
        if !anchor.annotation.trim().is_empty() {
            lines.push(format!("  note: {}", anchor.annotation.trim()));
        }
        if !anchor.tags.is_empty() {
            lines.push(format!("  tags: {}", anchor.tags.join(",")));
        }
    }

    lines.join("\n")
}

#[must_use]
pub fn render_anchor_rows(
    store: &LogAnchorStore,
    filter: &LogAnchorFilter,
    width: usize,
    height: usize,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let anchors = list_log_anchors(store, filter);
    let mut lines = Vec::new();
    lines.push(trim_to_width(
        &format!("log anchors rows={}", anchors.len()),
        width,
    ));
    if lines.len() >= height {
        return lines;
    }

    if anchors.is_empty() {
        lines.push(trim_to_width("no anchors", width));
        return lines;
    }

    for anchor in anchors {
        if lines.len() >= height {
            break;
        }
        let row = format!(
            "{} {} {}:{} {}",
            anchor.marker,
            anchor.loop_id,
            anchor.log_source,
            anchor.line_index,
            compact_text(&anchor.annotation, &anchor.excerpt)
        );
        lines.push(trim_to_width(&row, width));
    }
    lines
}

fn parse_import_anchor(
    value: &Value,
    index: usize,
    warnings: &mut Vec<String>,
) -> Option<LogAnchor> {
    let Some(obj) = value.as_object() else {
        warnings.push(format!("anchors[{index}] ignored (not object)"));
        return None;
    };

    let anchor_id = obj
        .get("anchor_id")
        .and_then(Value::as_str)
        .map(normalize_required)
        .unwrap_or_default();
    let marker = obj
        .get("marker")
        .and_then(Value::as_str)
        .map(normalize_marker)
        .unwrap_or_default();
    let loop_id = obj
        .get("loop_id")
        .and_then(Value::as_str)
        .map(normalize_required)
        .unwrap_or_default();
    let created_by = obj
        .get("created_by")
        .and_then(Value::as_str)
        .map(normalize_required)
        .unwrap_or_default();
    let created_at = obj
        .get("created_at")
        .and_then(Value::as_str)
        .map(normalize_required)
        .unwrap_or_default();
    let excerpt = obj
        .get("excerpt")
        .and_then(Value::as_str)
        .map(normalize_excerpt)
        .unwrap_or_default();

    if anchor_id.is_empty()
        || loop_id.is_empty()
        || created_by.is_empty()
        || created_at.is_empty()
        || excerpt.is_empty()
    {
        warnings.push(format!("anchors[{index}] missing required fields; skipped"));
        return None;
    }

    let line_index = obj.get("line_index").and_then(Value::as_u64).unwrap_or(0) as usize;

    let log_source = obj
        .get("log_source")
        .and_then(Value::as_str)
        .map(normalize_log_source)
        .unwrap_or_else(String::new);

    let tags = obj
        .get("tags")
        .and_then(Value::as_array)
        .map(|items| {
            normalize_tags(
                &items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();

    Some(LogAnchor {
        anchor_id,
        marker: if marker.is_empty() {
            format!("m{}", line_index)
        } else {
            marker
        },
        loop_id,
        log_source,
        line_index,
        timestamp: obj
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_owned(),
        excerpt,
        annotation: obj
            .get("annotation")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_owned(),
        tags,
        created_by,
        created_at,
    })
}

fn sort_anchors(anchors: &mut [LogAnchor]) {
    anchors.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.loop_id.cmp(&b.loop_id))
            .then_with(|| a.log_source.cmp(&b.log_source))
            .then_with(|| a.line_index.cmp(&b.line_index))
            .then_with(|| a.anchor_id.cmp(&b.anchor_id))
    });
}

fn sort_anchors_for_export(anchors: &mut [LogAnchor]) {
    anchors.sort_by(|a, b| {
        a.loop_id
            .cmp(&b.loop_id)
            .then_with(|| a.log_source.cmp(&b.log_source))
            .then_with(|| a.line_index.cmp(&b.line_index))
            .then_with(|| a.anchor_id.cmp(&b.anchor_id))
    });
}

fn next_anchor_id(base: &str, anchors: &[LogAnchor]) -> String {
    if anchors.iter().all(|anchor| anchor.anchor_id != base) {
        return base.to_owned();
    }

    for ordinal in 2..10_000 {
        let candidate = format!("{base}-{ordinal}");
        if anchors.iter().all(|anchor| anchor.anchor_id != candidate) {
            return candidate;
        }
    }

    format!("{base}-overflow")
}

fn next_marker(marker: &str, line_index: usize, anchors: &[LogAnchor]) -> String {
    let base = if marker.is_empty() {
        format!("m{line_index}")
    } else {
        marker.to_owned()
    };
    if anchors.iter().all(|anchor| anchor.marker != base) {
        return base;
    }
    for ordinal in 2..10_000 {
        let candidate = format!("{base}-{ordinal}");
        if anchors.iter().all(|anchor| anchor.marker != candidate) {
            return candidate;
        }
    }
    format!("{base}-overflow")
}

fn normalize_required(value: &str) -> String {
    value.trim().to_owned()
}

fn normalize_excerpt(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

fn normalize_log_source(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        "live".to_owned()
    } else {
        value
    }
}

fn normalize_marker(value: &str) -> String {
    let cleaned = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = cleaned
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    compact.trim().to_owned()
}

fn normalize_tags(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    for value in values {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }
        seen.insert(normalized);
    }
    seen.into_iter().collect()
}

fn slugify(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase().replace(' ', "-");
    let slug = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    if slug.is_empty() {
        "anchor".to_owned()
    } else {
        slug
    }
}

fn trim_to_width(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_owned()
    } else {
        text.chars().take(width).collect()
    }
}

fn compact_text(primary: &str, fallback: &str) -> String {
    let primary = primary.trim();
    if !primary.is_empty() {
        primary.to_owned()
    } else {
        fallback.trim().to_owned()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{
        add_log_anchor, annotate_log_anchor, export_anchor_bundle_json,
        export_anchor_handoff_markdown, import_anchor_bundle_json, list_log_anchors,
        remove_log_anchor, render_anchor_rows, resolve_anchor_target, LogAnchorDraft,
        LogAnchorFilter, LogAnchorStore,
    };

    fn draft(loop_id: &str, source: &str, line: usize, excerpt: &str) -> LogAnchorDraft {
        LogAnchorDraft {
            marker: format!("m{line}"),
            loop_id: loop_id.to_owned(),
            log_source: source.to_owned(),
            line_index: line,
            timestamp: "2026-02-12T12:00:00Z".to_owned(),
            excerpt: excerpt.to_owned(),
            annotation: String::new(),
            tags: vec!["handoff".to_owned()],
            created_by: "agent-a".to_owned(),
            created_at: "2026-02-12T12:00:00Z".to_owned(),
        }
    }

    #[test]
    fn add_annotation_and_remove_anchor_flow() {
        let mut store = LogAnchorStore::default();
        let anchor_id = add_log_anchor(&mut store, draft("loop-1", "live", 42, "tool failed"))
            .expect("add anchor");

        annotate_log_anchor(&mut store, &anchor_id, "retry after cache clear")
            .expect("annotate anchor");
        let anchor = store.get(&anchor_id).expect("anchor exists");
        assert_eq!(anchor.annotation, "retry after cache clear");
        assert_eq!(anchor.marker, "m42");

        remove_log_anchor(&mut store, &anchor_id).expect("remove anchor");
        assert!(store.get(&anchor_id).is_none());
    }

    #[test]
    fn duplicate_line_anchor_gets_unique_id_suffix() {
        let mut store = LogAnchorStore::default();
        let first = add_log_anchor(&mut store, draft("loop-1", "live", 42, "tool failed"))
            .expect("first anchor");
        let second = add_log_anchor(&mut store, draft("loop-1", "live", 42, "tool failed"))
            .expect("second anchor");

        assert_ne!(first, second);
        assert!(second.ends_with("-2"));
        let markers = store
            .anchors()
            .iter()
            .map(|anchor| anchor.marker.clone())
            .collect::<Vec<_>>();
        assert_eq!(markers, vec!["m42", "m42-2"]);
    }

    #[test]
    fn list_filters_by_loop_source_text_and_tag() {
        let mut store = LogAnchorStore::default();
        let mut first = draft("loop-1", "live", 7, "queue depth spike");
        first.annotation = "investigate scheduler".to_owned();
        first.tags = vec!["perf".to_owned(), "handoff".to_owned()];
        first.marker = "spike-1".to_owned();
        add_log_anchor(&mut store, first).expect("first anchor");

        let mut second = draft("loop-2", "latest-run", 3, "all green");
        second.tags = vec!["ok".to_owned()];
        add_log_anchor(&mut store, second).expect("second anchor");

        let filter = LogAnchorFilter {
            marker: "spike-1".to_owned(),
            loop_id: "loop-1".to_owned(),
            log_source: "live".to_owned(),
            text: "scheduler".to_owned(),
            tag: "perf".to_owned(),
        };
        let rows = list_log_anchors(&store, &filter);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].loop_id, "loop-1");
    }

    #[test]
    fn export_import_round_trip_preserves_anchor_fields() {
        let mut source = LogAnchorStore::default();
        let mut item = draft("loop-7", "live", 88, "panic: timeout");
        item.annotation = "root cause in adapter".to_owned();
        item.tags = vec!["p1".to_owned(), "handoff".to_owned()];
        add_log_anchor(&mut source, item).expect("add source anchor");

        let json = export_anchor_bundle_json(&source, &LogAnchorFilter::default());
        let mut target = LogAnchorStore::default();
        let outcome = import_anchor_bundle_json(&mut target, &json);

        assert_eq!(outcome.imported, 1);
        assert_eq!(outcome.skipped, 0);
        assert!(outcome.warnings.is_empty());
        assert_eq!(target.anchors().len(), 1);
        assert_eq!(target.anchors()[0].annotation, "root cause in adapter");
        assert_eq!(target.anchors()[0].marker, "m88");
    }

    #[test]
    fn import_skips_existing_anchor_and_reports_warning() {
        let mut store = LogAnchorStore::default();
        add_log_anchor(&mut store, draft("loop-1", "live", 1, "line 1")).expect("seed anchor");

        let json = export_anchor_bundle_json(&store, &LogAnchorFilter::default());
        let outcome = import_anchor_bundle_json(&mut store, &json);

        assert_eq!(outcome.imported, 0);
        assert_eq!(outcome.skipped, 1);
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("already exists")));
    }

    #[test]
    fn import_invalid_json_returns_warning_and_no_imports() {
        let mut store = LogAnchorStore::default();
        let outcome = import_anchor_bundle_json(&mut store, "{");

        assert_eq!(outcome.imported, 0);
        assert_eq!(outcome.skipped, 0);
        assert_eq!(store.anchors().len(), 0);
        assert!(!outcome.warnings.is_empty());
    }

    #[test]
    fn handoff_markdown_includes_anchor_metadata_and_note() {
        let mut store = LogAnchorStore::default();
        let mut item = draft("loop-9", "live", 101, "failed command");
        item.annotation = "repro in tmux pane 3".to_owned();
        item.tags = vec!["handoff".to_owned(), "p1".to_owned()];
        add_log_anchor(&mut store, item).expect("add anchor");

        let markdown = export_anchor_handoff_markdown(&store, &LogAnchorFilter::default());
        assert!(markdown.contains("# log anchors handoff"));
        assert!(markdown.contains("marker=m101"));
        assert!(markdown.contains("note: repro in tmux pane 3"));
        assert!(markdown.contains("tags: handoff,p1") || markdown.contains("tags: p1,handoff"));
    }

    #[test]
    fn render_anchor_rows_shows_compact_table_lines() {
        let mut store = LogAnchorStore::default();
        let mut item = draft("loop-3", "latest-run", 17, "build failed at step 4");
        item.annotation = "fix pending".to_owned();
        add_log_anchor(&mut store, item).expect("add anchor");

        let rows = render_anchor_rows(&store, &LogAnchorFilter::default(), 40, 4);
        assert_eq!(rows[0], "log anchors rows=1");
        assert!(rows[1].contains("m17 loop-3 latest-run:17"));
        assert!(rows[1].contains("fix pending"));
    }

    #[test]
    fn render_anchor_rows_truncates_unicode_without_panicking() {
        let mut store = LogAnchorStore::default();
        let mut item = draft("loop-3", "latest-run", 17, "⚠ résumé parsing failure");
        item.annotation = "⚡".to_owned();
        add_log_anchor(&mut store, item).expect("add anchor");

        let rows = render_anchor_rows(&store, &LogAnchorFilter::default(), 8, 4);
        for row in rows {
            assert!(row.chars().count() <= 8);
        }
    }

    #[test]
    fn resolve_target_accepts_marker_and_anchor_id() {
        let mut store = LogAnchorStore::default();
        let mut item = draft("loop-3", "latest-run", 17, "build failed");
        item.marker = "hotspot".to_owned();
        let anchor_id = add_log_anchor(&mut store, item).expect("add anchor");

        let by_marker = resolve_anchor_target(&store, "hotspot");
        assert_eq!(by_marker, Some(("latest-run".to_owned(), 17)));

        let by_id = resolve_anchor_target(&store, &anchor_id);
        assert_eq!(by_id, Some(("latest-run".to_owned(), 17)));
    }
}
