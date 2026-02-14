//! Layout preset persistence and restore for multi-pane Forge TUI.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::layouts::{fit_pane_layout_for_breakpoint, PaneLayout, PANE_LAYOUTS};

pub const LAYOUT_PRESET_SCHEMA_VERSION: u32 = 2;

const BUILTIN_PRESET_IDS: [&str; 4] = ["ops", "dev", "review", "night"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutPreset {
    pub id: String,
    pub label: String,
    pub rows: i32,
    pub cols: i32,
    pub multi_page: usize,
    pub focus_right: bool,
    pub pinned_first: bool,
}

impl LayoutPreset {
    #[must_use]
    pub fn requested_layout(&self) -> PaneLayout {
        PaneLayout {
            rows: self.rows,
            cols: self.cols,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutPresetStore {
    pub schema_version: u32,
    pub active_preset_id: String,
    pub presets: Vec<LayoutPreset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutPresetLoadOutcome {
    pub store: LayoutPresetStore,
    pub migrated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedLayoutPreset {
    pub preset_id: String,
    pub label: String,
    pub requested_layout: PaneLayout,
    pub effective_layout: PaneLayout,
    pub multi_page: usize,
    pub focus_right: bool,
    pub pinned_first: bool,
}

impl Default for LayoutPresetStore {
    fn default() -> Self {
        Self {
            schema_version: LAYOUT_PRESET_SCHEMA_VERSION,
            active_preset_id: "ops".to_owned(),
            presets: builtin_presets(),
        }
    }
}

impl LayoutPresetStore {
    #[must_use]
    pub fn preset(&self, preset_id: &str) -> Option<&LayoutPreset> {
        let preset_id = normalize_id(preset_id);
        self.presets.iter().find(|preset| preset.id == preset_id)
    }

    pub fn upsert_preset(&mut self, preset: LayoutPreset) {
        let preset = normalize_preset(preset);
        if let Some(existing) = self.presets.iter_mut().find(|item| item.id == preset.id) {
            *existing = preset;
        } else {
            self.presets.push(preset);
        }
        *self = sanitize_store(self.clone(), &mut Vec::new());
    }
}

#[must_use]
pub fn restore_layout_preset_store(raw: &str) -> LayoutPresetLoadOutcome {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return LayoutPresetLoadOutcome {
            store: LayoutPresetStore::default(),
            migrated: false,
            warnings: Vec::new(),
        };
    }

    let parsed = serde_json::from_str::<Value>(trimmed);
    let value = match parsed {
        Ok(value) => value,
        Err(err) => {
            return LayoutPresetLoadOutcome {
                store: LayoutPresetStore::default(),
                migrated: false,
                warnings: vec![format!("invalid json; defaults restored ({err})")],
            };
        }
    };

    let mut warnings = Vec::new();
    let schema_version = value
        .as_object()
        .and_then(|obj| obj.get("schema_version"))
        .and_then(Value::as_u64)
        .unwrap_or(1) as u32;

    let (store, migrated) = if schema_version <= 1 {
        (
            parse_v1_store(&value, &mut warnings),
            schema_version != LAYOUT_PRESET_SCHEMA_VERSION,
        )
    } else if schema_version == LAYOUT_PRESET_SCHEMA_VERSION {
        (parse_v2_store(&value, &mut warnings), false)
    } else {
        warnings.push(format!(
            "unknown schema_version={schema_version}; parsed as v{}",
            LAYOUT_PRESET_SCHEMA_VERSION
        ));
        (
            parse_v2_store(&value, &mut warnings),
            schema_version != LAYOUT_PRESET_SCHEMA_VERSION,
        )
    };

    LayoutPresetLoadOutcome {
        store: sanitize_store(store, &mut warnings),
        migrated,
        warnings,
    }
}

#[must_use]
pub fn persist_layout_preset_store(store: &LayoutPresetStore) -> String {
    let normalized = sanitize_store(store.clone(), &mut Vec::new());
    let mut root = Map::new();
    root.insert(
        "schema_version".to_owned(),
        Value::from(normalized.schema_version),
    );
    root.insert(
        "active_preset_id".to_owned(),
        Value::from(normalized.active_preset_id),
    );
    root.insert(
        "presets".to_owned(),
        Value::Array(
            normalized
                .presets
                .iter()
                .map(|preset| {
                    let mut item = Map::new();
                    item.insert("id".to_owned(), Value::from(preset.id.clone()));
                    item.insert("label".to_owned(), Value::from(preset.label.clone()));
                    item.insert("rows".to_owned(), Value::from(preset.rows));
                    item.insert("cols".to_owned(), Value::from(preset.cols));
                    item.insert("multi_page".to_owned(), Value::from(preset.multi_page));
                    item.insert("focus_right".to_owned(), Value::from(preset.focus_right));
                    item.insert("pinned_first".to_owned(), Value::from(preset.pinned_first));
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

#[must_use]
pub fn apply_layout_preset(
    preset: &LayoutPreset,
    width: i32,
    height: i32,
    gap: i32,
    min_cell_width: i32,
    min_cell_height: i32,
) -> AppliedLayoutPreset {
    let requested_layout = preset.requested_layout();
    let effective_layout = fit_pane_layout_for_breakpoint(
        requested_layout,
        width,
        height,
        gap,
        min_cell_width,
        min_cell_height,
    );
    AppliedLayoutPreset {
        preset_id: preset.id.clone(),
        label: preset.label.clone(),
        requested_layout,
        effective_layout,
        multi_page: preset.multi_page,
        focus_right: preset.focus_right,
        pinned_first: preset.pinned_first,
    }
}

fn parse_v1_store(value: &Value, warnings: &mut Vec<String>) -> LayoutPresetStore {
    let Some(obj) = value.as_object() else {
        warnings.push("v1 layout preset state was not an object; defaults applied".to_owned());
        return LayoutPresetStore::default();
    };

    let active = obj
        .get("active")
        .or_else(|| obj.get("active_preset_id"))
        .and_then(Value::as_str)
        .map(normalize_id)
        .unwrap_or_else(|| "ops".to_owned());

    let presets = obj
        .get("presets")
        .and_then(Value::as_array)
        .map(|items| parse_v1_presets(items, warnings))
        .unwrap_or_else(Vec::new);

    LayoutPresetStore {
        schema_version: LAYOUT_PRESET_SCHEMA_VERSION,
        active_preset_id: active,
        presets,
    }
}

fn parse_v2_store(value: &Value, warnings: &mut Vec<String>) -> LayoutPresetStore {
    let Some(obj) = value.as_object() else {
        warnings.push("layout preset state was not an object; defaults applied".to_owned());
        return LayoutPresetStore::default();
    };

    let active_preset_id = obj
        .get("active_preset_id")
        .and_then(Value::as_str)
        .map(normalize_id)
        .unwrap_or_else(|| "ops".to_owned());

    let presets = obj
        .get("presets")
        .and_then(Value::as_array)
        .map(|items| parse_v2_presets(items, warnings))
        .unwrap_or_else(Vec::new);

    LayoutPresetStore {
        schema_version: LAYOUT_PRESET_SCHEMA_VERSION,
        active_preset_id,
        presets,
    }
}

fn parse_v1_presets(values: &[Value], warnings: &mut Vec<String>) -> Vec<LayoutPreset> {
    let mut presets = Vec::new();
    for value in values {
        let Some(obj) = value.as_object() else {
            warnings.push("ignored malformed v1 preset entry (not object)".to_owned());
            continue;
        };
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(normalize_id)
            .unwrap_or_default();
        if id.is_empty() {
            warnings.push("ignored v1 preset with empty id".to_owned());
            continue;
        }

        let mut rows = obj.get("rows").and_then(Value::as_i64).unwrap_or(1) as i32;
        let mut cols = obj.get("cols").and_then(Value::as_i64).unwrap_or(1) as i32;
        if let Some(grid) = obj.get("grid").and_then(Value::as_str) {
            if let Some((parsed_rows, parsed_cols)) = parse_grid_pair(grid) {
                rows = parsed_rows;
                cols = parsed_cols;
            } else {
                warnings.push(format!(
                    "preset {id}: invalid grid={grid:?}; fallback rows/cols"
                ));
            }
        }

        let label = obj
            .get("label")
            .or_else(|| obj.get("name"))
            .and_then(Value::as_str)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| title_case(&id));

        let preset = normalize_preset(LayoutPreset {
            id,
            label,
            rows,
            cols,
            multi_page: 0,
            focus_right: obj
                .get("focus_right")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            pinned_first: true,
        });
        presets.push(preset);
    }
    presets
}

fn parse_v2_presets(values: &[Value], warnings: &mut Vec<String>) -> Vec<LayoutPreset> {
    let mut presets = Vec::new();
    for value in values {
        let Some(obj) = value.as_object() else {
            warnings.push("ignored malformed preset entry (not object)".to_owned());
            continue;
        };
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(normalize_id)
            .unwrap_or_default();
        if id.is_empty() {
            warnings.push("ignored preset with empty id".to_owned());
            continue;
        }
        let rows = obj.get("rows").and_then(Value::as_i64).unwrap_or(1) as i32;
        let cols = obj.get("cols").and_then(Value::as_i64).unwrap_or(1) as i32;
        let label = obj
            .get("label")
            .and_then(Value::as_str)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| title_case(&id));
        let preset = normalize_preset(LayoutPreset {
            id,
            label,
            rows,
            cols,
            multi_page: obj.get("multi_page").and_then(Value::as_u64).unwrap_or(0) as usize,
            focus_right: obj
                .get("focus_right")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            pinned_first: obj
                .get("pinned_first")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        });
        presets.push(preset);
    }
    presets
}

fn sanitize_store(mut store: LayoutPresetStore, warnings: &mut Vec<String>) -> LayoutPresetStore {
    store.schema_version = LAYOUT_PRESET_SCHEMA_VERSION;
    let mut seen = BTreeSet::new();
    let mut presets = Vec::new();
    for preset in store.presets {
        let preset = normalize_preset(preset);
        if seen.contains(&preset.id) {
            warnings.push(format!("duplicate preset id={} ignored", preset.id));
            continue;
        }
        seen.insert(preset.id.clone());
        presets.push(preset);
    }

    for builtin in builtin_presets() {
        if !seen.contains(&builtin.id) {
            presets.push(builtin);
        }
    }

    presets.sort_by_key(preset_sort_key);
    store.presets = presets;

    let active_preset_id = normalize_id(&store.active_preset_id);
    if store
        .presets
        .iter()
        .any(|preset| preset.id == active_preset_id)
    {
        store.active_preset_id = active_preset_id;
    } else {
        store.active_preset_id = "ops".to_owned();
        warnings.push("active preset missing; fell back to ops".to_owned());
    }
    store
}

fn normalize_preset(mut preset: LayoutPreset) -> LayoutPreset {
    preset.id = normalize_id(&preset.id);
    if preset.id.is_empty() {
        preset.id = "custom".to_owned();
    }
    preset.label = preset.label.trim().to_owned();
    if preset.label.is_empty() {
        preset.label = title_case(&preset.id);
    }

    let normalized_layout = nearest_supported_layout(PaneLayout {
        rows: preset.rows,
        cols: preset.cols,
    });
    preset.rows = normalized_layout.rows;
    preset.cols = normalized_layout.cols;
    preset
}

fn nearest_supported_layout(requested: PaneLayout) -> PaneLayout {
    let mut best = PANE_LAYOUTS[0];
    let mut best_distance = i32::MAX;
    for candidate in PANE_LAYOUTS {
        let distance =
            (candidate.rows - requested.rows).abs() + (candidate.cols - requested.cols).abs();
        if distance < best_distance {
            best = candidate;
            best_distance = distance;
            continue;
        }
        if distance == best_distance && candidate.capacity() > best.capacity() {
            best = candidate;
        }
    }
    best
}

fn builtin_presets() -> Vec<LayoutPreset> {
    vec![
        LayoutPreset {
            id: "ops".to_owned(),
            label: "Ops".to_owned(),
            rows: 2,
            cols: 2,
            multi_page: 0,
            focus_right: false,
            pinned_first: true,
        },
        LayoutPreset {
            id: "dev".to_owned(),
            label: "Dev".to_owned(),
            rows: 1,
            cols: 3,
            multi_page: 0,
            focus_right: true,
            pinned_first: true,
        },
        LayoutPreset {
            id: "review".to_owned(),
            label: "Review".to_owned(),
            rows: 2,
            cols: 3,
            multi_page: 0,
            focus_right: false,
            pinned_first: true,
        },
        LayoutPreset {
            id: "night".to_owned(),
            label: "Night".to_owned(),
            rows: 4,
            cols: 4,
            multi_page: 0,
            focus_right: true,
            pinned_first: true,
        },
    ]
}

fn preset_sort_key(preset: &LayoutPreset) -> (usize, usize, String) {
    let builtin_rank = BUILTIN_PRESET_IDS
        .iter()
        .position(|id| *id == preset.id)
        .unwrap_or(usize::MAX);
    let builtin_flag = if builtin_rank == usize::MAX { 1 } else { 0 };
    (builtin_flag, builtin_rank, preset.id.clone())
}

fn parse_grid_pair(input: &str) -> Option<(i32, i32)> {
    let mut parts = input.trim().split('x');
    let rows = parts.next()?.trim().parse::<i32>().ok()?;
    let cols = parts.next()?.trim().parse::<i32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((rows, cols))
}

fn normalize_id(value: &str) -> String {
    let mut out = String::new();
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_' || ch.is_ascii_whitespace()) && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

fn title_case(value: &str) -> String {
    let mut out = Vec::new();
    for part in value.split('-') {
        if part.is_empty() {
            continue;
        }
        let mut chars = part.chars();
        let Some(first) = chars.next() else {
            continue;
        };
        let mut title = String::new();
        title.push(first.to_ascii_uppercase());
        title.extend(chars);
        out.push(title);
    }
    if out.is_empty() {
        "Preset".to_owned()
    } else {
        out.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_layout_preset, persist_layout_preset_store, restore_layout_preset_store,
        LayoutPreset, LayoutPresetStore, LAYOUT_PRESET_SCHEMA_VERSION,
    };

    #[test]
    fn default_store_contains_builtin_presets() {
        let store = LayoutPresetStore::default();
        let ids = store
            .presets
            .iter()
            .map(|preset| preset.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["ops", "dev", "review", "night"]);
        assert_eq!(store.active_preset_id, "ops");
    }

    #[test]
    fn restore_v1_migrates_to_latest_schema() {
        let raw = r#"
        {
          "schema_version": 1,
          "active": "review",
          "presets": [
            {"id": "review", "label": "Review", "grid": "2x3", "focus_right": true}
          ]
        }
        "#;
        let outcome = restore_layout_preset_store(raw);
        assert!(outcome.migrated);
        assert_eq!(outcome.store.schema_version, LAYOUT_PRESET_SCHEMA_VERSION);
        assert_eq!(outcome.store.active_preset_id, "review");
        let review = outcome.store.preset("review");
        match review {
            Some(preset) => {
                assert_eq!(preset.rows, 2);
                assert_eq!(preset.cols, 3);
                assert!(preset.focus_right);
                assert!(preset.pinned_first);
            }
            None => panic!("expected review preset"),
        }
    }

    #[test]
    fn restore_corrupted_json_falls_back_to_defaults() {
        let outcome = restore_layout_preset_store("{not-json");
        assert_eq!(outcome.store, LayoutPresetStore::default());
        assert!(!outcome.migrated);
        assert!(!outcome.warnings.is_empty());
    }

    #[test]
    fn restore_corrupted_fields_is_resilient() {
        let raw = r#"
        {
          "schema_version": 2,
          "active_preset_id": "ops",
          "presets": [
            {"id": "custom A", "label": "", "rows": 99, "cols": -2, "multi_page": 4},
            "bad-item",
            {"id": "ops", "rows": 2, "cols": 2}
          ]
        }
        "#;
        let outcome = restore_layout_preset_store(raw);
        assert!(!outcome.store.presets.is_empty());
        let custom = outcome.store.preset("custom-a");
        match custom {
            Some(preset) => {
                assert!(preset.rows >= 1);
                assert!(preset.cols >= 1);
                assert_eq!(preset.label, "Custom A");
            }
            None => panic!("expected custom-a preset"),
        }
        assert!(!outcome.warnings.is_empty());
    }

    #[test]
    fn persist_round_trip_keeps_active_and_custom() {
        let mut store = LayoutPresetStore {
            active_preset_id: "custom-one".to_owned(),
            ..LayoutPresetStore::default()
        };
        store.upsert_preset(LayoutPreset {
            id: "custom_one".to_owned(),
            label: "Custom One".to_owned(),
            rows: 2,
            cols: 4,
            multi_page: 2,
            focus_right: true,
            pinned_first: false,
        });

        let json = persist_layout_preset_store(&store);
        let reloaded = restore_layout_preset_store(&json);
        assert_eq!(reloaded.store.active_preset_id, "custom-one");
        let custom = reloaded.store.preset("custom-one");
        match custom {
            Some(preset) => {
                assert_eq!(preset.multi_page, 2);
                assert!(preset.focus_right);
                assert!(!preset.pinned_first);
            }
            None => panic!("expected custom preset"),
        }
    }

    #[test]
    fn apply_layout_preset_uses_fit_layout_for_small_viewport() {
        let preset = LayoutPreset {
            id: "night".to_owned(),
            label: "Night".to_owned(),
            rows: 4,
            cols: 4,
            multi_page: 3,
            focus_right: true,
            pinned_first: true,
        };
        let applied = apply_layout_preset(&preset, 120, 30, 2, 44, 10);
        assert_eq!(applied.requested_layout.rows, 4);
        assert_eq!(applied.requested_layout.cols, 4);
        assert!(applied.effective_layout.rows <= applied.requested_layout.rows);
        assert!(applied.effective_layout.cols <= applied.requested_layout.cols);
    }

    #[test]
    fn missing_active_preset_falls_back_to_ops() {
        let raw = r#"
        {
          "schema_version": 2,
          "active_preset_id": "ghost",
          "presets": [{"id":"custom","label":"Custom","rows":2,"cols":2}]
        }
        "#;
        let outcome = restore_layout_preset_store(raw);
        assert_eq!(outcome.store.active_preset_id, "ops");
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("active preset missing")));
    }
}
