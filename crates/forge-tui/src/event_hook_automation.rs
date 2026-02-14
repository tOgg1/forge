//! Event hook automation planner.
//!
//! Consumes triggered alert-rule events and decides which configured hooks
//! should run now, with cooldown + hourly rate-limit guardrails.

use crate::alert_rule_dsl::{RuleAlertSeverity, TriggeredRuleAlert};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventHookConfig {
    pub hook_id: String,
    pub rule_ids: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
    pub enabled: bool,
    pub cooldown_s: i64,
    pub max_runs_per_hour: u32,
}

impl Default for EventHookConfig {
    fn default() -> Self {
        Self {
            hook_id: String::new(),
            rule_ids: Vec::new(),
            command: String::new(),
            args: Vec::new(),
            enabled: true,
            cooldown_s: 30,
            max_runs_per_hour: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookExecutionRecord {
    pub hook_id: String,
    pub rule_id: String,
    pub started_at_epoch_s: i64,
    pub success: bool,
    pub output_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HookAutomationState {
    pub history: Vec<HookExecutionRecord>,
    pub history_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedHookRun {
    pub hook_id: String,
    pub rule_id: String,
    pub severity: RuleAlertSeverity,
    pub command: String,
    pub args: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedHookRun {
    pub hook_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HookExecutionPlan {
    pub scheduled: Vec<PlannedHookRun>,
    pub skipped: Vec<SkippedHookRun>,
}

#[must_use]
pub fn plan_event_hook_runs(
    configs: &[EventHookConfig],
    triggered_alerts: &[TriggeredRuleAlert],
    now_epoch_s: i64,
    state: &HookAutomationState,
) -> HookExecutionPlan {
    let mut plan = HookExecutionPlan::default();
    let now_epoch_s = now_epoch_s.max(0);

    for config in configs {
        let hook_id = normalize_id(&config.hook_id);
        if hook_id.is_empty() {
            plan.skipped.push(SkippedHookRun {
                hook_id: "<invalid>".to_owned(),
                reason: "hook_id missing".to_owned(),
            });
            continue;
        }

        let command = config.command.trim();
        if !config.enabled {
            plan.skipped.push(SkippedHookRun {
                hook_id,
                reason: "disabled".to_owned(),
            });
            continue;
        }
        if command.is_empty() {
            plan.skipped.push(SkippedHookRun {
                hook_id,
                reason: "command missing".to_owned(),
            });
            continue;
        }

        let matching_alert = pick_matching_alert(config, triggered_alerts);
        let Some(alert) = matching_alert else {
            continue;
        };

        let cooldown_s = config.cooldown_s.max(0);
        if cooldown_s > 0 {
            if let Some(last_run_at) = last_run_epoch_s(state, &hook_id) {
                let elapsed = now_epoch_s.saturating_sub(last_run_at);
                if elapsed < cooldown_s {
                    plan.skipped.push(SkippedHookRun {
                        hook_id,
                        reason: format!("cooldown {}s remaining", cooldown_s - elapsed),
                    });
                    continue;
                }
            }
        }

        let max_runs = config.max_runs_per_hour;
        if max_runs > 0 {
            let runs_last_hour = run_count_since(state, &hook_id, now_epoch_s.saturating_sub(3600));
            if runs_last_hour >= max_runs as usize {
                plan.skipped.push(SkippedHookRun {
                    hook_id,
                    reason: format!("hourly limit reached ({max_runs})"),
                });
                continue;
            }
        }

        plan.scheduled.push(PlannedHookRun {
            hook_id,
            rule_id: alert.rule_id.clone(),
            severity: alert.severity,
            command: command.to_owned(),
            args: config.args.clone(),
            reason: format!(
                "matched rule {} ({})",
                alert.rule_id,
                alert.severity.label()
            ),
        });
    }

    plan.scheduled.sort_by(|a, b| {
        severity_rank(b.severity)
            .cmp(&severity_rank(a.severity))
            .then(a.hook_id.cmp(&b.hook_id))
    });

    plan
}

pub fn record_hook_execution(
    state: &mut HookAutomationState,
    run: &PlannedHookRun,
    started_at_epoch_s: i64,
    success: bool,
    output_summary: &str,
) {
    let limit = if state.history_limit == 0 {
        256
    } else {
        state.history_limit
    };
    state.history.push(HookExecutionRecord {
        hook_id: run.hook_id.clone(),
        rule_id: run.rule_id.clone(),
        started_at_epoch_s: started_at_epoch_s.max(0),
        success,
        output_summary: output_summary.trim().to_owned(),
    });

    if state.history.len() > limit {
        let drop_count = state.history.len() - limit;
        state.history.drain(0..drop_count);
    }
}

#[must_use]
pub fn render_event_hook_panel_lines(plan: &HookExecutionPlan, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }

    let mut lines = vec![
        fit_width("EVENT HOOK AUTOMATION", width),
        fit_width(
            &format!(
                "scheduled:{}  skipped:{}",
                plan.scheduled.len(),
                plan.skipped.len()
            ),
            width,
        ),
    ];
    if plan.scheduled.is_empty() {
        lines.push(fit_width("no hooks scheduled for current events", width));
    } else {
        for run in plan.scheduled.iter().take(4) {
            lines.push(fit_width(
                &format!(
                    "{} <- {} [{}]",
                    run.hook_id,
                    run.rule_id,
                    run.severity.label()
                ),
                width,
            ));
        }
    }
    lines
}

fn pick_matching_alert<'a>(
    config: &EventHookConfig,
    alerts: &'a [TriggeredRuleAlert],
) -> Option<&'a TriggeredRuleAlert> {
    let configured_rule_ids = config
        .rule_ids
        .iter()
        .map(|rule_id| normalize_id(rule_id))
        .filter(|rule_id| !rule_id.is_empty())
        .collect::<Vec<_>>();
    if configured_rule_ids.is_empty() {
        return None;
    }

    alerts
        .iter()
        .filter(|alert| configured_rule_ids.contains(&normalize_id(&alert.rule_id)))
        .max_by(|a, b| {
            severity_rank(a.severity)
                .cmp(&severity_rank(b.severity))
                .then(a.rule_id.cmp(&b.rule_id))
        })
}

fn last_run_epoch_s(state: &HookAutomationState, hook_id: &str) -> Option<i64> {
    state
        .history
        .iter()
        .filter(|record| normalize_id(&record.hook_id) == hook_id)
        .map(|record| record.started_at_epoch_s)
        .max()
}

fn run_count_since(state: &HookAutomationState, hook_id: &str, since_epoch_s: i64) -> usize {
    state
        .history
        .iter()
        .filter(|record| {
            normalize_id(&record.hook_id) == hook_id && record.started_at_epoch_s >= since_epoch_s
        })
        .count()
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn severity_rank(severity: RuleAlertSeverity) -> u8 {
    match severity {
        RuleAlertSeverity::Critical => 2,
        RuleAlertSeverity::Warning => 1,
    }
}

fn fit_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if value.len() <= width {
        return value.to_owned();
    }
    value.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        plan_event_hook_runs, record_hook_execution, render_event_hook_panel_lines,
        EventHookConfig, HookAutomationState, PlannedHookRun,
    };
    use crate::alert_rule_dsl::{RuleAlertSeverity, RuleEventSource, TriggeredRuleAlert};

    fn alert(rule_id: &str, severity: RuleAlertSeverity) -> TriggeredRuleAlert {
        TriggeredRuleAlert {
            rule_id: rule_id.to_owned(),
            source: RuleEventSource::Status,
            severity,
            message: format!("rule {rule_id}"),
        }
    }

    fn config(hook_id: &str, rule_ids: &[&str], command: &str) -> EventHookConfig {
        EventHookConfig {
            hook_id: hook_id.to_owned(),
            rule_ids: rule_ids.iter().map(|entry| (*entry).to_owned()).collect(),
            command: command.to_owned(),
            ..EventHookConfig::default()
        }
    }

    #[test]
    fn schedules_matching_hook() {
        let plan = plan_event_hook_runs(
            &[config("hook-a", &["panic-log"], "scripts/page.sh")],
            &[alert("panic-log", RuleAlertSeverity::Critical)],
            1_000,
            &HookAutomationState::default(),
        );
        assert_eq!(plan.scheduled.len(), 1);
        assert_eq!(plan.scheduled[0].hook_id, "hook-a");
        assert_eq!(plan.scheduled[0].rule_id, "panic-log");
    }

    #[test]
    fn cooldown_blocks_retrigger() {
        let mut state = HookAutomationState::default();
        let run = PlannedHookRun {
            hook_id: "hook-a".to_owned(),
            rule_id: "panic-log".to_owned(),
            severity: RuleAlertSeverity::Critical,
            command: "scripts/page.sh".to_owned(),
            args: Vec::new(),
            reason: "matched".to_owned(),
        };
        record_hook_execution(&mut state, &run, 1_000, true, "ok");
        let plan = plan_event_hook_runs(
            &[config("hook-a", &["panic-log"], "scripts/page.sh")],
            &[alert("panic-log", RuleAlertSeverity::Critical)],
            1_020,
            &state,
        );
        assert!(plan.scheduled.is_empty());
        assert!(plan
            .skipped
            .iter()
            .any(|entry| entry.reason.contains("cooldown")));
    }

    #[test]
    fn hourly_limit_is_enforced() {
        let mut state = HookAutomationState::default();
        let run = PlannedHookRun {
            hook_id: "hook-a".to_owned(),
            rule_id: "panic-log".to_owned(),
            severity: RuleAlertSeverity::Critical,
            command: "scripts/page.sh".to_owned(),
            args: Vec::new(),
            reason: "matched".to_owned(),
        };
        for offset in [100, 200, 300] {
            record_hook_execution(&mut state, &run, 3_600 + offset, true, "ok");
        }

        let mut hook = config("hook-a", &["panic-log"], "scripts/page.sh");
        hook.max_runs_per_hour = 3;
        hook.cooldown_s = 0;
        let plan = plan_event_hook_runs(
            &[hook],
            &[alert("panic-log", RuleAlertSeverity::Critical)],
            4_000,
            &state,
        );
        assert!(plan.scheduled.is_empty());
        assert!(plan
            .skipped
            .iter()
            .any(|entry| entry.reason.contains("hourly limit")));
    }

    #[test]
    fn disabled_and_missing_command_are_skipped() {
        let mut disabled = config("hook-a", &["panic-log"], "scripts/page.sh");
        disabled.enabled = false;
        let missing_cmd = config("hook-b", &["panic-log"], "");
        let plan = plan_event_hook_runs(
            &[disabled, missing_cmd],
            &[alert("panic-log", RuleAlertSeverity::Critical)],
            10,
            &HookAutomationState::default(),
        );
        assert!(plan.scheduled.is_empty());
        assert_eq!(plan.skipped.len(), 2);
    }

    #[test]
    fn panel_lines_include_summary() {
        let plan = plan_event_hook_runs(
            &[config("hook-a", &["panic-log"], "scripts/page.sh")],
            &[alert("panic-log", RuleAlertSeverity::Critical)],
            1_000,
            &HookAutomationState::default(),
        );
        let lines = render_event_hook_panel_lines(&plan, 80);
        assert!(lines[0].contains("EVENT HOOK AUTOMATION"));
        assert!(lines[1].contains("scheduled:1"));
        assert!(lines
            .iter()
            .any(|line| line.contains("hook-a <- panic-log")));
    }
}
