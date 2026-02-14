//! Keyboard macro recorder + runner model with reviewable safety checks.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardMacroStep {
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardMacroDraft {
    pub name: String,
    pub created_by: String,
    pub started_at_epoch_s: i64,
    pub steps: Vec<KeyboardMacroStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardMacroDefinition {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub started_at_epoch_s: i64,
    pub steps: Vec<KeyboardMacroStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardMacroPolicy {
    pub max_steps: usize,
    pub max_repeat: usize,
    pub blocked_steps: Vec<String>,
    pub destructive_tokens: Vec<String>,
}

impl Default for KeyboardMacroPolicy {
    fn default() -> Self {
        Self {
            max_steps: 64,
            max_repeat: 8,
            blocked_steps: vec!["ctrl+c".to_owned()],
            destructive_tokens: vec![
                "kill".to_owned(),
                "delete".to_owned(),
                "force".to_owned(),
                "rm".to_owned(),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroReviewSeverity {
    Warning,
    Blocker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroReviewIssue {
    pub severity: MacroReviewSeverity,
    pub step_index: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MacroReview {
    pub safe_to_run: bool,
    pub issues: Vec<MacroReviewIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroRunPlan {
    pub macro_id: String,
    pub repeat: usize,
    pub total_steps: usize,
    pub queued_steps: Vec<String>,
}

#[must_use]
pub fn start_macro_recording(
    name: &str,
    created_by: &str,
    started_at_epoch_s: i64,
) -> KeyboardMacroDraft {
    KeyboardMacroDraft {
        name: normalize_name(name),
        created_by: normalize_actor(created_by),
        started_at_epoch_s,
        steps: Vec::new(),
    }
}

pub fn append_macro_step(
    draft: &mut KeyboardMacroDraft,
    key: &str,
    policy: &KeyboardMacroPolicy,
) -> Result<(), String> {
    let normalized = normalize_required(key);
    if normalized.is_empty() {
        return Err("macro step key cannot be empty".to_owned());
    }

    if policy
        .blocked_steps
        .iter()
        .any(|blocked| normalize_required(blocked) == normalized)
    {
        return Err(format!("macro step '{key}' is blocked by policy"));
    }

    if draft.steps.len() >= policy.max_steps.max(1) {
        return Err(format!(
            "macro step limit reached ({})",
            policy.max_steps.max(1)
        ));
    }

    draft.steps.push(KeyboardMacroStep {
        key: key.trim().to_owned(),
    });
    Ok(())
}

pub fn finalize_macro_recording(
    draft: KeyboardMacroDraft,
) -> Result<KeyboardMacroDefinition, String> {
    if draft.steps.is_empty() {
        return Err("macro must include at least one step".to_owned());
    }

    let id = format!(
        "macro-{}",
        draft
            .name
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_ascii_lowercase()
    );

    Ok(KeyboardMacroDefinition {
        id,
        name: draft.name,
        created_by: draft.created_by,
        started_at_epoch_s: draft.started_at_epoch_s,
        steps: draft.steps,
    })
}

#[must_use]
pub fn review_macro_definition(
    definition: &KeyboardMacroDefinition,
    policy: &KeyboardMacroPolicy,
) -> MacroReview {
    let mut issues = Vec::new();

    if definition.steps.is_empty() {
        issues.push(MacroReviewIssue {
            severity: MacroReviewSeverity::Blocker,
            step_index: None,
            message: "macro has no steps".to_owned(),
        });
    }

    for (index, step) in definition.steps.iter().enumerate() {
        let normalized = normalize_required(&step.key);
        if normalized.is_empty() {
            issues.push(MacroReviewIssue {
                severity: MacroReviewSeverity::Blocker,
                step_index: Some(index),
                message: "empty step key".to_owned(),
            });
            continue;
        }

        if policy
            .blocked_steps
            .iter()
            .any(|blocked| normalize_required(blocked) == normalized)
        {
            issues.push(MacroReviewIssue {
                severity: MacroReviewSeverity::Blocker,
                step_index: Some(index),
                message: format!("blocked step '{}': review policy", step.key),
            });
        }

        if policy
            .destructive_tokens
            .iter()
            .any(|token| normalized.contains(&normalize_required(token)))
        {
            issues.push(MacroReviewIssue {
                severity: MacroReviewSeverity::Warning,
                step_index: Some(index),
                message: format!(
                    "destructive token in step '{}': requires operator review",
                    step.key
                ),
            });
        }
    }

    let safe_to_run = !issues
        .iter()
        .any(|issue| issue.severity == MacroReviewSeverity::Blocker);

    MacroReview {
        safe_to_run,
        issues,
    }
}

pub fn plan_macro_run(
    definition: &KeyboardMacroDefinition,
    repeat: usize,
    policy: &KeyboardMacroPolicy,
) -> Result<MacroRunPlan, String> {
    let repeat = repeat.max(1);
    if repeat > policy.max_repeat.max(1) {
        return Err(format!(
            "macro repeat {} exceeds policy limit {}",
            repeat,
            policy.max_repeat.max(1)
        ));
    }

    let review = review_macro_definition(definition, policy);
    if !review.safe_to_run {
        return Err("macro has blocking safety issues; review before run".to_owned());
    }

    let mut queued_steps = Vec::new();
    for _ in 0..repeat {
        for step in &definition.steps {
            queued_steps.push(step.key.clone());
        }
    }

    Ok(MacroRunPlan {
        macro_id: definition.id.clone(),
        repeat,
        total_steps: queued_steps.len(),
        queued_steps,
    })
}

#[must_use]
pub fn render_macro_definition(definition: &KeyboardMacroDefinition) -> Vec<String> {
    let mut lines = vec![format!(
        "macro {} by {} (steps={})",
        definition.name,
        definition.created_by,
        definition.steps.len()
    )];

    for (index, step) in definition.steps.iter().enumerate() {
        lines.push(format!("{:02}. {}", index + 1, step.key.trim()));
    }

    lines
}

fn normalize_required(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_name(value: &str) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        "macro".to_owned()
    } else {
        normalized.to_owned()
    }
}

fn normalize_actor(value: &str) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        "unknown".to_owned()
    } else {
        normalized.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_macro_step, finalize_macro_recording, plan_macro_run, render_macro_definition,
        review_macro_definition, start_macro_recording, KeyboardMacroPolicy, MacroReviewSeverity,
    };

    #[test]
    fn record_and_finalize_keeps_step_order() {
        let policy = KeyboardMacroPolicy::default();
        let mut draft = start_macro_recording("triage", "agent-a", 100);
        if let Err(err) = append_macro_step(&mut draft, "j", &policy) {
            panic!("step j should append: {err}");
        }
        if let Err(err) = append_macro_step(&mut draft, "k", &policy) {
            panic!("step k should append: {err}");
        }

        let definition = match finalize_macro_recording(draft) {
            Ok(definition) => definition,
            Err(err) => panic!("finalize should succeed: {err}"),
        };
        assert_eq!(definition.steps[0].key, "j");
        assert_eq!(definition.steps[1].key, "k");
        assert!(definition.id.starts_with("macro-"));
    }

    #[test]
    fn blocked_step_is_rejected_during_recording() {
        let mut policy = KeyboardMacroPolicy::default();
        policy.blocked_steps.push("q".to_owned());
        let mut draft = start_macro_recording("triage", "agent-a", 100);

        let err = match append_macro_step(&mut draft, "q", &policy) {
            Ok(()) => panic!("blocked key must fail"),
            Err(err) => err,
        };
        assert!(err.contains("blocked"));
    }

    #[test]
    fn review_flags_destructive_steps_as_warnings() {
        let policy = KeyboardMacroPolicy::default();
        let mut draft = start_macro_recording("cleanup", "agent-a", 100);
        if let Err(err) = append_macro_step(&mut draft, "open logs", &policy) {
            panic!("logs step should append: {err}");
        }
        if let Err(err) = append_macro_step(&mut draft, "delete stale loop", &policy) {
            panic!("delete step should append: {err}");
        }

        let definition = match finalize_macro_recording(draft) {
            Ok(definition) => definition,
            Err(err) => panic!("finalize should succeed: {err}"),
        };
        let review = review_macro_definition(&definition, &policy);
        assert!(review.safe_to_run);
        assert!(review.issues.iter().any(|issue| {
            issue.severity == MacroReviewSeverity::Warning
                && issue.message.contains("destructive token")
        }));
    }

    #[test]
    fn run_plan_enforces_repeat_limit() {
        let policy = KeyboardMacroPolicy {
            max_repeat: 2,
            ..KeyboardMacroPolicy::default()
        };
        let mut draft = start_macro_recording("triage", "agent-a", 100);
        if let Err(err) = append_macro_step(&mut draft, "j", &policy) {
            panic!("step should append: {err}");
        }
        let definition = match finalize_macro_recording(draft) {
            Ok(definition) => definition,
            Err(err) => panic!("finalize should succeed: {err}"),
        };

        let err = match plan_macro_run(&definition, 3, &policy) {
            Ok(_) => panic!("repeat over limit should fail"),
            Err(err) => err,
        };
        assert!(err.contains("exceeds policy limit"));
    }

    #[test]
    fn run_plan_expands_steps_for_repeat_count() {
        let policy = KeyboardMacroPolicy::default();
        let mut draft = start_macro_recording("triage", "agent-a", 100);
        if let Err(err) = append_macro_step(&mut draft, "j", &policy) {
            panic!("step1 should append: {err}");
        }
        if let Err(err) = append_macro_step(&mut draft, "k", &policy) {
            panic!("step2 should append: {err}");
        }
        let definition = match finalize_macro_recording(draft) {
            Ok(definition) => definition,
            Err(err) => panic!("finalize should succeed: {err}"),
        };

        let plan = match plan_macro_run(&definition, 2, &policy) {
            Ok(plan) => plan,
            Err(err) => panic!("plan should succeed: {err}"),
        };
        assert_eq!(plan.total_steps, 4);
        assert_eq!(plan.queued_steps, vec!["j", "k", "j", "k"]);
    }

    #[test]
    fn render_definition_is_reviewable_and_deterministic() {
        let policy = KeyboardMacroPolicy::default();
        let mut draft = start_macro_recording("triage", "agent-a", 100);
        if let Err(err) = append_macro_step(&mut draft, "j", &policy) {
            panic!("step1 should append: {err}");
        }
        if let Err(err) = append_macro_step(&mut draft, "open logs", &policy) {
            panic!("step2 should append: {err}");
        }
        let definition = match finalize_macro_recording(draft) {
            Ok(definition) => definition,
            Err(err) => panic!("finalize should succeed: {err}"),
        };

        let lines = render_macro_definition(&definition);
        assert_eq!(lines[0], "macro triage by agent-a (steps=2)");
        assert_eq!(lines[1], "01. j");
        assert_eq!(lines[2], "02. open logs");
    }
}
