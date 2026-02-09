//! Loop TUI theme/palette helpers.
//!
//! Parity port of `internal/looptui/theme.go`.

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

pub const PALETTE_ORDER: [&str; 4] = ["default", "high-contrast", "ocean", "sunset"];

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

#[must_use]
pub fn resolve_palette(name: &str) -> Palette {
    let trimmed = name.trim().to_ascii_lowercase();
    match trimmed.as_str() {
        "default" => DEFAULT_PALETTE,
        "high-contrast" => HIGH_CONTRAST_PALETTE,
        "ocean" => OCEAN_PALETTE,
        "sunset" => SUNSET_PALETTE,
        _ => DEFAULT_PALETTE,
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

#[cfg(test)]
mod tests {
    use super::{cycle_palette, resolve_palette, DEFAULT_PALETTE, HIGH_CONTRAST_PALETTE};

    #[test]
    fn resolve_palette_defaults_to_default() {
        assert_eq!(resolve_palette("unknown"), DEFAULT_PALETTE);
        assert_eq!(resolve_palette("  DEFAULT "), DEFAULT_PALETTE);
    }

    #[test]
    fn resolve_palette_matches_named_palettes() {
        assert_eq!(resolve_palette("high-contrast"), HIGH_CONTRAST_PALETTE);
    }

    #[test]
    fn cycle_palette_wraps_and_normalizes() {
        assert_eq!(cycle_palette("default", 1).name, "high-contrast");
        assert_eq!(cycle_palette("default", -1).name, "sunset");
        assert_eq!(cycle_palette("  OCEAN ", 1).name, "sunset");
    }
}
