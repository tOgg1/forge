//! Loop TUI action/confirm helpers.
//!
//! Focused parity slice for stop/kill/delete/resume flows from `internal/looptui/looptui.go`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopState {
    Running,
    Sleeping,
    Waiting,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Resume,
    Stop,
    Kill,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmState {
    pub action: ActionKind,
    pub loop_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRequest {
    pub kind: ActionKind,
    pub loop_id: String,
    pub force_delete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionTarget {
    pub loop_id: String,
    pub short_id: String,
    pub loop_state: LoopState,
    pub pool: String,
    pub profile: String,
    pub tags: Vec<String>,
    pub batch_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailPolicy {
    pub protected_pools: Vec<String>,
    pub protected_tags: Vec<String>,
    pub allow_overrides: bool,
    pub require_ticket_for_override: bool,
    pub min_override_reason_chars: usize,
    pub max_batch_without_override: usize,
}

impl Default for GuardrailPolicy {
    fn default() -> Self {
        Self {
            protected_pools: Vec::new(),
            protected_tags: Vec::new(),
            allow_overrides: true,
            require_ticket_for_override: false,
            min_override_reason_chars: 12,
            max_batch_without_override: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyOverride {
    pub actor: String,
    pub reason: String,
    pub ticket: Option<String>,
    pub approved_by: Option<String>,
    pub timestamp_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideAuditEntry {
    pub action: ActionKind,
    pub loop_id: String,
    pub actor: String,
    pub reason: String,
    pub ticket: Option<String>,
    pub approved_by: Option<String>,
    pub timestamp_utc: String,
    pub policy_rule: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardrailDecision {
    Allowed {
        confirm: Option<ConfirmState>,
        audit: Option<OverrideAuditEntry>,
    },
    Blocked {
        reason: String,
        escalation_hint: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmInputResult {
    Cancelled,
    OpenHelp,
    Submit(ActionRequest),
    Noop,
}

/// Evaluate policy-aware guardrails for risky actions before dispatching.
#[must_use]
pub fn evaluate_action_guardrail(
    action: ActionKind,
    target: &ActionTarget,
    policy: &GuardrailPolicy,
    override_request: Option<&PolicyOverride>,
) -> GuardrailDecision {
    let guardrail = find_guardrail_rule(action, target, policy);
    let mut audit = None;

    if let Some((policy_rule, reason, escalation_hint)) = guardrail {
        let Some(override_request) = override_request else {
            return GuardrailDecision::Blocked {
                reason,
                escalation_hint,
            };
        };
        if !policy.allow_overrides {
            return GuardrailDecision::Blocked {
                reason: "policy override denied by workspace settings".to_owned(),
                escalation_hint: "request policy owner to temporarily allow overrides".to_owned(),
            };
        }
        if override_request.actor.trim().is_empty() {
            return GuardrailDecision::Blocked {
                reason: "override actor is required".to_owned(),
                escalation_hint: "set actor id before requesting override".to_owned(),
            };
        }
        if override_request.reason.trim().len() < policy.min_override_reason_chars {
            return GuardrailDecision::Blocked {
                reason: format!(
                    "override reason too short (<{} chars)",
                    policy.min_override_reason_chars
                ),
                escalation_hint: "provide concrete impact/risk context in override reason"
                    .to_owned(),
            };
        }
        if policy.require_ticket_for_override
            && override_request
                .ticket
                .as_ref()
                .map_or(true, |ticket| ticket.trim().is_empty())
        {
            return GuardrailDecision::Blocked {
                reason: "override requires tracking ticket".to_owned(),
                escalation_hint: "attach incident/task ticket and retry override".to_owned(),
            };
        }

        audit = Some(OverrideAuditEntry {
            action,
            loop_id: target.loop_id.clone(),
            actor: override_request.actor.trim().to_owned(),
            reason: override_request.reason.trim().to_owned(),
            ticket: override_request
                .ticket
                .as_ref()
                .map(|ticket| ticket.trim().to_owned())
                .filter(|ticket| !ticket.is_empty()),
            approved_by: override_request
                .approved_by
                .as_ref()
                .map(|approved_by| approved_by.trim().to_owned())
                .filter(|approved_by| !approved_by.is_empty()),
            timestamp_utc: override_request.timestamp_utc.trim().to_owned(),
            policy_rule: policy_rule.to_owned(),
        });
    }

    let confirm = if action == ActionKind::Resume {
        None
    } else {
        match enter_confirm(action, &target.loop_id, &target.short_id, target.loop_state) {
            Ok(confirm) => Some(confirm),
            Err(err) => {
                return GuardrailDecision::Blocked {
                    reason: err,
                    escalation_hint: "resolve confirm-state mismatch before retrying action"
                        .to_owned(),
                }
            }
        }
    };

    GuardrailDecision::Allowed { confirm, audit }
}

fn find_guardrail_rule(
    action: ActionKind,
    target: &ActionTarget,
    policy: &GuardrailPolicy,
) -> Option<(&'static str, String, String)> {
    if action == ActionKind::Resume {
        return None;
    }

    let pool = target.pool.trim().to_ascii_lowercase();
    let protected_pools: Vec<String> = policy
        .protected_pools
        .iter()
        .map(|candidate| candidate.trim().to_ascii_lowercase())
        .filter(|candidate| !candidate.is_empty())
        .collect();
    if !pool.is_empty() && protected_pools.iter().any(|candidate| candidate == &pool) {
        return Some((
            "protected-pool",
            format!(
                "action blocked: pool '{}' is policy-protected",
                target.pool.trim()
            ),
            "escalate to pool owner or provide approved override rationale".to_owned(),
        ));
    }

    let target_tags: Vec<String> = target
        .tags
        .iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    let protected_tags: Vec<String> = policy
        .protected_tags
        .iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    if let Some(matched_tag) = protected_tags
        .iter()
        .find(|protected_tag| target_tags.iter().any(|tag| tag == *protected_tag))
    {
        return Some((
            "protected-tag",
            format!("action blocked: protected tag '{}' on target", matched_tag),
            "escalate to workspace policy owner or submit risk-acknowledged override".to_owned(),
        ));
    }

    let effective_batch = target.batch_size.max(1);
    if effective_batch > policy.max_batch_without_override {
        return Some((
            "batch-threshold",
            format!(
                "action blocked: batch size {} exceeds policy limit {}",
                effective_batch, policy.max_batch_without_override
            ),
            "split the action into smaller batches or provide approved override".to_owned(),
        ));
    }

    None
}

/// Build display ID used in prompts: short ID first, then full ID (max 8 chars).
#[must_use]
pub fn loop_display_id(loop_id: &str, short_id: &str) -> String {
    let short = short_id.trim();
    if !short.is_empty() {
        return short.to_string();
    }

    if loop_id.len() <= 8 {
        return loop_id.to_string();
    }
    loop_id.chars().take(8).collect()
}

/// Enter confirm mode for destructive actions.
pub fn enter_confirm(
    action: ActionKind,
    loop_id: &str,
    short_id: &str,
    state: LoopState,
) -> Result<ConfirmState, String> {
    let display_id = loop_display_id(loop_id, short_id);
    let prompt = match action {
        ActionKind::Stop => format!("Stop loop {display_id} after current iteration? [y/N]"),
        ActionKind::Kill => format!("Kill loop {display_id} immediately? [y/N]"),
        ActionKind::Delete => {
            if state == LoopState::Stopped {
                format!("Delete loop record {display_id}? [y/N]")
            } else {
                format!("Loop is still running. Force delete record {display_id}? [y/N]")
            }
        }
        ActionKind::Resume => {
            return Err("resume does not require confirmation".to_string());
        }
    };

    Ok(ConfirmState {
        action,
        loop_id: loop_id.to_string(),
        prompt,
    })
}

/// Convert an accepted confirm prompt into an executable action request.
#[must_use]
pub fn confirm_to_request(confirm: &ConfirmState) -> ActionRequest {
    ActionRequest {
        kind: confirm.action,
        loop_id: confirm.loop_id.clone(),
        force_delete: confirm.action == ActionKind::Delete
            && confirm.prompt.contains("Force delete"),
    }
}

/// Confirm-mode key handling parity.
#[must_use]
pub fn handle_confirm_input(confirm: &ConfirmState, key: &str) -> ConfirmInputResult {
    match key {
        "q" | "esc" | "n" | "N" | "enter" => ConfirmInputResult::Cancelled,
        "?" => ConfirmInputResult::OpenHelp,
        "y" | "Y" => ConfirmInputResult::Submit(confirm_to_request(confirm)),
        _ => ConfirmInputResult::Noop,
    }
}

/// Action status text used when dispatching async action commands.
#[must_use]
pub fn action_status(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::Resume => "Resuming loop...",
        ActionKind::Stop => "Requesting graceful stop...",
        ActionKind::Kill => "Killing loop...",
        ActionKind::Delete => "Deleting loop record...",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        action_status, confirm_to_request, enter_confirm, evaluate_action_guardrail,
        handle_confirm_input, loop_display_id, ActionKind, ActionTarget, ConfirmInputResult,
        GuardrailDecision, GuardrailPolicy, LoopState, PolicyOverride,
    };

    #[test]
    fn display_id_prefers_short_id_then_shortens_full_id() {
        assert_eq!(loop_display_id("loop-full-id-123", "abc123"), "abc123");
        assert_eq!(loop_display_id("loop-full-id-123", ""), "loop-ful");
        assert_eq!(loop_display_id("short", ""), "short");
    }

    #[test]
    fn stop_and_kill_prompts_match_go_shape() {
        let stop = match enter_confirm(
            ActionKind::Stop,
            "loop-running-id",
            "run123",
            LoopState::Running,
        ) {
            Ok(v) => v,
            Err(err) => panic!("stop confirm: {err}"),
        };
        assert_eq!(
            stop.prompt,
            "Stop loop run123 after current iteration? [y/N]"
        );

        let kill = match enter_confirm(
            ActionKind::Kill,
            "loop-running-id",
            "run123",
            LoopState::Running,
        ) {
            Ok(v) => v,
            Err(err) => panic!("kill confirm: {err}"),
        };
        assert_eq!(kill.prompt, "Kill loop run123 immediately? [y/N]");
    }

    #[test]
    fn delete_prompt_matches_state_rules() {
        let stopped = match enter_confirm(
            ActionKind::Delete,
            "loop-stopped-id",
            "stop12",
            LoopState::Stopped,
        ) {
            Ok(v) => v,
            Err(err) => panic!("delete confirm stopped: {err}"),
        };
        assert_eq!(stopped.prompt, "Delete loop record stop12? [y/N]");

        let running = match enter_confirm(
            ActionKind::Delete,
            "loop-running-id",
            "run123",
            LoopState::Running,
        ) {
            Ok(v) => v,
            Err(err) => panic!("delete confirm running: {err}"),
        };
        assert_eq!(
            running.prompt,
            "Loop is still running. Force delete record run123? [y/N]"
        );
    }

    #[test]
    fn resume_does_not_enter_confirm_mode() {
        let err = match enter_confirm(ActionKind::Resume, "loop", "", LoopState::Stopped) {
            Ok(_) => panic!("resume should not confirm"),
            Err(err) => err,
        };
        assert!(err.contains("does not require confirmation"), "{err}");
    }

    #[test]
    fn confirm_submit_builds_force_delete_request() {
        let running = match enter_confirm(
            ActionKind::Delete,
            "loop-running-id",
            "run123",
            LoopState::Running,
        ) {
            Ok(v) => v,
            Err(err) => panic!("confirm state: {err}"),
        };
        let request = confirm_to_request(&running);
        assert!(request.force_delete);

        let stopped = match enter_confirm(
            ActionKind::Delete,
            "loop-stopped-id",
            "stop12",
            LoopState::Stopped,
        ) {
            Ok(v) => v,
            Err(err) => panic!("confirm state: {err}"),
        };
        let request = confirm_to_request(&stopped);
        assert!(!request.force_delete);
    }

    #[test]
    fn confirm_key_handling_matches_model_behavior() {
        let confirm = match enter_confirm(
            ActionKind::Stop,
            "loop-running-id",
            "run123",
            LoopState::Running,
        ) {
            Ok(v) => v,
            Err(err) => panic!("confirm state: {err}"),
        };

        assert_eq!(
            handle_confirm_input(&confirm, "enter"),
            ConfirmInputResult::Cancelled
        );
        assert_eq!(
            handle_confirm_input(&confirm, "?"),
            ConfirmInputResult::OpenHelp
        );
        assert!(matches!(
            handle_confirm_input(&confirm, "y"),
            ConfirmInputResult::Submit(_)
        ));
        assert_eq!(
            handle_confirm_input(&confirm, "x"),
            ConfirmInputResult::Noop
        );
    }

    #[test]
    fn action_status_strings_match_go_text() {
        assert_eq!(action_status(ActionKind::Resume), "Resuming loop...");
        assert_eq!(
            action_status(ActionKind::Stop),
            "Requesting graceful stop..."
        );
        assert_eq!(action_status(ActionKind::Kill), "Killing loop...");
        assert_eq!(action_status(ActionKind::Delete), "Deleting loop record...");
    }

    fn sample_target() -> ActionTarget {
        ActionTarget {
            loop_id: "loop-a".to_owned(),
            short_id: "a123".to_owned(),
            loop_state: LoopState::Running,
            pool: "core".to_owned(),
            profile: "codex".to_owned(),
            tags: vec!["prod".to_owned(), "fleet".to_owned()],
            batch_size: 1,
        }
    }

    #[test]
    fn protected_pool_blocks_without_override_and_has_escalation_hint() {
        let policy = GuardrailPolicy {
            protected_pools: vec!["core".to_owned()],
            ..GuardrailPolicy::default()
        };
        let decision = evaluate_action_guardrail(ActionKind::Kill, &sample_target(), &policy, None);
        let GuardrailDecision::Blocked {
            reason,
            escalation_hint,
        } = decision
        else {
            panic!("expected block for protected pool");
        };
        assert!(reason.contains("policy-protected"));
        assert!(escalation_hint.contains("pool owner"));
    }

    #[test]
    fn protected_tag_override_creates_audit_entry() {
        let policy = GuardrailPolicy {
            protected_tags: vec!["prod".to_owned()],
            require_ticket_for_override: true,
            ..GuardrailPolicy::default()
        };
        let decision = evaluate_action_guardrail(
            ActionKind::Stop,
            &sample_target(),
            &policy,
            Some(&PolicyOverride {
                actor: "operator-a".to_owned(),
                reason: "incident containment for runaway task storm".to_owned(),
                ticket: Some("INC-2042".to_owned()),
                approved_by: Some("oncall-lead".to_owned()),
                timestamp_utc: "2026-02-12T11:00:00Z".to_owned(),
            }),
        );
        let GuardrailDecision::Allowed {
            confirm: Some(confirm),
            audit: Some(audit),
        } = decision
        else {
            panic!("expected allowed decision with confirmation + audit");
        };
        assert_eq!(confirm.action, ActionKind::Stop);
        assert_eq!(audit.policy_rule, "protected-tag");
        assert_eq!(audit.ticket.as_deref(), Some("INC-2042"));
        assert_eq!(audit.approved_by.as_deref(), Some("oncall-lead"));
    }

    #[test]
    fn override_requires_ticket_when_policy_demands_it() {
        let policy = GuardrailPolicy {
            protected_tags: vec!["prod".to_owned()],
            require_ticket_for_override: true,
            ..GuardrailPolicy::default()
        };
        let decision = evaluate_action_guardrail(
            ActionKind::Stop,
            &sample_target(),
            &policy,
            Some(&PolicyOverride {
                actor: "operator-a".to_owned(),
                reason: "incident containment for runaway task storm".to_owned(),
                ticket: None,
                approved_by: None,
                timestamp_utc: "2026-02-12T11:00:00Z".to_owned(),
            }),
        );
        let GuardrailDecision::Blocked { reason, .. } = decision else {
            panic!("expected override to fail without ticket");
        };
        assert!(reason.contains("requires tracking ticket"));
    }

    #[test]
    fn batch_threshold_blocks_without_override() {
        let mut target = sample_target();
        target.batch_size = 5;
        let policy = GuardrailPolicy {
            max_batch_without_override: 2,
            ..GuardrailPolicy::default()
        };
        let decision = evaluate_action_guardrail(ActionKind::Stop, &target, &policy, None);
        let GuardrailDecision::Blocked { reason, .. } = decision else {
            panic!("expected batch threshold block");
        };
        assert!(reason.contains("batch size 5 exceeds policy limit 2"));
    }

    #[test]
    fn destructive_action_without_block_returns_confirmation() {
        let target = sample_target();
        let decision = evaluate_action_guardrail(
            ActionKind::Delete,
            &target,
            &GuardrailPolicy::default(),
            None,
        );
        let GuardrailDecision::Allowed {
            confirm: Some(confirm),
            audit,
        } = decision
        else {
            panic!("expected allowed with confirmation");
        };
        assert!(confirm.prompt.contains("Force delete"));
        assert!(audit.is_none());
    }

    #[test]
    fn resume_is_allowed_without_confirmation() {
        let decision = evaluate_action_guardrail(
            ActionKind::Resume,
            &sample_target(),
            &GuardrailPolicy::default(),
            None,
        );
        let GuardrailDecision::Allowed { confirm, audit } = decision else {
            panic!("expected allowed resume");
        };
        assert!(confirm.is_none());
        assert!(audit.is_none());
    }

    #[test]
    fn override_reason_too_short_is_blocked() {
        let policy = GuardrailPolicy {
            protected_pools: vec!["core".to_owned()],
            min_override_reason_chars: 16,
            ..GuardrailPolicy::default()
        };
        let decision = evaluate_action_guardrail(
            ActionKind::Kill,
            &sample_target(),
            &policy,
            Some(&PolicyOverride {
                actor: "operator-a".to_owned(),
                reason: "urgent".to_owned(),
                ticket: Some("INC-9".to_owned()),
                approved_by: None,
                timestamp_utc: "2026-02-12T11:00:00Z".to_owned(),
            }),
        );
        let GuardrailDecision::Blocked {
            reason,
            escalation_hint,
        } = decision
        else {
            panic!("expected short-reason block");
        };
        assert!(reason.contains("reason too short"));
        assert!(escalation_hint.contains("impact/risk context"));
    }

    #[test]
    fn override_is_blocked_when_workspace_disables_it() {
        let policy = GuardrailPolicy {
            allow_overrides: false,
            protected_tags: vec!["prod".to_owned()],
            ..GuardrailPolicy::default()
        };
        let decision = evaluate_action_guardrail(
            ActionKind::Stop,
            &sample_target(),
            &policy,
            Some(&PolicyOverride {
                actor: "operator-a".to_owned(),
                reason: "incident containment for runaway task storm".to_owned(),
                ticket: Some("INC-2042".to_owned()),
                approved_by: None,
                timestamp_utc: "2026-02-12T11:00:00Z".to_owned(),
            }),
        );
        let GuardrailDecision::Blocked { reason, .. } = decision else {
            panic!("expected override denied block");
        };
        assert!(reason.contains("override denied"));
    }
}
