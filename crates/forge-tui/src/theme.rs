//! Loop TUI theme/palette helpers.
//!
//! Parity port of `internal/looptui/theme.go` with semantic theme-pack extensions.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

pub const THEME_PACK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub name: &'static str,
    pub background: &'static str,
    pub panel: &'static str,
    pub panel_alt: &'static str,
    pub text: &'static str,
    pub text_muted: &'static str,
    pub border: &'static str,
    pub accent: &'static str,
    pub focus: &'static str,
    pub success: &'static str,
    pub warning: &'static str,
    pub error: &'static str,
    pub info: &'static str,
}

pub const PALETTE_ORDER: [&str; 6] = [
    "default",
    "high-contrast",
    "low-light",
    "colorblind-safe",
    "ocean",
    "sunset",
];

pub const ACCESSIBILITY_PRESET_ORDER: [&str; 3] = ["high-contrast", "low-light", "colorblind-safe"];

pub const DEFAULT_PALETTE: Palette = Palette {
    name: "default",
    background: "#0B0F14",
    panel: "#121821",
    panel_alt: "#10161E",
    text: "#E6EDF3",
    text_muted: "#8B9AAE",
    border: "#223043",
    accent: "#5B8DEF",
    focus: "#7AA2F7",
    success: "#3FB950",
    warning: "#D29922",
    error: "#F85149",
    info: "#58A6FF",
};

const HIGH_CONTRAST_PALETTE: Palette = Palette {
    name: "high-contrast",
    background: "#000000",
    panel: "#0A0A0A",
    panel_alt: "#000000",
    text: "#FFFFFF",
    text_muted: "#C0C0C0",
    border: "#FFFFFF",
    accent: "#00A2FF",
    focus: "#FFD400",
    success: "#00FF5A",
    warning: "#FFB000",
    error: "#FF4040",
    info: "#66CCFF",
};

const LOW_LIGHT_PALETTE: Palette = Palette {
    name: "low-light",
    background: "#05070A",
    panel: "#0A1016",
    panel_alt: "#0E141B",
    text: "#E4E8EE",
    text_muted: "#9AA5B5",
    border: "#2A3644",
    accent: "#8AB4F8",
    focus: "#F4C95D",
    success: "#7BD88F",
    warning: "#F2C572",
    error: "#FF7A90",
    info: "#8AD4FF",
};

const COLORBLIND_SAFE_PALETTE: Palette = Palette {
    name: "colorblind-safe",
    background: "#0E1117",
    panel: "#151A24",
    panel_alt: "#1B2230",
    text: "#EAF0F6",
    text_muted: "#A7B3C6",
    border: "#324055",
    accent: "#4EA1FF",
    focus: "#FFD166",
    success: "#2EC4B6",
    warning: "#FFB347",
    error: "#FF6B6B",
    info: "#7CC8FF",
};

const OCEAN_PALETTE: Palette = Palette {
    name: "ocean",
    background: "#07121A",
    panel: "#0C1B27",
    panel_alt: "#102230",
    text: "#D8ECF7",
    text_muted: "#78A2B8",
    border: "#1E4A61",
    accent: "#3DD3FF",
    focus: "#71E0FF",
    success: "#55E39F",
    warning: "#FFC857",
    error: "#FF6B6B",
    info: "#4CC9F0",
};

const SUNSET_PALETTE: Palette = Palette {
    name: "sunset",
    background: "#140C10",
    panel: "#201218",
    panel_alt: "#28171F",
    text: "#F6E7E4",
    text_muted: "#C89A90",
    border: "#5D2E3F",
    accent: "#FF8C5A",
    focus: "#FFB077",
    success: "#7ED957",
    warning: "#FFD166",
    error: "#FF5D73",
    info: "#7FD1FF",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ThemeSemanticSlot {
    UiBackground,
    UiSurface,
    UiSurfaceAlt,
    UiTextPrimary,
    UiTextMuted,
    UiBorder,
    UiAccent,
    UiFocus,
    StatusSuccess,
    StatusWarning,
    StatusError,
    StatusInfo,
    TokenKeyword,
    TokenString,
    TokenNumber,
    TokenCommand,
    TokenPath,
}

impl ThemeSemanticSlot {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::UiBackground => "ui.background",
            Self::UiSurface => "ui.surface",
            Self::UiSurfaceAlt => "ui.surface-alt",
            Self::UiTextPrimary => "ui.text-primary",
            Self::UiTextMuted => "ui.text-muted",
            Self::UiBorder => "ui.border",
            Self::UiAccent => "ui.accent",
            Self::UiFocus => "ui.focus",
            Self::StatusSuccess => "status.success",
            Self::StatusWarning => "status.warning",
            Self::StatusError => "status.error",
            Self::StatusInfo => "status.info",
            Self::TokenKeyword => "token.keyword",
            Self::TokenString => "token.string",
            Self::TokenNumber => "token.number",
            Self::TokenCommand => "token.command",
            Self::TokenPath => "token.path",
        }
    }

    #[must_use]
    pub fn from_slug(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ui.background" => Some(Self::UiBackground),
            "ui.surface" => Some(Self::UiSurface),
            "ui.surface-alt" => Some(Self::UiSurfaceAlt),
            "ui.text-primary" => Some(Self::UiTextPrimary),
            "ui.text-muted" => Some(Self::UiTextMuted),
            "ui.border" => Some(Self::UiBorder),
            "ui.accent" => Some(Self::UiAccent),
            "ui.focus" => Some(Self::UiFocus),
            "status.success" => Some(Self::StatusSuccess),
            "status.warning" => Some(Self::StatusWarning),
            "status.error" => Some(Self::StatusError),
            "status.info" => Some(Self::StatusInfo),
            "token.keyword" => Some(Self::TokenKeyword),
            "token.string" => Some(Self::TokenString),
            "token.number" => Some(Self::TokenNumber),
            "token.command" => Some(Self::TokenCommand),
            "token.path" => Some(Self::TokenPath),
            _ => None,
        }
    }
}

pub const REQUIRED_SEMANTIC_SLOTS: [ThemeSemanticSlot; 17] = [
    ThemeSemanticSlot::UiBackground,
    ThemeSemanticSlot::UiSurface,
    ThemeSemanticSlot::UiSurfaceAlt,
    ThemeSemanticSlot::UiTextPrimary,
    ThemeSemanticSlot::UiTextMuted,
    ThemeSemanticSlot::UiBorder,
    ThemeSemanticSlot::UiAccent,
    ThemeSemanticSlot::UiFocus,
    ThemeSemanticSlot::StatusSuccess,
    ThemeSemanticSlot::StatusWarning,
    ThemeSemanticSlot::StatusError,
    ThemeSemanticSlot::StatusInfo,
    ThemeSemanticSlot::TokenKeyword,
    ThemeSemanticSlot::TokenString,
    ThemeSemanticSlot::TokenNumber,
    ThemeSemanticSlot::TokenCommand,
    ThemeSemanticSlot::TokenPath,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePack {
    pub id: String,
    pub title: String,
    pub palette_name: String,
    pub slots: BTreeMap<ThemeSemanticSlot, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemePackError {
    InvalidJson,
    InvalidSchemaVersion,
    InvalidId,
    MissingField(&'static str),
    InvalidField(&'static str),
    MissingSlot(String),
    UnknownSlot(String),
    InvalidColor { slot: String, color: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColorCapability {
    Ansi16,
    Ansi256,
    TrueColor,
}

impl TerminalColorCapability {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Ansi16 => "ansi16",
            Self::Ansi256 => "ansi256",
            Self::TrueColor => "truecolor",
        }
    }

    #[must_use]
    pub fn from_slug(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ansi16" | "16" => Some(Self::Ansi16),
            "ansi256" | "256" => Some(Self::Ansi256),
            "truecolor" | "24bit" => Some(Self::TrueColor),
            _ => None,
        }
    }

    #[must_use]
    pub fn adjusted_minimum_ratio(self, base_minimum: f64) -> f64 {
        match self {
            Self::TrueColor => base_minimum,
            Self::Ansi256 => (base_minimum - 0.2).max(3.0),
            Self::Ansi16 => (base_minimum - 0.7).max(2.5),
        }
    }
}

#[must_use]
pub fn detect_terminal_color_capability() -> TerminalColorCapability {
    if let Ok(override_value) = std::env::var("FORGE_TUI_COLOR_CAPABILITY") {
        if let Some(capability) = TerminalColorCapability::from_slug(&override_value) {
            return capability;
        }
    }

    let term = std::env::var("TERM").ok();
    let colorterm = std::env::var("COLORTERM").ok();
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let force_color = std::env::var("FORCE_COLOR")
        .ok()
        .and_then(|raw| parse_force_color_level(&raw))
        .or_else(|| {
            std::env::var("CLICOLOR_FORCE")
                .ok()
                .and_then(|raw| parse_force_color_level(&raw))
        });

    detect_terminal_color_capability_with(
        term.as_deref(),
        colorterm.as_deref(),
        no_color,
        force_color,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContrastViolation {
    pub theme_id: String,
    pub capability: TerminalColorCapability,
    pub foreground_slot: ThemeSemanticSlot,
    pub background_slot: ThemeSemanticSlot,
    pub ratio: f64,
    pub minimum_ratio: f64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContrastValidationReport {
    pub capability: TerminalColorCapability,
    pub themes_checked: usize,
    pub pairs_checked: usize,
    pub violations: Vec<ContrastViolation>,
}

#[must_use]
pub fn resolve_palette(name: &str) -> Palette {
    let trimmed = name.trim().to_ascii_lowercase();
    match trimmed.as_str() {
        "default" => DEFAULT_PALETTE,
        "high-contrast" => HIGH_CONTRAST_PALETTE,
        "low-light" => LOW_LIGHT_PALETTE,
        "colorblind-safe" => COLORBLIND_SAFE_PALETTE,
        "ocean" => OCEAN_PALETTE,
        "sunset" => SUNSET_PALETTE,
        _ => DEFAULT_PALETTE,
    }
}

#[must_use]
pub fn resolve_palette_for_capability(name: &str, capability: TerminalColorCapability) -> Palette {
    match capability {
        // ANSI16 often collapses subtle palettes into low-contrast pairs.
        // Force the high-contrast palette to preserve readability.
        TerminalColorCapability::Ansi16 => HIGH_CONTRAST_PALETTE,
        TerminalColorCapability::Ansi256 | TerminalColorCapability::TrueColor => {
            resolve_palette(name)
        }
    }
}

#[must_use]
pub fn cycle_palette(current: &str, delta: i32) -> Palette {
    if PALETTE_ORDER.is_empty() {
        return DEFAULT_PALETTE;
    }

    let current = current.trim().to_ascii_lowercase();
    let mut idx = 0i32;
    for (i, candidate) in PALETTE_ORDER.iter().enumerate() {
        if *candidate == current {
            idx = i as i32;
            break;
        }
    }

    idx += delta;
    while idx < 0 {
        idx += PALETTE_ORDER.len() as i32;
    }
    idx %= PALETTE_ORDER.len() as i32;
    resolve_palette(PALETTE_ORDER[idx as usize])
}

#[must_use]
pub fn cycle_accessibility_preset(current: &str, delta: i32) -> Palette {
    if ACCESSIBILITY_PRESET_ORDER.is_empty() {
        return HIGH_CONTRAST_PALETTE;
    }

    let current = current.trim().to_ascii_lowercase();
    let mut idx = -1i32;
    for (i, candidate) in ACCESSIBILITY_PRESET_ORDER.iter().enumerate() {
        if *candidate == current {
            idx = i as i32;
            break;
        }
    }

    if idx < 0 {
        return if delta < 0 {
            resolve_palette(ACCESSIBILITY_PRESET_ORDER[ACCESSIBILITY_PRESET_ORDER.len() - 1])
        } else {
            resolve_palette(ACCESSIBILITY_PRESET_ORDER[0])
        };
    }

    idx += delta;
    while idx < 0 {
        idx += ACCESSIBILITY_PRESET_ORDER.len() as i32;
    }
    idx %= ACCESSIBILITY_PRESET_ORDER.len() as i32;
    resolve_palette(ACCESSIBILITY_PRESET_ORDER[idx as usize])
}

#[must_use]
pub fn curated_theme_packs() -> Vec<ThemePack> {
    PALETTE_ORDER
        .iter()
        .map(|name| {
            let palette = resolve_palette(name);
            ThemePack {
                id: palette.name.to_owned(),
                title: theme_title(palette.name).to_owned(),
                palette_name: palette.name.to_owned(),
                slots: semantic_slots_for_palette(palette),
            }
        })
        .collect()
}

#[must_use]
pub fn resolve_theme_pack(id: &str) -> ThemePack {
    let lookup = normalize_id(id);
    curated_theme_packs()
        .into_iter()
        .find(|pack| pack.id == lookup)
        .unwrap_or_else(default_theme_pack)
}

#[must_use]
pub fn cycle_theme_pack(current: &str, delta: i32) -> ThemePack {
    let palette = cycle_palette(current, delta);
    resolve_theme_pack(palette.name)
}

#[must_use]
pub fn export_theme_pack(pack: &ThemePack) -> String {
    let mut root = Map::new();
    root.insert(
        "schema_version".to_owned(),
        Value::from(THEME_PACK_SCHEMA_VERSION),
    );
    root.insert("id".to_owned(), Value::from(pack.id.clone()));
    root.insert("title".to_owned(), Value::from(pack.title.clone()));
    root.insert(
        "palette_name".to_owned(),
        Value::from(pack.palette_name.clone()),
    );

    let mut slots = Map::new();
    for slot in REQUIRED_SEMANTIC_SLOTS {
        if let Some(color) = pack.slots.get(&slot) {
            slots.insert(slot.slug().to_owned(), Value::from(color.clone()));
        }
    }
    root.insert("slots".to_owned(), Value::Object(slots));

    serde_json::to_string_pretty(&Value::Object(root))
        .unwrap_or_else(|_| "{\"schema_version\":1}".to_owned())
}

pub fn import_theme_pack(raw: &str) -> Result<ThemePack, ThemePackError> {
    let parsed: Value = serde_json::from_str(raw).map_err(|_| ThemePackError::InvalidJson)?;
    let obj = parsed.as_object().ok_or(ThemePackError::InvalidJson)?;

    let schema_version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or(ThemePackError::MissingField("schema_version"))?;
    if schema_version != u64::from(THEME_PACK_SCHEMA_VERSION) {
        return Err(ThemePackError::InvalidSchemaVersion);
    }

    let id = normalize_id(
        obj.get("id")
            .and_then(Value::as_str)
            .ok_or(ThemePackError::MissingField("id"))?,
    );
    if id.is_empty() {
        return Err(ThemePackError::InvalidId);
    }

    let title = obj
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(ThemePackError::MissingField("title"))?
        .to_owned();

    let palette_name = normalize_id(
        obj.get("palette_name")
            .and_then(Value::as_str)
            .ok_or(ThemePackError::MissingField("palette_name"))?,
    );
    if palette_name.is_empty() {
        return Err(ThemePackError::InvalidField("palette_name"));
    }

    let slot_obj = obj
        .get("slots")
        .and_then(Value::as_object)
        .ok_or(ThemePackError::MissingField("slots"))?;

    let mut slots = BTreeMap::new();
    for (slot_name, color_value) in slot_obj {
        let slot = ThemeSemanticSlot::from_slug(slot_name)
            .ok_or_else(|| ThemePackError::UnknownSlot(slot_name.clone()))?;
        let color = color_value
            .as_str()
            .ok_or(ThemePackError::InvalidField("slots"))?
            .trim()
            .to_owned();
        if !is_hex_color(&color) {
            return Err(ThemePackError::InvalidColor {
                slot: slot_name.clone(),
                color,
            });
        }
        slots.insert(slot, color);
    }

    for required in REQUIRED_SEMANTIC_SLOTS {
        if !slots.contains_key(&required) {
            return Err(ThemePackError::MissingSlot(required.slug().to_owned()));
        }
    }

    Ok(ThemePack {
        id,
        title,
        palette_name,
        slots,
    })
}

#[must_use]
pub fn validate_curated_theme_contrast(
    capability: TerminalColorCapability,
) -> ContrastValidationReport {
    let packs = curated_theme_packs();
    validate_theme_packs_contrast(&packs, capability)
}

pub fn validate_curated_theme_contrast_fail_fast(
    capability: TerminalColorCapability,
) -> Result<ContrastValidationReport, ContrastViolation> {
    let packs = curated_theme_packs();
    validate_theme_packs_contrast_fail_fast(&packs, capability)
}

#[must_use]
pub fn validate_theme_packs_contrast(
    packs: &[ThemePack],
    capability: TerminalColorCapability,
) -> ContrastValidationReport {
    let mut violations = Vec::new();
    let mut pairs_checked = 0usize;
    for pack in packs {
        for (foreground_slot, background_slot, minimum_ratio, label) in contrast_requirements() {
            pairs_checked += 1;
            if let Some(violation) = evaluate_contrast_pair(
                pack,
                capability,
                foreground_slot,
                background_slot,
                minimum_ratio,
                label,
            ) {
                violations.push(violation);
            }
        }
    }

    ContrastValidationReport {
        capability,
        themes_checked: packs.len(),
        pairs_checked,
        violations,
    }
}

pub fn validate_theme_packs_contrast_fail_fast(
    packs: &[ThemePack],
    capability: TerminalColorCapability,
) -> Result<ContrastValidationReport, ContrastViolation> {
    let mut pairs_checked = 0usize;
    for pack in packs {
        for (foreground_slot, background_slot, minimum_ratio, label) in contrast_requirements() {
            pairs_checked += 1;
            if let Some(violation) = evaluate_contrast_pair(
                pack,
                capability,
                foreground_slot,
                background_slot,
                minimum_ratio,
                label,
            ) {
                return Err(violation);
            }
        }
    }

    Ok(ContrastValidationReport {
        capability,
        themes_checked: packs.len(),
        pairs_checked,
        violations: Vec::new(),
    })
}

fn theme_title(id: &str) -> &'static str {
    match id {
        "default" => "Default",
        "high-contrast" => "High Contrast",
        "low-light" => "Low Light",
        "colorblind-safe" => "Colorblind Safe",
        "ocean" => "Ocean",
        "sunset" => "Sunset",
        _ => "Custom",
    }
}

fn default_theme_pack() -> ThemePack {
    ThemePack {
        id: "default".to_owned(),
        title: "Default".to_owned(),
        palette_name: "default".to_owned(),
        slots: semantic_slots_for_palette(DEFAULT_PALETTE),
    }
}

fn semantic_slots_for_palette(palette: Palette) -> BTreeMap<ThemeSemanticSlot, String> {
    let mut slots = BTreeMap::new();
    slots.insert(
        ThemeSemanticSlot::UiBackground,
        palette.background.to_owned(),
    );
    slots.insert(ThemeSemanticSlot::UiSurface, palette.panel.to_owned());
    slots.insert(
        ThemeSemanticSlot::UiSurfaceAlt,
        palette.panel_alt.to_owned(),
    );
    slots.insert(ThemeSemanticSlot::UiTextPrimary, palette.text.to_owned());
    slots.insert(
        ThemeSemanticSlot::UiTextMuted,
        palette.text_muted.to_owned(),
    );
    slots.insert(ThemeSemanticSlot::UiBorder, palette.border.to_owned());
    slots.insert(ThemeSemanticSlot::UiAccent, palette.accent.to_owned());
    slots.insert(ThemeSemanticSlot::UiFocus, palette.focus.to_owned());
    slots.insert(ThemeSemanticSlot::StatusSuccess, palette.success.to_owned());
    slots.insert(ThemeSemanticSlot::StatusWarning, palette.warning.to_owned());
    slots.insert(ThemeSemanticSlot::StatusError, palette.error.to_owned());
    slots.insert(ThemeSemanticSlot::StatusInfo, palette.info.to_owned());
    slots.insert(ThemeSemanticSlot::TokenKeyword, palette.accent.to_owned());
    slots.insert(ThemeSemanticSlot::TokenString, palette.success.to_owned());
    slots.insert(ThemeSemanticSlot::TokenNumber, palette.warning.to_owned());
    slots.insert(ThemeSemanticSlot::TokenCommand, palette.focus.to_owned());
    slots.insert(ThemeSemanticSlot::TokenPath, palette.info.to_owned());
    slots
}

fn contrast_requirements() -> [(ThemeSemanticSlot, ThemeSemanticSlot, f64, &'static str); 8] {
    [
        (
            ThemeSemanticSlot::UiTextPrimary,
            ThemeSemanticSlot::UiBackground,
            4.5,
            "primary text",
        ),
        (
            ThemeSemanticSlot::UiTextMuted,
            ThemeSemanticSlot::UiBackground,
            3.2,
            "muted text",
        ),
        (
            ThemeSemanticSlot::UiAccent,
            ThemeSemanticSlot::UiBackground,
            3.0,
            "accent text",
        ),
        (
            ThemeSemanticSlot::StatusSuccess,
            ThemeSemanticSlot::UiBackground,
            3.0,
            "status success",
        ),
        (
            ThemeSemanticSlot::StatusWarning,
            ThemeSemanticSlot::UiBackground,
            3.0,
            "status warning",
        ),
        (
            ThemeSemanticSlot::StatusError,
            ThemeSemanticSlot::UiBackground,
            3.0,
            "status error",
        ),
        (
            ThemeSemanticSlot::StatusInfo,
            ThemeSemanticSlot::UiBackground,
            3.0,
            "status info",
        ),
        (
            ThemeSemanticSlot::TokenKeyword,
            ThemeSemanticSlot::UiBackground,
            3.0,
            "keyword token",
        ),
    ]
}

fn evaluate_contrast_pair(
    pack: &ThemePack,
    capability: TerminalColorCapability,
    foreground_slot: ThemeSemanticSlot,
    background_slot: ThemeSemanticSlot,
    minimum_ratio: f64,
    label: &str,
) -> Option<ContrastViolation> {
    let foreground = pack.slots.get(&foreground_slot)?;
    let background = pack.slots.get(&background_slot)?;
    let foreground_rgb = parse_hex_color(foreground)?;
    let background_rgb = parse_hex_color(background)?;
    let foreground_rgb = apply_terminal_capability(foreground_rgb, capability);
    let background_rgb = apply_terminal_capability(background_rgb, capability);
    let ratio = contrast_ratio(foreground_rgb, background_rgb);
    let minimum_ratio = capability.adjusted_minimum_ratio(minimum_ratio);
    if ratio + 1e-9 < minimum_ratio {
        return Some(ContrastViolation {
            theme_id: pack.id.clone(),
            capability,
            foreground_slot,
            background_slot,
            ratio,
            minimum_ratio,
            label: label.to_owned(),
        });
    }
    None
}

fn normalize_id(value: &str) -> String {
    let mut output = String::new();
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_' || ch.is_ascii_whitespace()) && !output.ends_with('-') {
            output.push('-');
        }
    }
    output.trim_matches('-').to_owned()
}

fn detect_terminal_color_capability_with(
    term: Option<&str>,
    colorterm: Option<&str>,
    no_color: bool,
    force_color: Option<u8>,
) -> TerminalColorCapability {
    if no_color {
        return TerminalColorCapability::Ansi16;
    }

    if let Some(level) = force_color {
        return match level {
            0 | 1 => TerminalColorCapability::Ansi16,
            2 => TerminalColorCapability::Ansi256,
            _ => TerminalColorCapability::TrueColor,
        };
    }

    if colorterm.is_some_and(|value| contains_ci(value, "truecolor"))
        || colorterm.is_some_and(|value| contains_ci(value, "24bit"))
    {
        return TerminalColorCapability::TrueColor;
    }

    if let Some(term) = term {
        if contains_ci(term, "truecolor")
            || contains_ci(term, "24bit")
            || contains_ci(term, "direct")
        {
            return TerminalColorCapability::TrueColor;
        }
        if contains_ci(term, "256color") {
            return TerminalColorCapability::Ansi256;
        }
        if term.trim().eq_ignore_ascii_case("dumb") {
            return TerminalColorCapability::Ansi16;
        }
    }

    TerminalColorCapability::Ansi256
}

fn parse_force_color_level(raw: &str) -> Option<u8> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "true" || normalized == "yes" {
        return Some(1);
    }
    if normalized == "false" || normalized == "no" {
        return Some(0);
    }
    normalized.parse::<u8>().ok()
}

fn contains_ci(value: &str, needle: &str) -> bool {
    value.to_ascii_lowercase().contains(needle)
}

fn is_hex_color(value: &str) -> bool {
    if value.len() != 7 || !value.starts_with('#') {
        return false;
    }
    value.chars().skip(1).all(|ch| ch.is_ascii_hexdigit())
}

fn parse_hex_color(value: &str) -> Option<(u8, u8, u8)> {
    if !is_hex_color(value) {
        return None;
    }
    let r = u8::from_str_radix(&value[1..3], 16).ok()?;
    let g = u8::from_str_radix(&value[3..5], 16).ok()?;
    let b = u8::from_str_radix(&value[5..7], 16).ok()?;
    Some((r, g, b))
}

fn apply_terminal_capability(
    rgb: (u8, u8, u8),
    capability: TerminalColorCapability,
) -> (u8, u8, u8) {
    match capability {
        TerminalColorCapability::TrueColor => rgb,
        TerminalColorCapability::Ansi256 => quantize_to_ansi256(rgb),
        TerminalColorCapability::Ansi16 => quantize_to_ansi16(rgb),
    }
}

fn quantize_to_ansi256((r, g, b): (u8, u8, u8)) -> (u8, u8, u8) {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    (
        nearest_level(r, &LEVELS),
        nearest_level(g, &LEVELS),
        nearest_level(b, &LEVELS),
    )
}

fn quantize_to_ansi16(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    const ANSI16_COLORS: [(u8, u8, u8); 16] = [
        (0x00, 0x00, 0x00),
        (0x80, 0x00, 0x00),
        (0x00, 0x80, 0x00),
        (0x80, 0x80, 0x00),
        (0x00, 0x00, 0x80),
        (0x80, 0x00, 0x80),
        (0x00, 0x80, 0x80),
        (0xc0, 0xc0, 0xc0),
        (0x80, 0x80, 0x80),
        (0xff, 0x00, 0x00),
        (0x00, 0xff, 0x00),
        (0xff, 0xff, 0x00),
        (0x00, 0x00, 0xff),
        (0xff, 0x00, 0xff),
        (0x00, 0xff, 0xff),
        (0xff, 0xff, 0xff),
    ];
    let mut best = ANSI16_COLORS[0];
    let mut best_distance = color_distance_sq(rgb, best);
    for color in ANSI16_COLORS.iter().copied().skip(1) {
        let distance = color_distance_sq(rgb, color);
        if distance < best_distance {
            best_distance = distance;
            best = color;
        }
    }
    best
}

fn nearest_level(value: u8, levels: &[u8]) -> u8 {
    let mut best = levels[0];
    let mut best_distance = u8::abs_diff(value, best);
    for level in levels.iter().copied().skip(1) {
        let distance = u8::abs_diff(value, level);
        if distance < best_distance {
            best_distance = distance;
            best = level;
        }
    }
    best
}

fn color_distance_sq((r1, g1, b1): (u8, u8, u8), (r2, g2, b2): (u8, u8, u8)) -> u32 {
    let dr = i32::from(r1) - i32::from(r2);
    let dg = i32::from(g1) - i32::from(g2);
    let db = i32::from(b1) - i32::from(b2);
    (dr * dr + dg * dg + db * db) as u32
}

fn contrast_ratio(foreground: (u8, u8, u8), background: (u8, u8, u8)) -> f64 {
    let l1 = relative_luminance(foreground);
    let l2 = relative_luminance(background);
    let (high, low) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (high + 0.05) / (low + 0.05)
}

fn relative_luminance((r, g, b): (u8, u8, u8)) -> f64 {
    fn channel(value: u8) -> f64 {
        let normalized = f64::from(value) / 255.0;
        if normalized <= 0.039_28 {
            normalized / 12.92
        } else {
            ((normalized + 0.055) / 1.055).powf(2.4)
        }
    }

    (0.2126 * channel(r)) + (0.7152 * channel(g)) + (0.0722 * channel(b))
}

#[cfg(test)]
mod tests {
    use super::{
        curated_theme_packs, cycle_accessibility_preset, cycle_palette, cycle_theme_pack,
        export_theme_pack, import_theme_pack, resolve_palette, resolve_palette_for_capability,
        resolve_theme_pack, validate_curated_theme_contrast,
        validate_curated_theme_contrast_fail_fast, validate_theme_packs_contrast,
        TerminalColorCapability, ThemePackError, ThemeSemanticSlot, COLORBLIND_SAFE_PALETTE,
        DEFAULT_PALETTE, HIGH_CONTRAST_PALETTE, LOW_LIGHT_PALETTE, REQUIRED_SEMANTIC_SLOTS,
    };

    #[test]
    fn resolve_palette_defaults_to_default() {
        assert_eq!(resolve_palette("unknown"), DEFAULT_PALETTE);
        assert_eq!(resolve_palette("  DEFAULT "), DEFAULT_PALETTE);
    }

    #[test]
    fn resolve_palette_matches_named_palettes() {
        assert_eq!(resolve_palette("high-contrast"), HIGH_CONTRAST_PALETTE);
        assert_eq!(resolve_palette("low-light"), LOW_LIGHT_PALETTE);
        assert_eq!(resolve_palette("colorblind-safe"), COLORBLIND_SAFE_PALETTE);
    }

    #[test]
    fn resolve_palette_for_capability_uses_high_contrast_for_ansi16() {
        let palette = resolve_palette_for_capability("sunset", TerminalColorCapability::Ansi16);
        assert_eq!(palette, HIGH_CONTRAST_PALETTE);
    }

    #[test]
    fn cycle_palette_wraps_and_normalizes() {
        assert_eq!(cycle_palette("default", 1).name, "high-contrast");
        assert_eq!(cycle_palette("default", 2).name, "low-light");
        assert_eq!(cycle_palette("default", -1).name, "sunset");
        assert_eq!(cycle_palette("  OCEAN ", 1).name, "sunset");
    }

    #[test]
    fn cycle_accessibility_preset_wraps() {
        assert_eq!(
            cycle_accessibility_preset("high-contrast", 1).name,
            "low-light"
        );
        assert_eq!(
            cycle_accessibility_preset("low-light", 1).name,
            "colorblind-safe"
        );
        assert_eq!(
            cycle_accessibility_preset("high-contrast", -1).name,
            "colorblind-safe"
        );
        assert_eq!(
            cycle_accessibility_preset("default", 1).name,
            "high-contrast"
        );
    }

    #[test]
    fn curated_theme_packs_include_required_semantic_slots() {
        let packs = curated_theme_packs();
        assert_eq!(packs.len(), 6);
        assert_eq!(packs[0].id, "default");

        for pack in packs {
            for slot in REQUIRED_SEMANTIC_SLOTS {
                let value = pack.slots.get(&slot);
                match value {
                    Some(color) => assert!(color.starts_with('#')),
                    None => panic!("missing slot {}", slot.slug()),
                }
            }
        }
    }

    #[test]
    fn export_import_theme_pack_round_trip() {
        let pack = resolve_theme_pack("colorblind-safe");
        let raw = export_theme_pack(&pack);
        let parsed = import_theme_pack(&raw);

        match parsed {
            Ok(parsed) => {
                assert_eq!(parsed.id, "colorblind-safe");
                assert_eq!(
                    parsed.slots.get(&ThemeSemanticSlot::StatusWarning),
                    Some(&"#FFB347".to_owned())
                );
                assert_eq!(
                    parsed.slots.get(&ThemeSemanticSlot::TokenKeyword),
                    parsed.slots.get(&ThemeSemanticSlot::UiAccent)
                );
            }
            Err(err) => panic!("roundtrip should parse: {err:?}"),
        }
    }

    #[test]
    fn import_rejects_missing_required_slot() {
        let pack = resolve_theme_pack("sunset");
        let mut raw = export_theme_pack(&pack);
        raw = raw.replace("\"token.path\": \"#7FD1FF\",\n", "");

        let parsed = import_theme_pack(&raw);
        match parsed {
            Err(ThemePackError::MissingSlot(slot)) => assert_eq!(slot, "token.path"),
            Err(err) => panic!("expected missing slot error, got {err:?}"),
            Ok(_) => panic!("expected import failure"),
        }
    }

    #[test]
    fn resolve_and_cycle_theme_pack_match_palette_order() {
        assert_eq!(resolve_theme_pack("not-real").id, "default");
        assert_eq!(cycle_theme_pack("default", 1).id, "high-contrast");
        assert_eq!(cycle_theme_pack("default", 2).id, "low-light");
        assert_eq!(cycle_theme_pack("default", -1).id, "sunset");
    }

    #[test]
    fn curated_theme_contrast_passes_all_capabilities() {
        for capability in [
            TerminalColorCapability::TrueColor,
            TerminalColorCapability::Ansi256,
            TerminalColorCapability::Ansi16,
        ] {
            let result = validate_curated_theme_contrast_fail_fast(capability);
            match result {
                Ok(report) => {
                    assert_eq!(report.themes_checked, 6);
                    assert_eq!(report.violations.len(), 0);
                    assert!(report.pairs_checked > 0);
                }
                Err(violation) => {
                    panic!("expected curated themes to pass contrast checks: {violation:?}")
                }
            }
        }
    }

    #[test]
    fn contrast_report_collects_violations_for_bad_pack() {
        let mut bad_pack = resolve_theme_pack("default");
        bad_pack.slots.insert(
            ThemeSemanticSlot::UiTextPrimary,
            bad_pack
                .slots
                .get(&ThemeSemanticSlot::UiBackground)
                .cloned()
                .unwrap_or_else(|| "#000000".to_owned()),
        );
        let packs = vec![bad_pack];
        let report = validate_theme_packs_contrast(&packs, TerminalColorCapability::TrueColor);

        assert_eq!(report.themes_checked, 1);
        assert!(!report.violations.is_empty());
        assert_eq!(
            report.violations[0].foreground_slot,
            ThemeSemanticSlot::UiTextPrimary
        );
        assert_eq!(
            report.violations[0].background_slot,
            ThemeSemanticSlot::UiBackground
        );
    }

    #[test]
    fn fail_fast_returns_first_violation() {
        let mut bad_pack = resolve_theme_pack("ocean");
        bad_pack.slots.insert(
            ThemeSemanticSlot::UiAccent,
            bad_pack
                .slots
                .get(&ThemeSemanticSlot::UiBackground)
                .cloned()
                .unwrap_or_else(|| "#000000".to_owned()),
        );
        let packs = vec![bad_pack];
        let result = validate_curated_theme_contrast(TerminalColorCapability::TrueColor);
        assert_eq!(result.capability.slug(), "truecolor");

        let fail_fast = super::validate_theme_packs_contrast_fail_fast(
            &packs,
            TerminalColorCapability::TrueColor,
        );
        match fail_fast {
            Err(violation) => {
                assert_eq!(violation.foreground_slot, ThemeSemanticSlot::UiAccent);
                assert_eq!(violation.background_slot, ThemeSemanticSlot::UiBackground);
            }
            Ok(_) => panic!("expected fail-fast violation"),
        }
    }

    #[test]
    fn detects_truecolor_from_colorterm() {
        let capability = super::detect_terminal_color_capability_with(
            Some("xterm-256color"),
            Some("truecolor"),
            false,
            None,
        );
        assert_eq!(capability, TerminalColorCapability::TrueColor);
    }

    #[test]
    fn detects_ansi256_from_term() {
        let capability = super::detect_terminal_color_capability_with(
            Some("screen-256color"),
            None,
            false,
            None,
        );
        assert_eq!(capability, TerminalColorCapability::Ansi256);
    }

    #[test]
    fn detects_ansi16_when_no_color_is_set() {
        let capability =
            super::detect_terminal_color_capability_with(Some("xterm-256color"), None, true, None);
        assert_eq!(capability, TerminalColorCapability::Ansi16);
    }

    #[test]
    fn force_color_level_overrides_detection() {
        let capability =
            super::detect_terminal_color_capability_with(Some("dumb"), None, false, Some(3));
        assert_eq!(capability, TerminalColorCapability::TrueColor);
    }
}
