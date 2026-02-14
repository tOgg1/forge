//! Adaptive degradation policy tuner for constrained terminals and links.

use crate::theme::TerminalColorCapability;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradationPolicyMode {
    Off,
    Balanced,
    Aggressive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DegradationSignals {
    pub frame_time_p95_ms: u64,
    pub input_latency_p95_ms: u64,
    pub transport_staleness_s: u64,
    pub viewport_width: usize,
    pub viewport_height: usize,
    pub color_capability: TerminalColorCapability,
}

impl Default for DegradationSignals {
    fn default() -> Self {
        Self {
            frame_time_p95_ms: 16,
            input_latency_p95_ms: 20,
            transport_staleness_s: 0,
            viewport_width: 160,
            viewport_height: 48,
            color_capability: TerminalColorCapability::TrueColor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DegradationDecision {
    pub mode: DegradationPolicyMode,
    pub compact_density: bool,
    pub reduced_motion: bool,
    pub syntax_highlighting: bool,
    pub max_rendered_log_lines: usize,
    pub poll_interval_ms: u64,
    pub max_multi_layout: (i32, i32),
    pub reasons: Vec<String>,
}

#[must_use]
pub fn tune_degradation_policy(
    mode: DegradationPolicyMode,
    signals: &DegradationSignals,
) -> DegradationDecision {
    let mut score = 0u8;
    let mut reasons = Vec::new();

    match mode {
        DegradationPolicyMode::Off => {
            return DegradationDecision {
                mode,
                compact_density: false,
                reduced_motion: false,
                syntax_highlighting: true,
                max_rendered_log_lines: 400,
                poll_interval_ms: 1000,
                max_multi_layout: (4, 4),
                reasons,
            };
        }
        DegradationPolicyMode::Balanced => {
            if signals.frame_time_p95_ms > 28 {
                score += 2;
                reasons.push(format!("frame_p95={}ms", signals.frame_time_p95_ms));
            }
            if signals.input_latency_p95_ms > 120 {
                score += 1;
                reasons.push(format!("input_p95={}ms", signals.input_latency_p95_ms));
            }
            if signals.transport_staleness_s > 8 {
                score += 1;
                reasons.push(format!(
                    "transport_stale={}s",
                    signals.transport_staleness_s
                ));
            }
        }
        DegradationPolicyMode::Aggressive => {
            if signals.frame_time_p95_ms > 22 {
                score += 3;
                reasons.push(format!("frame_p95={}ms", signals.frame_time_p95_ms));
            }
            if signals.input_latency_p95_ms > 80 {
                score += 2;
                reasons.push(format!("input_p95={}ms", signals.input_latency_p95_ms));
            }
            if signals.transport_staleness_s > 4 {
                score += 2;
                reasons.push(format!(
                    "transport_stale={}s",
                    signals.transport_staleness_s
                ));
            }
        }
    }

    if signals.viewport_width < 100 || signals.viewport_height < 30 {
        score += 2;
        reasons.push(format!(
            "viewport={}x{}",
            signals.viewport_width, signals.viewport_height
        ));
    }
    if signals.color_capability == TerminalColorCapability::Ansi16 {
        score += 1;
        reasons.push("capability=ansi16".to_owned());
    }

    let compact_density = score >= 2;
    let reduced_motion = score >= 3;
    let syntax_highlighting = score < 5;
    let max_rendered_log_lines = if score >= 6 {
        120
    } else if score >= 4 {
        200
    } else if score >= 2 {
        300
    } else {
        400
    };
    let poll_interval_ms = if score >= 6 {
        2200
    } else if score >= 4 {
        1600
    } else if score >= 2 {
        1200
    } else {
        1000
    };
    let max_multi_layout = if score >= 6 {
        (2, 2)
    } else if score >= 3 {
        (3, 3)
    } else {
        (4, 4)
    };

    DegradationDecision {
        mode,
        compact_density,
        reduced_motion,
        syntax_highlighting,
        max_rendered_log_lines,
        poll_interval_ms,
        max_multi_layout,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::{tune_degradation_policy, DegradationPolicyMode, DegradationSignals};
    use crate::theme::TerminalColorCapability;

    #[test]
    fn off_mode_keeps_full_quality_defaults() {
        let decision =
            tune_degradation_policy(DegradationPolicyMode::Off, &DegradationSignals::default());
        assert!(!decision.compact_density);
        assert!(!decision.reduced_motion);
        assert!(decision.syntax_highlighting);
        assert_eq!(decision.max_multi_layout, (4, 4));
        assert_eq!(decision.poll_interval_ms, 1000);
    }

    #[test]
    fn balanced_mode_degrades_when_frame_and_viewport_are_bad() {
        let signals = DegradationSignals {
            frame_time_p95_ms: 40,
            input_latency_p95_ms: 30,
            transport_staleness_s: 0,
            viewport_width: 90,
            viewport_height: 26,
            color_capability: TerminalColorCapability::TrueColor,
        };
        let decision = tune_degradation_policy(DegradationPolicyMode::Balanced, &signals);
        assert!(decision.compact_density);
        assert!(decision.reduced_motion);
        assert_eq!(decision.max_multi_layout, (3, 3));
        assert!(decision
            .reasons
            .iter()
            .any(|reason| reason.contains("frame_p95")));
        assert!(decision
            .reasons
            .iter()
            .any(|reason| reason.contains("viewport=90x26")));
    }

    #[test]
    fn aggressive_mode_caps_features_under_high_stress() {
        let signals = DegradationSignals {
            frame_time_p95_ms: 55,
            input_latency_p95_ms: 220,
            transport_staleness_s: 12,
            viewport_width: 80,
            viewport_height: 24,
            color_capability: TerminalColorCapability::Ansi16,
        };
        let decision = tune_degradation_policy(DegradationPolicyMode::Aggressive, &signals);
        assert!(decision.compact_density);
        assert!(decision.reduced_motion);
        assert!(!decision.syntax_highlighting);
        assert_eq!(decision.max_multi_layout, (2, 2));
        assert_eq!(decision.max_rendered_log_lines, 120);
        assert!(decision.poll_interval_ms >= 1600);
        assert!(decision
            .reasons
            .iter()
            .any(|reason| reason.contains("capability=ansi16")));
    }
}
