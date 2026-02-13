//! Cost and resource tracker panel model.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendDirection {
    Up,
    Flat,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerAlertLevel {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerAlertKind {
    CostBurnRate,
    TokenSpike,
    ApiRateSpike,
    CpuPressure,
    MemoryPressure,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackerAlert {
    pub level: TrackerAlertLevel,
    pub kind: TrackerAlertKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResourceSample {
    pub timestamp_ms: i64,
    pub tokens_used: u64,
    pub api_calls: u64,
    pub compute_ms: u64,
    pub cost_usd: f64,
    pub cpu_percent: f64,
    pub memory_mb: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResourceBudgetPolicy {
    pub burn_rate_warn_usd_per_hour: f64,
    pub burn_rate_critical_usd_per_hour: f64,
    pub token_spike_factor: f64,
    pub api_spike_factor: f64,
    pub cpu_warn_percent: f64,
    pub cpu_critical_percent: f64,
    pub memory_warn_mb: f64,
    pub memory_critical_mb: f64,
    pub min_history_samples_for_spike_detection: usize,
}

impl Default for ResourceBudgetPolicy {
    fn default() -> Self {
        Self {
            burn_rate_warn_usd_per_hour: 25.0,
            burn_rate_critical_usd_per_hour: 60.0,
            token_spike_factor: 2.2,
            api_spike_factor: 2.0,
            cpu_warn_percent: 80.0,
            cpu_critical_percent: 92.0,
            memory_warn_mb: 6144.0,
            memory_critical_mb: 8192.0,
            min_history_samples_for_spike_detection: 6,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceTrackerSummary {
    pub samples: usize,
    pub total_tokens: u64,
    pub total_api_calls: u64,
    pub total_compute_ms: u64,
    pub total_cost_usd: f64,
    pub latest_tokens_per_min: f64,
    pub latest_api_calls_per_min: f64,
    pub latest_cost_per_hour_usd: f64,
    pub avg_cpu_percent: f64,
    pub avg_memory_mb: f64,
    pub cpu_trend: TrendDirection,
    pub memory_trend: TrendDirection,
    pub cost_trend: TrendDirection,
    pub token_sparkline: String,
    pub cost_sparkline: String,
    pub alerts: Vec<TrackerAlert>,
}

#[must_use]
pub fn build_resource_tracker_summary(
    samples: &[ResourceSample],
    policy: &ResourceBudgetPolicy,
) -> ResourceTrackerSummary {
    if samples.is_empty() {
        return ResourceTrackerSummary {
            samples: 0,
            total_tokens: 0,
            total_api_calls: 0,
            total_compute_ms: 0,
            total_cost_usd: 0.0,
            latest_tokens_per_min: 0.0,
            latest_api_calls_per_min: 0.0,
            latest_cost_per_hour_usd: 0.0,
            avg_cpu_percent: 0.0,
            avg_memory_mb: 0.0,
            cpu_trend: TrendDirection::Flat,
            memory_trend: TrendDirection::Flat,
            cost_trend: TrendDirection::Flat,
            token_sparkline: "-".to_owned(),
            cost_sparkline: "-".to_owned(),
            alerts: Vec::new(),
        };
    }

    let mut ordered = samples.to_vec();
    ordered.sort_by_key(|sample| sample.timestamp_ms);

    let total_tokens = ordered.iter().map(|sample| sample.tokens_used).sum::<u64>();
    let total_api_calls = ordered.iter().map(|sample| sample.api_calls).sum::<u64>();
    let total_compute_ms = ordered.iter().map(|sample| sample.compute_ms).sum::<u64>();
    let total_cost_usd = ordered.iter().map(|sample| sample.cost_usd).sum::<f64>();
    let avg_cpu_percent =
        ordered.iter().map(|sample| sample.cpu_percent).sum::<f64>() / ordered.len() as f64;
    let avg_memory_mb =
        ordered.iter().map(|sample| sample.memory_mb).sum::<f64>() / ordered.len() as f64;

    let latest = ordered.last().copied().unwrap_or(ResourceSample {
        timestamp_ms: 0,
        tokens_used: 0,
        api_calls: 0,
        compute_ms: 0,
        cost_usd: 0.0,
        cpu_percent: 0.0,
        memory_mb: 0.0,
    });
    let latest_interval_minutes = latest_interval_minutes(&ordered).max(1e-6);
    let latest_tokens_per_min = latest.tokens_used as f64 / latest_interval_minutes;
    let latest_api_calls_per_min = latest.api_calls as f64 / latest_interval_minutes;
    let latest_cost_per_hour_usd = latest.cost_usd * (60.0 / latest_interval_minutes);

    let cpu_trend = trend_of(
        &ordered
            .iter()
            .map(|sample| sample.cpu_percent)
            .collect::<Vec<_>>(),
    );
    let memory_trend = trend_of(
        &ordered
            .iter()
            .map(|sample| sample.memory_mb)
            .collect::<Vec<_>>(),
    );
    let cost_trend = trend_of(
        &ordered
            .iter()
            .map(|sample| sample.cost_usd)
            .collect::<Vec<_>>(),
    );

    let token_sparkline = sparkline_u64(
        &ordered
            .iter()
            .map(|sample| sample.tokens_used)
            .collect::<Vec<_>>(),
    );
    let cost_sparkline = sparkline_f64(
        &ordered
            .iter()
            .map(|sample| sample.cost_usd)
            .collect::<Vec<_>>(),
    );
    let alerts = detect_alerts(
        &ordered,
        latest_tokens_per_min,
        latest_api_calls_per_min,
        latest_cost_per_hour_usd,
        policy,
    );

    ResourceTrackerSummary {
        samples: ordered.len(),
        total_tokens,
        total_api_calls,
        total_compute_ms,
        total_cost_usd,
        latest_tokens_per_min,
        latest_api_calls_per_min,
        latest_cost_per_hour_usd,
        avg_cpu_percent,
        avg_memory_mb,
        cpu_trend,
        memory_trend,
        cost_trend,
        token_sparkline,
        cost_sparkline,
        alerts,
    }
}

#[must_use]
pub fn render_resource_tracker_panel_lines(
    summary: &ResourceTrackerSummary,
    width: usize,
) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut lines = vec![
        fit_width("COST/RESOURCE TRACKER", width),
        fit_width(
            &format!(
                "cost:${:.2}  burn:${:.2}/h  tokens:{}  api:{}",
                summary.total_cost_usd,
                summary.latest_cost_per_hour_usd,
                summary.total_tokens,
                summary.total_api_calls
            ),
            width,
        ),
        fit_width(
            &format!(
                "cpu:{:.1}%({})  mem:{:.0}MB({})  compute:{}ms",
                summary.avg_cpu_percent,
                trend_label(summary.cpu_trend),
                summary.avg_memory_mb,
                trend_label(summary.memory_trend),
                summary.total_compute_ms
            ),
            width,
        ),
        fit_width(
            &format!(
                "rate tok/min:{:.0} api/min:{:.1} cost-trend:{}",
                summary.latest_tokens_per_min,
                summary.latest_api_calls_per_min,
                trend_label(summary.cost_trend)
            ),
            width,
        ),
        fit_width(&format!("token {}", summary.token_sparkline), width),
        fit_width(&format!("cost  {}", summary.cost_sparkline), width),
    ];

    if summary.alerts.is_empty() {
        lines.push(fit_width("alerts: none", width));
    } else {
        let first = &summary.alerts[0];
        lines.push(fit_width(
            &format!(
                "alerts: {} {}",
                alert_level_label(first.level),
                first.message
            ),
            width,
        ));
        if summary.alerts.len() > 1 {
            lines.push(fit_width(
                &format!("alerts:+{}", summary.alerts.len() - 1),
                width,
            ));
        }
    }
    lines
}

fn detect_alerts(
    ordered: &[ResourceSample],
    latest_tokens_per_min: f64,
    latest_api_calls_per_min: f64,
    latest_cost_per_hour_usd: f64,
    policy: &ResourceBudgetPolicy,
) -> Vec<TrackerAlert> {
    let mut alerts = Vec::new();
    let latest = ordered.last().copied().unwrap_or(ResourceSample {
        timestamp_ms: 0,
        tokens_used: 0,
        api_calls: 0,
        compute_ms: 0,
        cost_usd: 0.0,
        cpu_percent: 0.0,
        memory_mb: 0.0,
    });

    if latest_cost_per_hour_usd >= policy.burn_rate_critical_usd_per_hour {
        alerts.push(TrackerAlert {
            level: TrackerAlertLevel::Critical,
            kind: TrackerAlertKind::CostBurnRate,
            message: format!("burn rate ${latest_cost_per_hour_usd:.2}/h exceeds critical"),
        });
    } else if latest_cost_per_hour_usd >= policy.burn_rate_warn_usd_per_hour {
        alerts.push(TrackerAlert {
            level: TrackerAlertLevel::Warning,
            kind: TrackerAlertKind::CostBurnRate,
            message: format!("burn rate ${latest_cost_per_hour_usd:.2}/h exceeds warning"),
        });
    }

    if latest.cpu_percent >= policy.cpu_critical_percent {
        alerts.push(TrackerAlert {
            level: TrackerAlertLevel::Critical,
            kind: TrackerAlertKind::CpuPressure,
            message: format!("cpu {:.1}% exceeds critical", latest.cpu_percent),
        });
    } else if latest.cpu_percent >= policy.cpu_warn_percent {
        alerts.push(TrackerAlert {
            level: TrackerAlertLevel::Warning,
            kind: TrackerAlertKind::CpuPressure,
            message: format!("cpu {:.1}% exceeds warning", latest.cpu_percent),
        });
    }

    if latest.memory_mb >= policy.memory_critical_mb {
        alerts.push(TrackerAlert {
            level: TrackerAlertLevel::Critical,
            kind: TrackerAlertKind::MemoryPressure,
            message: format!("memory {:.0}MB exceeds critical", latest.memory_mb),
        });
    } else if latest.memory_mb >= policy.memory_warn_mb {
        alerts.push(TrackerAlert {
            level: TrackerAlertLevel::Warning,
            kind: TrackerAlertKind::MemoryPressure,
            message: format!("memory {:.0}MB exceeds warning", latest.memory_mb),
        });
    }

    if ordered.len() >= policy.min_history_samples_for_spike_detection {
        let history = &ordered[..ordered.len() - 1];
        let history_interval_minutes = latest_interval_minutes(history).max(1e-6);
        let history_tokens_per_min = history
            .iter()
            .map(|sample| sample.tokens_used as f64 / history_interval_minutes)
            .sum::<f64>()
            / history.len() as f64;
        let history_api_per_min = history
            .iter()
            .map(|sample| sample.api_calls as f64 / history_interval_minutes)
            .sum::<f64>()
            / history.len() as f64;

        if history_tokens_per_min > 0.0
            && latest_tokens_per_min >= history_tokens_per_min * policy.token_spike_factor
        {
            alerts.push(TrackerAlert {
                level: TrackerAlertLevel::Warning,
                kind: TrackerAlertKind::TokenSpike,
                message: format!(
                    "token spike {:.0}/min vs baseline {:.0}/min",
                    latest_tokens_per_min, history_tokens_per_min
                ),
            });
        }
        if history_api_per_min > 0.0
            && latest_api_calls_per_min >= history_api_per_min * policy.api_spike_factor
        {
            alerts.push(TrackerAlert {
                level: TrackerAlertLevel::Warning,
                kind: TrackerAlertKind::ApiRateSpike,
                message: format!(
                    "api-rate spike {:.1}/min vs baseline {:.1}/min",
                    latest_api_calls_per_min, history_api_per_min
                ),
            });
        }
    }

    alerts.sort_by(|a, b| {
        alert_rank(b.level)
            .cmp(&alert_rank(a.level))
            .then_with(|| alert_kind_rank(a.kind).cmp(&alert_kind_rank(b.kind)))
    });
    alerts
}

fn latest_interval_minutes(samples: &[ResourceSample]) -> f64 {
    if samples.len() < 2 {
        return 1.0;
    }
    let mut deltas = Vec::new();
    for pair in samples.windows(2) {
        let delta_ms = pair[1].timestamp_ms.saturating_sub(pair[0].timestamp_ms);
        if delta_ms > 0 {
            deltas.push(delta_ms as f64 / 60_000.0);
        }
    }
    if deltas.is_empty() {
        1.0
    } else {
        deltas.iter().sum::<f64>() / deltas.len() as f64
    }
}

fn trend_of(values: &[f64]) -> TrendDirection {
    if values.len() < 2 {
        return TrendDirection::Flat;
    }
    let tail = values.len().min(6);
    let segment = &values[values.len() - tail..];
    let mid = segment.len() / 2;
    let left = segment[..mid].iter().sum::<f64>() / mid.max(1) as f64;
    let right = segment[mid..].iter().sum::<f64>() / (segment.len() - mid).max(1) as f64;
    let delta = right - left;
    let threshold = (left.abs() * 0.05).max(0.25);
    if delta > threshold {
        TrendDirection::Up
    } else if delta < -threshold {
        TrendDirection::Down
    } else {
        TrendDirection::Flat
    }
}

fn sparkline_u64(values: &[u64]) -> String {
    sparkline_f64(&values.iter().map(|value| *value as f64).collect::<Vec<_>>())
}

fn sparkline_f64(values: &[f64]) -> String {
    if values.is_empty() {
        return "-".to_owned();
    }
    let min = values
        .iter()
        .fold(f64::INFINITY, |acc, value| acc.min(*value));
    let max = values
        .iter()
        .fold(f64::NEG_INFINITY, |acc, value| acc.max(*value));
    if (max - min).abs() < f64::EPSILON {
        return "▁".repeat(values.len());
    }
    let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    values
        .iter()
        .map(|value| {
            let norm = ((*value - min) / (max - min)).clamp(0.0, 1.0);
            let idx = (norm * (blocks.len() - 1) as f64).round() as usize;
            blocks[idx.min(blocks.len() - 1)]
        })
        .collect()
}

fn alert_rank(level: TrackerAlertLevel) -> u8 {
    match level {
        TrackerAlertLevel::Critical => 3,
        TrackerAlertLevel::Warning => 2,
        TrackerAlertLevel::Info => 1,
    }
}

fn alert_kind_rank(kind: TrackerAlertKind) -> u8 {
    match kind {
        TrackerAlertKind::CostBurnRate => 1,
        TrackerAlertKind::CpuPressure => 2,
        TrackerAlertKind::MemoryPressure => 3,
        TrackerAlertKind::TokenSpike => 4,
        TrackerAlertKind::ApiRateSpike => 5,
    }
}

fn trend_label(trend: TrendDirection) -> &'static str {
    match trend {
        TrendDirection::Up => "up",
        TrendDirection::Flat => "flat",
        TrendDirection::Down => "down",
    }
}

fn alert_level_label(level: TrackerAlertLevel) -> &'static str {
    match level {
        TrackerAlertLevel::Info => "info",
        TrackerAlertLevel::Warning => "warn",
        TrackerAlertLevel::Critical => "critical",
    }
}

fn fit_width(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_resource_tracker_summary, render_resource_tracker_panel_lines, ResourceBudgetPolicy,
        ResourceSample, TrackerAlertKind, TrackerAlertLevel, TrendDirection,
    };

    fn sample_series() -> Vec<ResourceSample> {
        vec![
            ResourceSample {
                timestamp_ms: 1_000,
                tokens_used: 400,
                api_calls: 4,
                compute_ms: 10_000,
                cost_usd: 0.35,
                cpu_percent: 55.0,
                memory_mb: 2048.0,
            },
            ResourceSample {
                timestamp_ms: 61_000,
                tokens_used: 450,
                api_calls: 4,
                compute_ms: 11_000,
                cost_usd: 0.38,
                cpu_percent: 58.0,
                memory_mb: 2200.0,
            },
            ResourceSample {
                timestamp_ms: 121_000,
                tokens_used: 420,
                api_calls: 5,
                compute_ms: 10_500,
                cost_usd: 0.36,
                cpu_percent: 57.0,
                memory_mb: 2300.0,
            },
            ResourceSample {
                timestamp_ms: 181_000,
                tokens_used: 460,
                api_calls: 5,
                compute_ms: 11_500,
                cost_usd: 0.41,
                cpu_percent: 61.0,
                memory_mb: 2400.0,
            },
            ResourceSample {
                timestamp_ms: 241_000,
                tokens_used: 480,
                api_calls: 6,
                compute_ms: 12_000,
                cost_usd: 0.43,
                cpu_percent: 63.0,
                memory_mb: 2500.0,
            },
            ResourceSample {
                timestamp_ms: 301_000,
                tokens_used: 1_200,
                api_calls: 12,
                compute_ms: 15_000,
                cost_usd: 1.35,
                cpu_percent: 88.0,
                memory_mb: 7300.0,
            },
        ]
    }

    #[test]
    fn summary_aggregates_totals_rates_and_trends() {
        let summary =
            build_resource_tracker_summary(&sample_series(), &ResourceBudgetPolicy::default());
        assert_eq!(summary.samples, 6);
        assert!(summary.total_tokens > 3000);
        assert!(summary.total_cost_usd > 3.0);
        assert!(summary.latest_cost_per_hour_usd > 70.0);
        assert_eq!(summary.cpu_trend, TrendDirection::Up);
        assert_eq!(summary.memory_trend, TrendDirection::Up);
        assert_eq!(summary.cost_trend, TrendDirection::Up);
        assert!(!summary.token_sparkline.is_empty());
        assert!(!summary.cost_sparkline.is_empty());
    }

    #[test]
    fn summary_emits_burn_cpu_memory_and_spike_alerts() {
        let summary =
            build_resource_tracker_summary(&sample_series(), &ResourceBudgetPolicy::default());
        assert!(summary.alerts.iter().any(|alert| {
            alert.level == TrackerAlertLevel::Critical
                && alert.kind == TrackerAlertKind::CostBurnRate
        }));
        assert!(summary.alerts.iter().any(|alert| {
            alert.level == TrackerAlertLevel::Warning && alert.kind == TrackerAlertKind::TokenSpike
        }));
        assert!(summary.alerts.iter().any(|alert| {
            alert.level == TrackerAlertLevel::Warning
                && alert.kind == TrackerAlertKind::MemoryPressure
        }));
    }

    #[test]
    fn panel_lines_include_cost_rate_and_alert_summary() {
        let summary =
            build_resource_tracker_summary(&sample_series(), &ResourceBudgetPolicy::default());
        let lines = render_resource_tracker_panel_lines(&summary, 120);
        let joined = lines.join("\n");
        assert!(joined.contains("COST/RESOURCE TRACKER"));
        assert!(joined.contains("burn:$"));
        assert!(joined.contains("tok/min"));
        assert!(joined.contains("alerts:"));
    }

    #[test]
    fn empty_series_yields_zeroed_summary_without_alerts() {
        let summary = build_resource_tracker_summary(&[], &ResourceBudgetPolicy::default());
        assert_eq!(summary.samples, 0);
        assert_eq!(summary.total_cost_usd, 0.0);
        assert!(summary.alerts.is_empty());
        let lines = render_resource_tracker_panel_lines(&summary, 80);
        assert!(lines.iter().any(|line| line.contains("alerts: none")));
    }
}
