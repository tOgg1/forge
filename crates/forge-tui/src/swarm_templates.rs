//! Swarm template library and spawn presets for Forge TUI.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwarmTemplate {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub max_concurrency: usize,
    pub profile_map: Vec<TemplateProfileMap>,
    pub spawn_presets: Vec<SwarmSpawnPreset>,
    pub guardrails: SwarmGuardrails,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateProfileMap {
    pub lane: &'static str,
    pub profile: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwarmSpawnPreset {
    pub lane: &'static str,
    pub profile: &'static str,
    pub prompt: &'static str,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwarmGuardrails {
    pub stale_takeover_minutes: u32,
    pub require_claim_broadcast: bool,
    pub require_full_validation_before_close: bool,
    pub max_parallel_task_claims: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RampStagePlan {
    pub id: &'static str,
    pub description: &'static str,
    pub target_concurrency: usize,
    pub required_preflight_checks: Vec<&'static str>,
    pub min_proof_runs_passing: usize,
    pub min_healthy_loops: usize,
    pub max_missing_health_signals: usize,
    pub max_claim_conflicts: usize,
    pub max_stale_in_progress_tasks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlledRampWizard {
    pub template_id: String,
    pub stages: Vec<RampStagePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RampHealthSnapshot {
    pub preflight_checks_passed: usize,
    pub proof_runs_passing: usize,
    pub healthy_loops: usize,
    pub missing_health_signals: usize,
    pub claim_conflicts: usize,
    pub stale_in_progress_tasks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RampGateStatus {
    pub can_expand: bool,
    pub stage_id: String,
    pub target_concurrency: usize,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RampDecision {
    Blocked {
        stage_id: String,
        blockers: Vec<String>,
    },
    Advance {
        next_stage_index: usize,
        target_concurrency: usize,
    },
    Complete,
}

#[must_use]
pub fn controlled_ramp_wizard(template: &SwarmTemplate) -> ControlledRampWizard {
    let max = template.max_concurrency.max(1);
    let proof_target = 1usize.min(max);
    let ramp_target = max.div_ceil(2).max((proof_target + 1).min(max));
    let full_target = max;
    let preflight = vec!["forge ps", "sv task ready --json", "fmail log task -n 200"];

    ControlledRampWizard {
        template_id: template.id.to_owned(),
        stages: vec![
            RampStagePlan {
                id: "proof",
                description: "Single-loop proof and command-path validation.",
                target_concurrency: proof_target,
                required_preflight_checks: preflight.clone(),
                min_proof_runs_passing: 1,
                min_healthy_loops: proof_target,
                max_missing_health_signals: 0,
                max_claim_conflicts: 0,
                max_stale_in_progress_tasks: 0,
            },
            RampStagePlan {
                id: "ramp",
                description: "Controlled expansion after proof and health checks.",
                target_concurrency: ramp_target,
                required_preflight_checks: preflight.clone(),
                min_proof_runs_passing: 2,
                min_healthy_loops: ramp_target,
                max_missing_health_signals: 0,
                max_claim_conflicts: 0,
                max_stale_in_progress_tasks: 0,
            },
            RampStagePlan {
                id: "full",
                description: "Full-scale topology after sustained healthy operation.",
                target_concurrency: full_target,
                required_preflight_checks: preflight,
                min_proof_runs_passing: 3,
                min_healthy_loops: full_target,
                max_missing_health_signals: 0,
                max_claim_conflicts: 0,
                max_stale_in_progress_tasks: 0,
            },
        ],
    }
}

#[must_use]
pub fn evaluate_ramp_gate(stage: &RampStagePlan, snapshot: &RampHealthSnapshot) -> RampGateStatus {
    let mut blockers = Vec::new();
    if snapshot.preflight_checks_passed < stage.required_preflight_checks.len() {
        blockers.push(format!(
            "preflight checks incomplete: {}/{}",
            snapshot.preflight_checks_passed,
            stage.required_preflight_checks.len()
        ));
    }
    if snapshot.proof_runs_passing < stage.min_proof_runs_passing {
        blockers.push(format!(
            "proof runs passing below threshold: {} < {}",
            snapshot.proof_runs_passing, stage.min_proof_runs_passing
        ));
    }
    if snapshot.healthy_loops < stage.min_healthy_loops {
        blockers.push(format!(
            "healthy loops below target: {} < {}",
            snapshot.healthy_loops, stage.min_healthy_loops
        ));
    }
    if snapshot.missing_health_signals > stage.max_missing_health_signals {
        blockers.push(format!(
            "missing health signals: {} > {}",
            snapshot.missing_health_signals, stage.max_missing_health_signals
        ));
    }
    if snapshot.claim_conflicts > stage.max_claim_conflicts {
        blockers.push(format!(
            "claim conflicts present: {} > {}",
            snapshot.claim_conflicts, stage.max_claim_conflicts
        ));
    }
    if snapshot.stale_in_progress_tasks > stage.max_stale_in_progress_tasks {
        blockers.push(format!(
            "stale in-progress tasks: {} > {}",
            snapshot.stale_in_progress_tasks, stage.max_stale_in_progress_tasks
        ));
    }

    RampGateStatus {
        can_expand: blockers.is_empty(),
        stage_id: stage.id.to_owned(),
        target_concurrency: stage.target_concurrency,
        blockers,
    }
}

#[must_use]
pub fn evaluate_ramp_progression(
    wizard: &ControlledRampWizard,
    stage_index: usize,
    snapshot: &RampHealthSnapshot,
) -> RampDecision {
    let Some(stage) = wizard.stages.get(stage_index) else {
        return RampDecision::Complete;
    };
    let gate = evaluate_ramp_gate(stage, snapshot);
    if !gate.can_expand {
        return RampDecision::Blocked {
            stage_id: gate.stage_id,
            blockers: gate.blockers,
        };
    }
    if stage_index + 1 >= wizard.stages.len() {
        return RampDecision::Complete;
    }
    let next_stage_index = stage_index + 1;
    RampDecision::Advance {
        next_stage_index,
        target_concurrency: wizard.stages[next_stage_index].target_concurrency,
    }
}

#[must_use]
pub fn default_swarm_templates() -> Vec<SwarmTemplate> {
    vec![template_small(), template_medium(), template_full()]
}

#[must_use]
pub fn find_swarm_template(id_or_title: &str) -> Option<SwarmTemplate> {
    let needle = id_or_title.trim().to_ascii_lowercase();
    default_swarm_templates().into_iter().find(|template| {
        template.id.eq_ignore_ascii_case(&needle)
            || template.title.eq_ignore_ascii_case(&needle)
            || template
                .title
                .to_ascii_lowercase()
                .replace(' ', "-")
                .eq_ignore_ascii_case(&needle)
    })
}

fn template_small() -> SwarmTemplate {
    SwarmTemplate {
        id: "small",
        title: "Small",
        description: "2-3 loops for focused feature delivery with low coordination overhead.",
        max_concurrency: 3,
        profile_map: vec![
            TemplateProfileMap {
                lane: "dev",
                profile: "codex3",
            },
            TemplateProfileMap {
                lane: "proof",
                profile: "cc2",
            },
            TemplateProfileMap {
                lane: "committer",
                profile: "codex2",
            },
        ],
        spawn_presets: vec![
            SwarmSpawnPreset {
                lane: "dev",
                profile: "codex3",
                prompt: "swarm-tui-next-codex-continuous",
                count: 1,
            },
            SwarmSpawnPreset {
                lane: "proof",
                profile: "cc2",
                prompt: "swarm-tui-next-claude-single",
                count: 1,
            },
            SwarmSpawnPreset {
                lane: "committer",
                profile: "codex2",
                prompt: "swarm-tui-next-codex-continuous",
                count: 1,
            },
        ],
        guardrails: SwarmGuardrails {
            stale_takeover_minutes: 45,
            require_claim_broadcast: true,
            require_full_validation_before_close: true,
            max_parallel_task_claims: 1,
        },
    }
}

fn template_medium() -> SwarmTemplate {
    SwarmTemplate {
        id: "medium",
        title: "Medium",
        description:
            "Balanced topology for sustained throughput across feature + proof + committer lanes.",
        max_concurrency: 6,
        profile_map: vec![
            TemplateProfileMap {
                lane: "dev-codex",
                profile: "codex3",
            },
            TemplateProfileMap {
                lane: "dev-claude",
                profile: "cc3",
            },
            TemplateProfileMap {
                lane: "proof",
                profile: "cc2",
            },
            TemplateProfileMap {
                lane: "committer",
                profile: "codex2",
            },
        ],
        spawn_presets: vec![
            SwarmSpawnPreset {
                lane: "dev-codex",
                profile: "codex3",
                prompt: "swarm-tui-next-codex-continuous",
                count: 2,
            },
            SwarmSpawnPreset {
                lane: "dev-claude",
                profile: "cc3",
                prompt: "swarm-tui-next-claude-single",
                count: 1,
            },
            SwarmSpawnPreset {
                lane: "proof",
                profile: "cc2",
                prompt: "swarm-tui-next-claude-single",
                count: 2,
            },
            SwarmSpawnPreset {
                lane: "committer",
                profile: "codex2",
                prompt: "swarm-tui-next-codex-continuous",
                count: 1,
            },
        ],
        guardrails: SwarmGuardrails {
            stale_takeover_minutes: 45,
            require_claim_broadcast: true,
            require_full_validation_before_close: true,
            max_parallel_task_claims: 1,
        },
    }
}

fn template_full() -> SwarmTemplate {
    SwarmTemplate {
        id: "full",
        title: "Full",
        description: "High-throughput topology for large backlogs with explicit safety rails.",
        max_concurrency: 10,
        profile_map: vec![
            TemplateProfileMap {
                lane: "dev-codex",
                profile: "codex3",
            },
            TemplateProfileMap {
                lane: "dev-claude",
                profile: "cc3",
            },
            TemplateProfileMap {
                lane: "proof-codex",
                profile: "codex2",
            },
            TemplateProfileMap {
                lane: "proof-claude",
                profile: "cc2",
            },
            TemplateProfileMap {
                lane: "stale-auditor",
                profile: "codex3",
            },
            TemplateProfileMap {
                lane: "committer",
                profile: "codex2",
            },
        ],
        spawn_presets: vec![
            SwarmSpawnPreset {
                lane: "dev-codex",
                profile: "codex3",
                prompt: "swarm-tui-next-codex-continuous",
                count: 3,
            },
            SwarmSpawnPreset {
                lane: "dev-claude",
                profile: "cc3",
                prompt: "swarm-tui-next-claude-single",
                count: 2,
            },
            SwarmSpawnPreset {
                lane: "proof-codex",
                profile: "codex2",
                prompt: "swarm-tui-next-codex-continuous",
                count: 2,
            },
            SwarmSpawnPreset {
                lane: "proof-claude",
                profile: "cc2",
                prompt: "swarm-tui-next-claude-single",
                count: 2,
            },
            SwarmSpawnPreset {
                lane: "stale-auditor",
                profile: "codex3",
                prompt: "rust-swarm-stale-auditor",
                count: 1,
            },
            SwarmSpawnPreset {
                lane: "committer",
                profile: "codex2",
                prompt: "rust-swarm-committer",
                count: 1,
            },
        ],
        guardrails: SwarmGuardrails {
            stale_takeover_minutes: 45,
            require_claim_broadcast: true,
            require_full_validation_before_close: true,
            max_parallel_task_claims: 1,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        controlled_ramp_wizard, default_swarm_templates, evaluate_ramp_progression,
        find_swarm_template, RampDecision, RampHealthSnapshot,
    };

    #[test]
    fn default_library_contains_small_medium_full_in_order() {
        let templates = default_swarm_templates();
        let ids: Vec<&str> = templates.iter().map(|template| template.id).collect();
        assert_eq!(ids, vec!["small", "medium", "full"]);
    }

    #[test]
    fn templates_include_profile_maps_spawn_presets_and_guardrails() {
        for template in default_swarm_templates() {
            assert!(!template.profile_map.is_empty());
            assert!(!template.spawn_presets.is_empty());
            assert!(template.max_concurrency >= template.guardrails.max_parallel_task_claims);
            assert!(template.guardrails.require_claim_broadcast);
            assert!(template.guardrails.require_full_validation_before_close);
            for preset in &template.spawn_presets {
                assert!(preset.count >= 1);
                assert!(
                    template
                        .profile_map
                        .iter()
                        .any(|mapping| mapping.lane == preset.lane
                            && mapping.profile == preset.profile),
                    "missing profile mapping for lane={} profile={} in template={}",
                    preset.lane,
                    preset.profile,
                    template.id
                );
            }
        }
    }

    #[test]
    fn find_template_matches_id_or_title_case_insensitive() {
        assert_eq!(
            find_swarm_template("small").map(|template| template.id),
            Some("small")
        );
        assert_eq!(
            find_swarm_template("SMALL").map(|template| template.id),
            Some("small")
        );
        assert_eq!(
            find_swarm_template("Medium").map(|template| template.id),
            Some("medium")
        );
        assert_eq!(
            find_swarm_template("full").map(|template| template.id),
            Some("full")
        );
        assert!(find_swarm_template("unknown").is_none());
    }

    #[test]
    fn full_template_has_highest_capacity_and_includes_auditor_lane() {
        let templates = default_swarm_templates();
        assert!(templates.iter().any(|template| {
            template.id == "full"
                && template.max_concurrency == 10
                && template
                    .spawn_presets
                    .iter()
                    .any(|preset| preset.lane == "stale-auditor")
        }));
    }

    #[test]
    fn controlled_ramp_wizard_has_ordered_stages_and_targets() {
        let templates = default_swarm_templates();
        let template = match templates
            .into_iter()
            .find(|template| template.id == "medium")
        {
            Some(template) => template,
            None => panic!("medium template exists"),
        };
        let wizard = controlled_ramp_wizard(&template);
        let stage_ids: Vec<&str> = wizard.stages.iter().map(|stage| stage.id).collect();
        assert_eq!(stage_ids, vec!["proof", "ramp", "full"]);
        assert!(wizard.stages[0].target_concurrency <= wizard.stages[1].target_concurrency);
        assert!(wizard.stages[1].target_concurrency <= wizard.stages[2].target_concurrency);
        assert_eq!(
            wizard.stages[2].target_concurrency,
            template.max_concurrency
        );
    }

    #[test]
    fn controlled_ramp_blocks_on_missing_health_signals() {
        let templates = default_swarm_templates();
        let template = match templates
            .into_iter()
            .find(|template| template.id == "small")
        {
            Some(template) => template,
            None => panic!("small template exists"),
        };
        let wizard = controlled_ramp_wizard(&template);
        let snapshot = RampHealthSnapshot {
            preflight_checks_passed: 3,
            proof_runs_passing: 2,
            healthy_loops: wizard.stages[0].target_concurrency,
            missing_health_signals: 1,
            claim_conflicts: 0,
            stale_in_progress_tasks: 0,
        };
        let decision = evaluate_ramp_progression(&wizard, 0, &snapshot);
        match decision {
            RampDecision::Blocked { blockers, .. } => {
                assert!(blockers
                    .iter()
                    .any(|blocker| blocker.contains("missing health signals")));
            }
            other => panic!("expected blocked decision, got {other:?}"),
        }
    }

    #[test]
    fn controlled_ramp_blocks_when_preflight_incomplete_between_stages() {
        let templates = default_swarm_templates();
        let template = match templates
            .into_iter()
            .find(|template| template.id == "medium")
        {
            Some(template) => template,
            None => panic!("medium template exists"),
        };
        let wizard = controlled_ramp_wizard(&template);
        let snapshot = RampHealthSnapshot {
            preflight_checks_passed: 1,
            proof_runs_passing: 2,
            healthy_loops: wizard.stages[1].target_concurrency,
            missing_health_signals: 0,
            claim_conflicts: 0,
            stale_in_progress_tasks: 0,
        };
        let decision = evaluate_ramp_progression(&wizard, 1, &snapshot);
        assert!(matches!(decision, RampDecision::Blocked { .. }));
    }

    #[test]
    fn controlled_ramp_advances_when_gate_is_healthy() {
        let templates = default_swarm_templates();
        let template = match templates
            .into_iter()
            .find(|template| template.id == "small")
        {
            Some(template) => template,
            None => panic!("small template exists"),
        };
        let wizard = controlled_ramp_wizard(&template);
        let snapshot = RampHealthSnapshot {
            preflight_checks_passed: 3,
            proof_runs_passing: 2,
            healthy_loops: wizard.stages[0].target_concurrency,
            missing_health_signals: 0,
            claim_conflicts: 0,
            stale_in_progress_tasks: 0,
        };
        let decision = evaluate_ramp_progression(&wizard, 0, &snapshot);
        assert_eq!(
            decision,
            RampDecision::Advance {
                next_stage_index: 1,
                target_concurrency: wizard.stages[1].target_concurrency
            }
        );
    }

    #[test]
    fn controlled_ramp_marks_complete_after_final_healthy_stage() {
        let templates = default_swarm_templates();
        let template = match templates.into_iter().find(|template| template.id == "full") {
            Some(template) => template,
            None => panic!("full template exists"),
        };
        let wizard = controlled_ramp_wizard(&template);
        let final_stage = &wizard.stages[wizard.stages.len() - 1];
        let snapshot = RampHealthSnapshot {
            preflight_checks_passed: 3,
            proof_runs_passing: 5,
            healthy_loops: final_stage.target_concurrency,
            missing_health_signals: 0,
            claim_conflicts: 0,
            stale_in_progress_tasks: 0,
        };
        let decision = evaluate_ramp_progression(&wizard, wizard.stages.len() - 1, &snapshot);
        assert_eq!(decision, RampDecision::Complete);
    }
}
