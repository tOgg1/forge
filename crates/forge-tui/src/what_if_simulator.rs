//! What-if simulator for stop/scale actions against queue and throughput.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopThroughputSample {
    pub loop_id: String,
    pub avg_run_secs: u32,
    pub success_rate_percent: u8,
    pub queue_depth: u32,
    pub running_agents: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhatIfAction {
    StopLoop { loop_id: String },
    ResumeLoop { loop_id: String },
    ScaleLoop { loop_id: String, delta_agents: i16 },
    InjectQueue { loop_id: String, delta_items: i32 },
    SetSuccessRate { loop_id: String, percent: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopWhatIfProjection {
    pub loop_id: String,
    pub baseline_queue_depth: u32,
    pub projected_queue_depth: u32,
    pub baseline_throughput_milli_per_min: u64,
    pub projected_throughput_milli_per_min: u64,
    pub baseline_eta_secs: Option<u64>,
    pub projected_eta_secs: Option<u64>,
    pub impact_label: String,
}

#[must_use]
pub fn simulate_what_if(
    samples: &[LoopThroughputSample],
    actions: &[WhatIfAction],
) -> Vec<LoopWhatIfProjection> {
    let mut baseline: BTreeMap<String, LoopThroughputSample> = samples
        .iter()
        .map(|sample| (sample.loop_id.clone(), sample.clone()))
        .collect();

    for action in actions {
        match action {
            WhatIfAction::StopLoop { loop_id } => {
                if let Some(sample) = baseline.get_mut(loop_id) {
                    sample.running_agents = 0;
                }
            }
            WhatIfAction::ResumeLoop { loop_id } => {
                if let Some(sample) = baseline.get_mut(loop_id) {
                    if sample.running_agents == 0 {
                        sample.running_agents = 1;
                    }
                }
            }
            WhatIfAction::ScaleLoop {
                loop_id,
                delta_agents,
            } => {
                if let Some(sample) = baseline.get_mut(loop_id) {
                    let current = i32::from(sample.running_agents);
                    let next = (current + i32::from(*delta_agents)).max(0);
                    sample.running_agents = next as u16;
                }
            }
            WhatIfAction::InjectQueue {
                loop_id,
                delta_items,
            } => {
                if let Some(sample) = baseline.get_mut(loop_id) {
                    let current = i64::from(sample.queue_depth);
                    let next = (current + i64::from(*delta_items)).max(0) as u32;
                    sample.queue_depth = next;
                }
            }
            WhatIfAction::SetSuccessRate { loop_id, percent } => {
                if let Some(sample) = baseline.get_mut(loop_id) {
                    sample.success_rate_percent = percent.min(&100).to_owned();
                }
            }
        }
    }

    let mut projections = Vec::new();
    for source in samples {
        let projected = baseline
            .get(&source.loop_id)
            .cloned()
            .unwrap_or_else(|| source.clone());

        let baseline_tp = throughput_milli_per_min(source);
        let projected_tp = throughput_milli_per_min(&projected);
        let baseline_eta = eta_secs(source.queue_depth, baseline_tp);
        let projected_eta = eta_secs(projected.queue_depth, projected_tp);

        projections.push(LoopWhatIfProjection {
            loop_id: source.loop_id.clone(),
            baseline_queue_depth: source.queue_depth,
            projected_queue_depth: projected.queue_depth,
            baseline_throughput_milli_per_min: baseline_tp,
            projected_throughput_milli_per_min: projected_tp,
            baseline_eta_secs: baseline_eta,
            projected_eta_secs: projected_eta,
            impact_label: impact_label(baseline_eta, projected_eta),
        });
    }
    projections.sort_by(|a, b| a.loop_id.cmp(&b.loop_id));
    projections
}

#[must_use]
pub fn render_projection_rows(
    projections: &[LoopWhatIfProjection],
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }
    let mut rows = vec![trim_to_width(
        &format!("what-if projections: {}", projections.len()),
        width,
    )];
    if rows.len() >= max_rows {
        return rows;
    }
    if projections.is_empty() {
        rows.push(trim_to_width("no loops available", width));
        rows.truncate(max_rows);
        return rows;
    }
    for projection in projections {
        if rows.len() >= max_rows {
            break;
        }
        let row = format!(
            "{} q:{}->{}, eta:{}->{}, tp:{}->{}, impact:{}",
            projection.loop_id,
            projection.baseline_queue_depth,
            projection.projected_queue_depth,
            render_eta(projection.baseline_eta_secs),
            render_eta(projection.projected_eta_secs),
            projection.baseline_throughput_milli_per_min,
            projection.projected_throughput_milli_per_min,
            projection.impact_label
        );
        rows.push(trim_to_width(&row, width));
    }
    rows
}

fn throughput_milli_per_min(sample: &LoopThroughputSample) -> u64 {
    if sample.avg_run_secs == 0 || sample.running_agents == 0 || sample.success_rate_percent == 0 {
        return 0;
    }
    let running = u64::from(sample.running_agents);
    let success = u64::from(sample.success_rate_percent);
    let avg_secs = u64::from(sample.avg_run_secs);
    running.saturating_mul(success).saturating_mul(600) / avg_secs
}

fn eta_secs(queue_depth: u32, throughput_milli_per_min: u64) -> Option<u64> {
    if queue_depth == 0 {
        return Some(0);
    }
    if throughput_milli_per_min == 0 {
        return None;
    }
    let numerator = u64::from(queue_depth).saturating_mul(60_000);
    Some(numerator.div_ceil(throughput_milli_per_min))
}

fn impact_label(baseline: Option<u64>, projected: Option<u64>) -> String {
    match (baseline, projected) {
        (Some(_), None) => "blocked".to_owned(),
        (None, None) => "blocked".to_owned(),
        (None, Some(_)) => "recovered".to_owned(),
        (Some(base), Some(next)) if next < base => "improved".to_owned(),
        (Some(base), Some(next)) if next > base => "degraded".to_owned(),
        _ => "steady".to_owned(),
    }
}

fn render_eta(value: Option<u64>) -> String {
    value.map_or_else(|| "blocked".to_owned(), |seconds| format!("{seconds}s"))
}

fn trim_to_width(text: &str, width: usize) -> String {
    if text.len() <= width {
        text.to_owned()
    } else {
        text[0..width].to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{render_projection_rows, simulate_what_if, LoopThroughputSample, WhatIfAction};

    fn sample_loops() -> Vec<LoopThroughputSample> {
        vec![
            LoopThroughputSample {
                loop_id: "loop-a".to_owned(),
                avg_run_secs: 30,
                success_rate_percent: 90,
                queue_depth: 24,
                running_agents: 2,
            },
            LoopThroughputSample {
                loop_id: "loop-b".to_owned(),
                avg_run_secs: 45,
                success_rate_percent: 85,
                queue_depth: 18,
                running_agents: 1,
            },
        ]
    }

    #[test]
    fn stop_loop_projects_blocked_eta() {
        let projections = simulate_what_if(
            &sample_loops(),
            &[WhatIfAction::StopLoop {
                loop_id: "loop-a".to_owned(),
            }],
        );
        let loop_a = projections
            .iter()
            .find(|row| row.loop_id == "loop-a")
            .unwrap();
        assert!(loop_a.projected_eta_secs.is_none());
        assert_eq!(loop_a.impact_label, "blocked");
    }

    #[test]
    fn scaling_up_improves_eta() {
        let projections = simulate_what_if(
            &sample_loops(),
            &[WhatIfAction::ScaleLoop {
                loop_id: "loop-b".to_owned(),
                delta_agents: 2,
            }],
        );
        let loop_b = projections
            .iter()
            .find(|row| row.loop_id == "loop-b")
            .unwrap();
        assert!(
            loop_b.projected_throughput_milli_per_min > loop_b.baseline_throughput_milli_per_min
        );
        assert!(loop_b.projected_eta_secs.unwrap() < loop_b.baseline_eta_secs.unwrap());
        assert_eq!(loop_b.impact_label, "improved");
    }

    #[test]
    fn scaling_down_clamps_at_zero() {
        let projections = simulate_what_if(
            &sample_loops(),
            &[WhatIfAction::ScaleLoop {
                loop_id: "loop-b".to_owned(),
                delta_agents: -5,
            }],
        );
        let loop_b = projections
            .iter()
            .find(|row| row.loop_id == "loop-b")
            .unwrap();
        assert_eq!(loop_b.projected_throughput_milli_per_min, 0);
        assert_eq!(loop_b.projected_eta_secs, None);
    }

    #[test]
    fn projection_rows_snapshot() {
        let projections = simulate_what_if(
            &sample_loops(),
            &[
                WhatIfAction::ScaleLoop {
                    loop_id: "loop-a".to_owned(),
                    delta_agents: 1,
                },
                WhatIfAction::InjectQueue {
                    loop_id: "loop-b".to_owned(),
                    delta_items: 9,
                },
            ],
        );
        let rows = render_projection_rows(&projections, 200, 8);
        assert_eq!(
            rows,
            vec![
                "what-if projections: 2".to_owned(),
                "loop-a q:24->24, eta:7s->5s, tp:36->54, impact:improved".to_owned(),
                "loop-b q:18->27, eta:16s->24s, tp:11->11, impact:degraded".to_owned(),
            ]
        );
    }
}
