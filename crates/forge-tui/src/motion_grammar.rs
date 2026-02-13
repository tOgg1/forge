//! Tasteful motion grammar for the FrankenTUI dashboard.
//!
//! Provides lightweight animation primitives that enhance spatial awareness and
//! polish without distracting from content. Three motion types:
//!
//! - **Enter transitions**: brief fade-in when switching tabs or opening modals.
//! - **Focus pulses**: subtle highlight flash when moving focus between panes.
//! - **Loading shimmer**: animated bar/dots during `action_busy` states.
//!
//! All motion respects a reduced-motion mode (env `FORGE_TUI_REDUCE_MOTION=1`
//! or programmatic toggle). When reduced, transitions complete instantly and
//! shimmer falls back to a static indicator.

use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Duration of a tab-enter fade-in transition.
const ENTER_TRANSITION_DURATION: Duration = Duration::from_millis(300);

/// Duration of a focus pulse highlight.
const FOCUS_PULSE_DURATION: Duration = Duration::from_millis(200);

/// Shimmer animation cycle length (one full sweep).
const SHIMMER_CYCLE: Duration = Duration::from_millis(1200);

/// Shimmer bar width in characters.
const SHIMMER_WIDTH: usize = 20;

// ---------------------------------------------------------------------------
// Reduced-motion detection
// ---------------------------------------------------------------------------

/// Check the `FORGE_TUI_REDUCE_MOTION` environment variable.
#[must_use]
pub fn env_prefers_reduced_motion() -> bool {
    std::env::var("FORGE_TUI_REDUCE_MOTION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Easing
// ---------------------------------------------------------------------------

/// Ease-out cubic: fast start, smooth deceleration.
fn ease_out_cubic(t: f64) -> f64 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}

// ---------------------------------------------------------------------------
// Motion state
// ---------------------------------------------------------------------------

/// Centralized motion state embedded in the App.
#[derive(Debug, Clone)]
pub struct MotionState {
    /// When the last tab switch happened.
    tab_entered_at: Option<Instant>,

    /// When focus last moved between panes.
    focus_pulsed_at: Option<Instant>,

    /// When action_busy was set (for shimmer phase).
    shimmer_started_at: Option<Instant>,

    /// Whether reduced-motion mode is active.
    pub reduced_motion: bool,
}

impl Default for MotionState {
    fn default() -> Self {
        Self {
            tab_entered_at: None,
            focus_pulsed_at: None,
            shimmer_started_at: None,
            reduced_motion: env_prefers_reduced_motion(),
        }
    }
}

impl MotionState {
    /// Create motion state with explicit reduced-motion setting.
    #[must_use]
    pub fn new(reduced_motion: bool) -> Self {
        Self {
            reduced_motion,
            ..Default::default()
        }
    }

    // -- triggers (called from App update logic) ----------------------------

    /// Signal that a tab switch just occurred.
    pub fn trigger_tab_enter(&mut self) {
        if !self.reduced_motion {
            self.tab_entered_at = Some(Instant::now());
        }
    }

    /// Signal that focus moved between panes.
    pub fn trigger_focus_pulse(&mut self) {
        if !self.reduced_motion {
            self.focus_pulsed_at = Some(Instant::now());
        }
    }

    /// Signal that a loading action started.
    pub fn trigger_shimmer_start(&mut self) {
        if !self.reduced_motion {
            self.shimmer_started_at = Some(Instant::now());
        }
    }

    /// Signal that the loading action ended.
    pub fn clear_shimmer(&mut self) {
        self.shimmer_started_at = None;
    }

    // -- queries (called from App render logic) -----------------------------

    /// Enter-transition progress: 0.0 (just started) to 1.0 (fully visible).
    /// Returns `None` when no transition is active.
    #[must_use]
    pub fn enter_progress(&self) -> Option<f64> {
        self.enter_progress_at(Instant::now())
    }

    /// Testable variant with explicit `now`.
    #[must_use]
    pub fn enter_progress_at(&self, now: Instant) -> Option<f64> {
        let started = self.tab_entered_at?;
        let elapsed = now.duration_since(started);
        if elapsed >= ENTER_TRANSITION_DURATION {
            return None; // transition complete
        }
        let t = elapsed.as_secs_f64() / ENTER_TRANSITION_DURATION.as_secs_f64();
        Some(ease_out_cubic(t))
    }

    /// Focus-pulse intensity: 1.0 (peak) fading to 0.0.
    /// Returns `None` when no pulse is active.
    #[must_use]
    pub fn focus_pulse_intensity(&self) -> Option<f64> {
        self.focus_pulse_intensity_at(Instant::now())
    }

    /// Testable variant with explicit `now`.
    #[must_use]
    pub fn focus_pulse_intensity_at(&self, now: Instant) -> Option<f64> {
        let started = self.focus_pulsed_at?;
        let elapsed = now.duration_since(started);
        if elapsed >= FOCUS_PULSE_DURATION {
            return None;
        }
        let t = elapsed.as_secs_f64() / FOCUS_PULSE_DURATION.as_secs_f64();
        Some(1.0 - ease_out_cubic(t))
    }

    /// Whether any motion is currently active (used to request re-render).
    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.is_animating_at(Instant::now())
    }

    /// Testable variant with explicit `now`.
    #[must_use]
    pub fn is_animating_at(&self, now: Instant) -> bool {
        self.enter_progress_at(now).is_some()
            || self.focus_pulse_intensity_at(now).is_some()
            || self.shimmer_started_at.is_some()
    }
}

// ---------------------------------------------------------------------------
// Render helpers
// ---------------------------------------------------------------------------

/// Minimum fraction of rows shown at the start of an enter transition.
/// A 0.6 floor means the transition reveals from 60% to 100% — a subtle
/// "expand" feel that doesn't hide too much content at any point.
const ENTER_MIN_FRACTION: f64 = 0.6;

/// Compute which content rows should be visible during an enter transition.
///
/// Returns the number of rows to show (from top). During the transition this
/// ramps from `ENTER_MIN_FRACTION * total_rows` to `total_rows` with an
/// ease-out curve. If no transition is active, returns `total_rows`.
#[must_use]
pub fn visible_rows_for_enter(motion: &MotionState, total_rows: usize) -> usize {
    match motion.enter_progress() {
        Some(progress) => {
            // Lerp from ENTER_MIN_FRACTION to 1.0
            let fraction = ENTER_MIN_FRACTION + progress * (1.0 - ENTER_MIN_FRACTION);
            let rows = (fraction * total_rows as f64).ceil() as usize;
            rows.clamp(1, total_rows)
        }
        None => total_rows,
    }
}

/// Render a shimmer bar string for the loading overlay.
///
/// Returns a string like `"⣾ loading: refreshing loop inventory ░░▒▓█▓▒░░"`
/// where the bright segment sweeps left-to-right. In reduced-motion mode,
/// returns a static `"◆"` indicator.
#[must_use]
pub fn shimmer_bar(motion: &MotionState, label: &str, width: usize) -> String {
    if motion.reduced_motion {
        return format!("\u{25C6} {label}");
    }

    let phase = match motion.shimmer_started_at {
        Some(started) => {
            let elapsed = Instant::now().duration_since(started);
            let cycle_pos = elapsed.as_secs_f64() / SHIMMER_CYCLE.as_secs_f64();
            cycle_pos % 1.0
        }
        None => 0.0,
    };

    shimmer_bar_at_phase(label, width, phase)
}

/// Testable shimmer bar with explicit phase (0.0..1.0).
#[must_use]
pub fn shimmer_bar_at_phase(label: &str, width: usize, phase: f64) -> String {
    let prefix = format!("\u{28FE} {label} ");
    let bar_width = width
        .saturating_sub(prefix.chars().count())
        .min(SHIMMER_WIDTH);
    if bar_width == 0 {
        return prefix;
    }

    // Shimmer uses unicode block elements to show a bright sweep.
    // ░ = dim, ▒ = medium, ▓ = bright, █ = peak
    const BLOCKS: [char; 4] = ['\u{2591}', '\u{2592}', '\u{2593}', '\u{2588}'];

    let center = (phase * bar_width as f64) as usize;
    let mut bar = String::with_capacity(bar_width);
    for i in 0..bar_width {
        let dist = (i as f64 - center as f64).abs();
        let idx = if dist < 1.0 {
            3 // peak
        } else if dist < 2.0 {
            2 // bright
        } else if dist < 3.0 {
            1 // medium
        } else {
            0 // dim
        };
        bar.push(BLOCKS[idx]);
    }

    format!("{prefix}{bar}")
}

/// Compute a focus-pulse indicator prefix for the active pane header.
///
/// Returns `"▸ "` during pulse, `"  "` otherwise.
#[must_use]
pub fn focus_pulse_prefix(motion: &MotionState) -> &'static str {
    match motion.focus_pulse_intensity() {
        Some(intensity) if intensity > 0.3 => "\u{25B8} ",
        _ => "  ",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn default_state_has_no_active_motion() {
        let state = MotionState::new(false);
        let now = Instant::now();
        assert!(!state.is_animating_at(now));
        assert!(state.enter_progress_at(now).is_none());
        assert!(state.focus_pulse_intensity_at(now).is_none());
    }

    #[test]
    fn reduced_motion_suppresses_triggers() {
        let mut state = MotionState::new(true);
        state.trigger_tab_enter();
        state.trigger_focus_pulse();
        state.trigger_shimmer_start();

        assert!(state.tab_entered_at.is_none());
        assert!(state.focus_pulsed_at.is_none());
        assert!(state.shimmer_started_at.is_none());
        assert!(!state.is_animating());
    }

    #[test]
    fn enter_transition_progress_ramps_up() {
        let mut state = MotionState::new(false);
        let start = Instant::now();
        state.tab_entered_at = Some(start);

        // At start: progress near 0
        let p = state.enter_progress_at(start).unwrap();
        assert!(p < 0.01, "expected near zero, got {p}");

        // Midway: progress between 0 and 1
        let mid = start + Duration::from_millis(150);
        let p = state.enter_progress_at(mid).unwrap();
        assert!(p > 0.3 && p < 1.0, "expected mid-range, got {p}");

        // After duration: None (complete)
        let after = start + Duration::from_millis(400);
        assert!(state.enter_progress_at(after).is_none());
    }

    #[test]
    fn focus_pulse_fades_out() {
        let mut state = MotionState::new(false);
        let start = Instant::now();
        state.focus_pulsed_at = Some(start);

        // At start: full intensity
        let i = state.focus_pulse_intensity_at(start).unwrap();
        assert!((i - 1.0).abs() < 0.01, "expected ~1.0, got {i}");

        // After duration: None
        let after = start + Duration::from_millis(300);
        assert!(state.focus_pulse_intensity_at(after).is_none());
    }

    #[test]
    fn shimmer_bar_static_in_reduced_motion() {
        let state = MotionState::new(true);
        let bar = shimmer_bar(&state, "loading", 40);
        assert_eq!(bar, "\u{25C6} loading");
    }

    #[test]
    fn shimmer_bar_at_phase_renders_sweep() {
        let bar = shimmer_bar_at_phase("loading", 40, 0.5);
        assert!(bar.contains('\u{2588}'), "expected peak block in shimmer");
        assert!(bar.contains('\u{2591}'), "expected dim block in shimmer");
    }

    #[test]
    fn shimmer_bar_at_phase_handles_narrow_width() {
        let bar = shimmer_bar_at_phase("loading: refreshing", 10, 0.5);
        // Should not panic; may just be prefix
        assert!(!bar.is_empty());
    }

    #[test]
    fn visible_rows_shows_all_when_no_transition() {
        let state = MotionState::new(false);
        assert_eq!(visible_rows_for_enter(&state, 30), 30);
    }

    #[test]
    fn visible_rows_ramps_during_transition() {
        let mut state = MotionState::new(false);
        state.tab_entered_at = Some(Instant::now());
        let rows = visible_rows_for_enter(&state, 30);
        // At t=0, should show at least 60% of rows (ENTER_MIN_FRACTION)
        assert!(rows >= 18, "expected at least 18 rows (60%), got {rows}");
        assert!(rows <= 30);
    }

    #[test]
    fn focus_pulse_prefix_shows_indicator() {
        let mut state = MotionState::new(false);
        let start = Instant::now();
        state.focus_pulsed_at = Some(start);
        // At start intensity is 1.0, should show indicator
        let prefix = focus_pulse_prefix(&state);
        assert_eq!(prefix, "\u{25B8} ");
    }

    #[test]
    fn focus_pulse_prefix_blank_when_idle() {
        let state = MotionState::new(false);
        assert_eq!(focus_pulse_prefix(&state), "  ");
    }

    #[test]
    fn ease_out_cubic_bounds() {
        assert!((ease_out_cubic(0.0) - 0.0).abs() < 1e-9);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < 1e-9);
        // Monotonically increasing
        let mid = ease_out_cubic(0.5);
        assert!(mid > 0.0 && mid < 1.0);
        assert!(mid > 0.5, "ease-out should be above linear at midpoint");
    }

    #[test]
    fn is_animating_reflects_active_transitions() {
        let mut state = MotionState::new(false);
        assert!(!state.is_animating());

        state.trigger_tab_enter();
        assert!(state.is_animating());

        // Shimmer also counts
        let mut state2 = MotionState::new(false);
        state2.trigger_shimmer_start();
        assert!(state2.is_animating());
    }

    #[test]
    fn clear_shimmer_stops_animation() {
        let mut state = MotionState::new(false);
        state.trigger_shimmer_start();
        assert!(state.shimmer_started_at.is_some());

        state.clear_shimmer();
        assert!(state.shimmer_started_at.is_none());
    }

    #[test]
    fn env_reduced_motion_defaults_to_false() {
        // When env var is not set (typical test env), should return false
        // We don't set/unset env vars in tests to avoid races
        let state = MotionState::default();
        // Just verify it doesn't panic; actual value depends on env
        let _ = state.reduced_motion;
    }
}
