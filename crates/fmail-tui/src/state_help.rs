use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub message_id: String,
    pub target: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPreferences {
    pub live_tail_auto: bool,
    pub relative_time: bool,
    pub sound_alerts: bool,
    pub dashboard_views: Vec<String>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            live_tail_auto: true,
            relative_time: true,
            sound_alerts: false,
            dashboard_views: vec![
                "topics".to_owned(),
                "thread".to_owned(),
                "agents".to_owned(),
                "live-tail".to_owned(),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PersistedState {
    pub read_markers: BTreeMap<String, String>,
    pub bookmarks: Vec<Bookmark>,
    pub highlight_patterns: Vec<String>,
    pub keymap_overrides: BTreeMap<String, String>,
    pub preferences: UiPreferences,
}

impl PersistedState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn to_json_pretty(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|error| format!("serialize state: {error}"))
    }

    pub fn from_json(raw: &str) -> Result<Self, String> {
        serde_json::from_str(raw).map_err(|error| format!("parse state: {error}"))
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path).map_err(|error| format!("read state: {error}"))?;
        Self::from_json(&raw)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("create state dir: {error}"))?;
        }
        let temp = path.with_extension("json.tmp");
        let raw = self.to_json_pretty()?;
        fs::write(&temp, raw).map_err(|error| format!("write state temp: {error}"))?;
        fs::rename(&temp, path).map_err(|error| format!("rename state temp: {error}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
    pub key: &'static str,
    pub description: &'static str,
}

#[must_use]
pub fn default_keymap() -> Vec<KeyBinding> {
    vec![
        KeyBinding {
            key: "j/k",
            description: "move selection",
        },
        KeyBinding {
            key: "Tab",
            description: "switch section/view",
        },
        KeyBinding {
            key: "Enter",
            description: "open/read selected item",
        },
        KeyBinding {
            key: "Space",
            description: "pause/resume live tail or toggle rule",
        },
        KeyBinding {
            key: "/",
            description: "open search/filter",
        },
        KeyBinding {
            key: "c",
            description: "clear filters/state section",
        },
        KeyBinding {
            key: "?",
            description: "open help",
        },
        KeyBinding {
            key: "q",
            description: "quit",
        },
    ]
}

#[must_use]
pub fn render_help_frame(
    width: usize,
    height: usize,
    theme: ThemeSpec,
    active_view: &str,
    keymap: &[KeyBinding],
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    frame.draw_text(
        0,
        0,
        &truncate(&format!("HELP  view: {}", active_view.trim()), width),
        TextRole::Accent,
    );
    if height == 1 {
        return frame;
    }

    frame.draw_text(0, 1, "Keymap", TextRole::Muted);
    for (idx, binding) in keymap.iter().enumerate() {
        let row = idx + 2;
        if row >= height {
            break;
        }
        let line = format!("{:<8} {}", binding.key, binding.description);
        frame.draw_text(0, row, &truncate(&line, width), TextRole::Primary);
    }
    frame
}

fn truncate(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return input.to_owned();
    }
    if max_chars == 1 {
        return "…".to_owned();
    }
    let mut out = chars.into_iter().take(max_chars - 1).collect::<String>();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::{default_keymap, render_help_frame, Bookmark, PersistedState};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    #[test]
    fn state_round_trip_json() {
        let mut state = PersistedState::new();
        state
            .read_markers
            .insert("task".to_owned(), "20260209-120000-0001".to_owned());
        state.bookmarks.push(Bookmark {
            message_id: "20260209-120100-0001".to_owned(),
            target: "task".to_owned(),
            note: "follow up".to_owned(),
        });
        state.highlight_patterns = vec!["panic".to_owned(), "error".to_owned()];

        let json = state.to_json_pretty();
        assert!(json.is_ok());
        let reparsed = PersistedState::from_json(&json.unwrap_or_default());
        assert!(reparsed.is_ok());
        assert_eq!(reparsed.unwrap_or_default(), state);
    }

    #[test]
    fn save_and_load_state_file() {
        let unique = format!(
            "fmail_tui_state_help_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        );
        let root = std::env::temp_dir().join(unique);
        let created = std::fs::create_dir_all(&root);
        assert!(created.is_ok());
        let path = root.join(".fmail/tui-state.json");

        let mut state = PersistedState::new();
        state
            .keymap_overrides
            .insert("quit".to_owned(), "Ctrl+C".to_owned());
        let saved = state.save(&path);
        assert!(saved.is_ok());

        let loaded = PersistedState::load(&path);
        assert!(loaded.is_ok());
        assert_eq!(loaded.unwrap_or_default(), state);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn default_keymap_has_parity_bindings() {
        let keymap = default_keymap();
        assert!(keymap.iter().any(|binding| binding.key == "j/k"));
        assert!(keymap.iter().any(|binding| binding.key == "Tab"));
        assert!(keymap.iter().any(|binding| binding.key == "?"));
    }

    #[test]
    fn help_frame_snapshot() {
        let frame = render_help_frame(44, 7, ThemeSpec::default(), "timeline", &default_keymap());
        assert_render_frame_snapshot(
            "fmail_tui_help_frame",
            &frame,
            "HELP  view: timeline                        \nKeymap                                      \nj/k      move selection                     \nTab      switch section/view                \nEnter    open/read selected item            \nSpace    pause/resume live tail or toggle r…\n/        open search/filter                 ",
        );
    }
}
