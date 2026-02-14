//! Cross-loop impact analysis for proposed shared-surface changes.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InterfaceKind {
    PublicApi,
    SharedConfig,
    Migration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedChange {
    pub source_loop_id: String,
    pub component: String,
    pub kind: InterfaceKind,
    pub symbols: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopDependencySample {
    pub loop_id: String,
    pub active: bool,
    pub owner: Option<String>,
    pub current_task_id: Option<String>,
    pub interfaces: Vec<LoopInterfaceUse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopInterfaceUse {
    pub component: String,
    pub kind: InterfaceKind,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImpactSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactMatch {
    pub component: String,
    pub kind: InterfaceKind,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactedLoop {
    pub loop_id: String,
    pub active: bool,
    pub owner: Option<String>,
    pub current_task_id: Option<String>,
    pub severity: ImpactSeverity,
    pub impact_score: i32,
    pub matched_symbols: usize,
    pub matches: Vec<ImpactMatch>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactMatrixRow {
    pub source_loop_id: String,
    pub target_loop_id: String,
    pub component: String,
    pub kind: InterfaceKind,
    pub symbol_count: usize,
    pub symbols: Vec<String>,
    pub severity: ImpactSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinationActionKind {
    PauseDependents,
    NotifyDependents,
    LetRace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinationAction {
    pub kind: CoordinationActionKind,
    pub loops: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossLoopImpactReport {
    pub proposed_change: ProposedChange,
    pub impacted_loops: Vec<ImpactedLoop>,
    pub matrix: Vec<ImpactMatrixRow>,
    pub actions: Vec<CoordinationAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImpactPolicy {
    pub critical_score: i32,
    pub high_score: i32,
    pub medium_score: i32,
    pub pause_on_critical_active: bool,
    pub notify_min_severity: ImpactSeverity,
    pub max_matrix_rows: usize,
}

impl Default for ImpactPolicy {
    fn default() -> Self {
        Self {
            critical_score: 95,
            high_score: 70,
            medium_score: 45,
            pause_on_critical_active: true,
            notify_min_severity: ImpactSeverity::Medium,
            max_matrix_rows: 50,
        }
    }
}

#[must_use]
pub fn analyze_cross_loop_impact(
    proposed_change: &ProposedChange,
    dependencies: &[LoopDependencySample],
    policy: &ImpactPolicy,
) -> CrossLoopImpactReport {
    let normalized_change = normalize_change(proposed_change);
    let normalized_dependencies = dependencies
        .iter()
        .map(normalize_dependency)
        .collect::<Vec<_>>();

    let mut impacted = Vec::new();
    let mut matrix = Vec::new();
    for dependency in &normalized_dependencies {
        if dependency.loop_id == normalized_change.source_loop_id || dependency.loop_id.is_empty() {
            continue;
        }

        let mut matches = Vec::new();
        for usage in &dependency.interfaces {
            if usage.component != normalized_change.component
                || usage.kind != normalized_change.kind
            {
                continue;
            }
            let overlap = overlapping_symbols(&normalized_change.symbols, &usage.symbols);
            if overlap.is_empty() {
                continue;
            }
            matches.push(ImpactMatch {
                component: usage.component.clone(),
                kind: usage.kind,
                symbols: overlap,
            });
        }

        if matches.is_empty() {
            continue;
        }

        let matched_symbols = matches.iter().map(|item| item.symbols.len()).sum::<usize>();
        let impact_score = score_impact(dependency, matched_symbols, normalized_change.kind);
        let severity = classify_severity(impact_score, policy);
        let reasons = impact_reasons(dependency, matched_symbols, impact_score, severity);

        impacted.push(ImpactedLoop {
            loop_id: dependency.loop_id.clone(),
            active: dependency.active,
            owner: dependency.owner.clone(),
            current_task_id: dependency.current_task_id.clone(),
            severity,
            impact_score,
            matched_symbols,
            matches: matches.clone(),
            reasons,
        });

        for impact in matches {
            matrix.push(ImpactMatrixRow {
                source_loop_id: normalized_change.source_loop_id.clone(),
                target_loop_id: dependency.loop_id.clone(),
                component: impact.component,
                kind: impact.kind,
                symbol_count: impact.symbols.len(),
                symbols: impact.symbols,
                severity,
            });
        }
    }

    impacted.sort_by(|a, b| {
        b.impact_score
            .cmp(&a.impact_score)
            .then(a.loop_id.cmp(&b.loop_id))
    });
    matrix.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(a.target_loop_id.cmp(&b.target_loop_id))
            .then(a.component.cmp(&b.component))
    });
    matrix.truncate(policy.max_matrix_rows.max(1));

    let actions = recommend_coordination_actions(&impacted, policy);
    CrossLoopImpactReport {
        proposed_change: normalized_change,
        impacted_loops: impacted,
        matrix,
        actions,
    }
}

#[must_use]
pub fn render_impact_matrix_rows(report: &CrossLoopImpactReport) -> Vec<String> {
    if report.matrix.is_empty() {
        return vec!["(no impacted loops)".to_owned()];
    }
    report
        .matrix
        .iter()
        .map(|row| {
            format!(
                "{} -> {} [{}:{}] symbols={} sev={}",
                row.source_loop_id,
                row.target_loop_id,
                row.component,
                interface_kind_label(row.kind),
                row.symbol_count,
                severity_label(row.severity)
            )
        })
        .collect()
}

#[must_use]
pub fn build_fmail_coordination_messages(report: &CrossLoopImpactReport) -> Vec<(String, String)> {
    let impact_by_loop = report
        .impacted_loops
        .iter()
        .map(|loop_impact| (loop_impact.loop_id.as_str(), loop_impact))
        .collect::<BTreeMap<_, _>>();

    let mut messages = Vec::new();
    for action in &report.actions {
        for loop_id in &action.loops {
            let Some(loop_impact) = impact_by_loop.get(loop_id.as_str()) else {
                continue;
            };
            let topic = format!("@{}", loop_id);
            let message = format!(
                "impact alert: {} change in {} affects your loop (severity={}, symbols={}, task={})",
                interface_kind_label(report.proposed_change.kind),
                report.proposed_change.component,
                severity_label(loop_impact.severity),
                loop_impact.matched_symbols,
                loop_impact
                    .current_task_id
                    .as_deref()
                    .unwrap_or("-")
            );
            messages.push((topic, message));
        }
    }
    messages.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    messages.dedup();
    messages
}

fn recommend_coordination_actions(
    impacted_loops: &[ImpactedLoop],
    policy: &ImpactPolicy,
) -> Vec<CoordinationAction> {
    let critical_active = impacted_loops
        .iter()
        .filter(|item| item.active && item.severity == ImpactSeverity::Critical)
        .map(|item| item.loop_id.clone())
        .collect::<Vec<_>>();
    let notify = impacted_loops
        .iter()
        .filter(|item| item.severity >= policy.notify_min_severity)
        .map(|item| item.loop_id.clone())
        .collect::<Vec<_>>();

    let mut actions = Vec::new();
    if policy.pause_on_critical_active && !critical_active.is_empty() {
        actions.push(CoordinationAction {
            kind: CoordinationActionKind::PauseDependents,
            loops: critical_active.clone(),
            reason: "critical active dependent loops detected".to_owned(),
        });
    }
    if !notify.is_empty() {
        actions.push(CoordinationAction {
            kind: CoordinationActionKind::NotifyDependents,
            loops: notify,
            reason: format!(
                "notify loops with severity >= {}",
                severity_label(policy.notify_min_severity)
            ),
        });
    }
    if actions.is_empty() {
        actions.push(CoordinationAction {
            kind: CoordinationActionKind::LetRace,
            loops: Vec::new(),
            reason: "no medium+ impact detected".to_owned(),
        });
    }
    actions
}

fn score_impact(
    dependency: &LoopDependencySample,
    matched_symbols: usize,
    kind: InterfaceKind,
) -> i32 {
    let mut score = 0i32;
    score += match kind {
        InterfaceKind::PublicApi => 30,
        InterfaceKind::SharedConfig => 24,
        InterfaceKind::Migration => 40,
    };
    score += (matched_symbols.min(6) as i32) * 8;
    if dependency.active {
        score += 20;
    }
    if dependency.current_task_id.is_some() {
        score += 8;
    }
    if dependency.owner.is_some() {
        score += 4;
    }
    score
}

fn classify_severity(score: i32, policy: &ImpactPolicy) -> ImpactSeverity {
    if score >= policy.critical_score {
        ImpactSeverity::Critical
    } else if score >= policy.high_score {
        ImpactSeverity::High
    } else if score >= policy.medium_score {
        ImpactSeverity::Medium
    } else {
        ImpactSeverity::Low
    }
}

fn impact_reasons(
    dependency: &LoopDependencySample,
    matched_symbols: usize,
    impact_score: i32,
    severity: ImpactSeverity,
) -> Vec<String> {
    let mut reasons = Vec::new();
    reasons.push(format!(
        "matched-symbols:{} (+{})",
        matched_symbols,
        matched_symbols * 8
    ));
    if dependency.active {
        reasons.push("active-loop (+20)".to_owned());
    }
    if dependency.current_task_id.is_some() {
        reasons.push("task-in-flight (+8)".to_owned());
    }
    reasons.push(format!(
        "severity={} (score={})",
        severity_label(severity),
        impact_score
    ));
    reasons
}

fn normalize_change(change: &ProposedChange) -> ProposedChange {
    let symbols = normalize_symbols(&change.symbols);
    ProposedChange {
        source_loop_id: normalize_id(&change.source_loop_id),
        component: normalize_component(&change.component),
        kind: change.kind,
        symbols,
        summary: change.summary.trim().to_owned(),
    }
}

fn normalize_dependency(sample: &LoopDependencySample) -> LoopDependencySample {
    let mut interfaces = sample
        .interfaces
        .iter()
        .map(|usage| LoopInterfaceUse {
            component: normalize_component(&usage.component),
            kind: usage.kind,
            symbols: normalize_symbols(&usage.symbols),
        })
        .filter(|usage| !usage.component.is_empty())
        .collect::<Vec<_>>();
    interfaces.sort_by(|a, b| {
        a.component
            .cmp(&b.component)
            .then(a.kind.cmp(&b.kind))
            .then(a.symbols.join("|").cmp(&b.symbols.join("|")))
    });
    interfaces
        .dedup_by(|a, b| a.component == b.component && a.kind == b.kind && a.symbols == b.symbols);

    LoopDependencySample {
        loop_id: normalize_id(&sample.loop_id),
        active: sample.active,
        owner: sample
            .owner
            .as_deref()
            .map(normalize_id)
            .filter(|value| !value.is_empty()),
        current_task_id: sample
            .current_task_id
            .as_deref()
            .map(normalize_id)
            .filter(|value| !value.is_empty()),
        interfaces,
    }
}

fn overlapping_symbols(change_symbols: &[String], usage_symbols: &[String]) -> Vec<String> {
    if change_symbols.is_empty() || usage_symbols.is_empty() {
        return Vec::new();
    }
    let usage = usage_symbols.iter().cloned().collect::<BTreeSet<_>>();
    change_symbols
        .iter()
        .filter(|symbol| usage.contains(*symbol))
        .cloned()
        .collect::<Vec<_>>()
}

fn normalize_component(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn normalize_id(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn normalize_symbols(symbols: &[String]) -> Vec<String> {
    let mut unique = BTreeSet::new();
    for symbol in symbols {
        let normalized = symbol.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            unique.insert(normalized);
        }
    }
    unique.into_iter().collect()
}

fn interface_kind_label(kind: InterfaceKind) -> &'static str {
    match kind {
        InterfaceKind::PublicApi => "api",
        InterfaceKind::SharedConfig => "config",
        InterfaceKind::Migration => "migration",
    }
}

fn severity_label(severity: ImpactSeverity) -> &'static str {
    match severity {
        ImpactSeverity::Low => "low",
        ImpactSeverity::Medium => "medium",
        ImpactSeverity::High => "high",
        ImpactSeverity::Critical => "critical",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        analyze_cross_loop_impact, build_fmail_coordination_messages, render_impact_matrix_rows,
        ImpactPolicy, ImpactSeverity, InterfaceKind, LoopDependencySample, LoopInterfaceUse,
        ProposedChange,
    };

    fn sample_change() -> ProposedChange {
        ProposedChange {
            source_loop_id: "loop-a".to_owned(),
            component: "forge-db".to_owned(),
            kind: InterfaceKind::PublicApi,
            symbols: vec![
                "load_runs".to_owned(),
                "persist_run".to_owned(),
                "schema_version".to_owned(),
            ],
            summary: "refactor db write path".to_owned(),
        }
    }

    fn sample_dependencies() -> Vec<LoopDependencySample> {
        vec![
            LoopDependencySample {
                loop_id: "loop-b".to_owned(),
                active: true,
                owner: Some("agent-b".to_owned()),
                current_task_id: Some("forge-x1".to_owned()),
                interfaces: vec![LoopInterfaceUse {
                    component: "forge-db".to_owned(),
                    kind: InterfaceKind::PublicApi,
                    symbols: vec!["load_runs".to_owned(), "other_fn".to_owned()],
                }],
            },
            LoopDependencySample {
                loop_id: "loop-c".to_owned(),
                active: false,
                owner: Some("agent-c".to_owned()),
                current_task_id: None,
                interfaces: vec![LoopInterfaceUse {
                    component: "forge-db".to_owned(),
                    kind: InterfaceKind::PublicApi,
                    symbols: vec!["persist_run".to_owned(), "schema_version".to_owned()],
                }],
            },
            LoopDependencySample {
                loop_id: "loop-d".to_owned(),
                active: true,
                owner: None,
                current_task_id: Some("forge-y2".to_owned()),
                interfaces: vec![LoopInterfaceUse {
                    component: "forge-cli".to_owned(),
                    kind: InterfaceKind::PublicApi,
                    symbols: vec!["run".to_owned()],
                }],
            },
        ]
    }

    #[test]
    fn matches_overlapping_api_symbols_and_ranks_impacts() {
        let report = analyze_cross_loop_impact(
            &sample_change(),
            &sample_dependencies(),
            &ImpactPolicy::default(),
        );
        assert_eq!(report.impacted_loops.len(), 2);
        assert_eq!(report.impacted_loops[0].loop_id, "loop-b");
        assert!(report.impacted_loops[0].impact_score >= report.impacted_loops[1].impact_score);
        assert_eq!(report.matrix.len(), 2);
        assert!(report
            .matrix
            .iter()
            .any(|row| row.target_loop_id == "loop-c" && row.symbol_count == 2));
    }

    #[test]
    fn includes_pause_action_for_critical_active_dependents() {
        let policy = ImpactPolicy {
            critical_score: 60,
            ..ImpactPolicy::default()
        };
        let report = analyze_cross_loop_impact(&sample_change(), &sample_dependencies(), &policy);
        assert!(report
            .actions
            .iter()
            .any(|action| action.kind == super::CoordinationActionKind::PauseDependents));
        assert!(report
            .actions
            .iter()
            .any(|action| action.kind == super::CoordinationActionKind::NotifyDependents));
    }

    #[test]
    fn lets_race_when_no_impacted_loops() {
        let change = ProposedChange {
            component: "forge-mail".to_owned(),
            ..sample_change()
        };
        let report =
            analyze_cross_loop_impact(&change, &sample_dependencies(), &ImpactPolicy::default());
        assert!(report.impacted_loops.is_empty());
        assert_eq!(report.actions.len(), 1);
        assert_eq!(
            report.actions[0].kind,
            super::CoordinationActionKind::LetRace
        );
    }

    #[test]
    fn handles_shared_config_and_migration_kinds() {
        let change = ProposedChange {
            kind: InterfaceKind::SharedConfig,
            symbols: vec!["poll_interval_ms".to_owned()],
            ..sample_change()
        };
        let dependencies = vec![
            LoopDependencySample {
                interfaces: vec![LoopInterfaceUse {
                    component: "forge-db".to_owned(),
                    kind: InterfaceKind::SharedConfig,
                    symbols: vec!["poll_interval_ms".to_owned()],
                }],
                ..sample_dependencies()[0].clone()
            },
            LoopDependencySample {
                interfaces: vec![LoopInterfaceUse {
                    component: "forge-db".to_owned(),
                    kind: InterfaceKind::Migration,
                    symbols: vec!["v4".to_owned()],
                }],
                ..sample_dependencies()[1].clone()
            },
        ];
        let report = analyze_cross_loop_impact(&change, &dependencies, &ImpactPolicy::default());
        assert_eq!(report.impacted_loops.len(), 1);
        assert_eq!(report.impacted_loops[0].loop_id, "loop-b");
        assert!(report
            .impacted_loops
            .iter()
            .all(|impact| impact.severity >= ImpactSeverity::Medium));
    }

    #[test]
    fn render_rows_and_fmail_messages_are_stable() {
        let report = analyze_cross_loop_impact(
            &sample_change(),
            &sample_dependencies(),
            &ImpactPolicy::default(),
        );
        let rows = render_impact_matrix_rows(&report);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains("loop-a -> loop-b"));

        let messages = build_fmail_coordination_messages(&report);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].0, "@loop-b");
        assert!(messages[0]
            .1
            .contains("impact alert: api change in forge-db"));
    }
}
