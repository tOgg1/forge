//! Operator runbook engine with lightweight TOML/YAML manifest parsing.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunbookStep {
    pub step_id: String,
    pub title: String,
    pub check: String,
    pub action: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunbookDefinition {
    pub runbook_id: String,
    pub title: String,
    pub description: String,
    pub steps: Vec<RunbookStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunbookStepStatus {
    Pending,
    Done,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunbookRunStep {
    pub step_id: String,
    pub title: String,
    pub check: String,
    pub action: String,
    pub required: bool,
    pub status: RunbookStepStatus,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunbookRun {
    pub runbook_id: String,
    pub title: String,
    pub description: String,
    pub steps: Vec<RunbookRunStep>,
    pub selected_step: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunbookStepAction {
    MarkDone,
    Skip,
    Reset,
}

#[must_use]
pub fn builtin_runbooks() -> Vec<RunbookDefinition> {
    vec![
        RunbookDefinition {
            runbook_id: "shift-handoff".to_owned(),
            title: "Shift handoff".to_owned(),
            description: "Transfer context safely to the next operator.".to_owned(),
            steps: vec![
                step(
                    "review-alerts",
                    "Review active alerts",
                    "scan alert rail and unresolved incidents",
                    "jump alerts panel",
                    true,
                ),
                step(
                    "sync-fmail",
                    "Sync fmail inbox",
                    "check unanswered asks and stale threads",
                    "open inbox and send heartbeat replies",
                    true,
                ),
                step(
                    "publish-summary",
                    "Publish handoff summary",
                    "capture top blockers and ownership",
                    "send shift summary via fmail",
                    true,
                ),
            ],
        },
        RunbookDefinition {
            runbook_id: "incident-response".to_owned(),
            title: "Incident response".to_owned(),
            description: "Stabilize fleet and isolate root cause quickly.".to_owned(),
            steps: vec![
                step(
                    "identify-blast-radius",
                    "Identify blast radius",
                    "list impacted loops and services",
                    "open multi-loop compare",
                    true,
                ),
                step(
                    "collect-evidence",
                    "Collect evidence",
                    "capture logs, stack traces, and recent commits",
                    "export focused evidence bundle",
                    true,
                ),
                step(
                    "mitigate",
                    "Apply mitigation",
                    "choose safe stop/rollback action",
                    "execute mitigation command",
                    true,
                ),
            ],
        },
        RunbookDefinition {
            runbook_id: "fleet-scale-up".to_owned(),
            title: "Fleet scale-up".to_owned(),
            description: "Expand active loop capacity without overload.".to_owned(),
            steps: vec![
                step(
                    "check-budget",
                    "Check budget guardrails",
                    "confirm headroom and policy limits",
                    "open budget guardrails panel",
                    true,
                ),
                step(
                    "stage-ramp",
                    "Stage ramp",
                    "define incremental worker increase",
                    "start controlled ramp wizard",
                    true,
                ),
                step(
                    "verify-health",
                    "Verify health post-ramp",
                    "watch error rates and queue latency",
                    "open analytics + logs",
                    true,
                ),
            ],
        },
        RunbookDefinition {
            runbook_id: "graceful-shutdown".to_owned(),
            title: "Graceful shutdown".to_owned(),
            description: "Drain work and stop loops safely.".to_owned(),
            steps: vec![
                step(
                    "freeze-new-work",
                    "Freeze new work",
                    "pause queue ingestion",
                    "trigger safe-stop all",
                    true,
                ),
                step(
                    "drain-active-runs",
                    "Drain active runs",
                    "wait for in-flight runs to finish",
                    "monitor run timeline",
                    true,
                ),
                step(
                    "confirm-zero-queue",
                    "Confirm zero queue",
                    "verify no pending tasks",
                    "review readiness board",
                    true,
                ),
            ],
        },
    ]
}

#[must_use]
pub fn start_runbook(definition: &RunbookDefinition) -> RunbookRun {
    let steps = definition
        .steps
        .iter()
        .map(|step| RunbookRunStep {
            step_id: normalize_id(&step.step_id),
            title: normalize_text(&step.title),
            check: normalize_text(&step.check),
            action: normalize_text(&step.action),
            required: step.required,
            status: RunbookStepStatus::Pending,
            note: String::new(),
        })
        .collect::<Vec<_>>();

    RunbookRun {
        runbook_id: normalize_id(&definition.runbook_id),
        title: normalize_text(&definition.title),
        description: normalize_text(&definition.description),
        selected_step: 0,
        steps,
    }
}

pub fn apply_runbook_step_action(
    run: &mut RunbookRun,
    step_id: &str,
    action: RunbookStepAction,
    note: Option<&str>,
) -> Result<(), String> {
    let step_id = normalize_id(step_id);
    let Some(index) = run.steps.iter().position(|step| step.step_id == step_id) else {
        return Err(format!("step not found: {step_id}"));
    };

    let step = &mut run.steps[index];
    step.status = match action {
        RunbookStepAction::MarkDone => RunbookStepStatus::Done,
        RunbookStepAction::Skip => RunbookStepStatus::Skipped,
        RunbookStepAction::Reset => RunbookStepStatus::Pending,
    };
    if let Some(note) = note {
        step.note = normalize_text(note);
    }

    run.selected_step = next_recommended_step_index(run).unwrap_or(index);
    Ok(())
}

#[must_use]
pub fn next_recommended_step_index(run: &RunbookRun) -> Option<usize> {
    run.steps
        .iter()
        .position(|step| step.status == RunbookStepStatus::Pending)
}

#[must_use]
pub fn render_runbook_lines(run: &RunbookRun, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let done = run
        .steps
        .iter()
        .filter(|step| step.status == RunbookStepStatus::Done)
        .count();
    let skipped = run
        .steps
        .iter()
        .filter(|step| step.status == RunbookStepStatus::Skipped)
        .count();
    let total = run.steps.len();

    let mut lines = Vec::new();
    lines.push(fit_width(
        &format!("Runbook {} ({})", run.title, run.runbook_id),
        width,
    ));
    lines.push(fit_width(
        &format!(
            "progress: done={} skipped={} total={}",
            done, skipped, total
        ),
        width,
    ));
    lines.push(fit_width(&run.description, width));

    for (index, step) in run.steps.iter().enumerate() {
        if lines.len() >= max_lines {
            break;
        }
        let marker = if index == run.selected_step { ">" } else { " " };
        let status = match step.status {
            RunbookStepStatus::Pending => "pending",
            RunbookStepStatus::Done => "done",
            RunbookStepStatus::Skipped => "skipped",
        };
        let required = if step.required {
            "required"
        } else {
            "optional"
        };
        lines.push(fit_width(
            &format!(
                "{} [{}] {} ({}) - check:{} action:{}",
                marker, status, step.title, required, step.check, step.action
            ),
            width,
        ));
        if !step.note.is_empty() && lines.len() < max_lines {
            lines.push(fit_width(&format!("    note: {}", step.note), width));
        }
    }

    lines.into_iter().take(max_lines).collect()
}

pub fn parse_runbook_manifest(input: &str) -> Result<RunbookDefinition, String> {
    if input.trim().is_empty() {
        return Err("manifest is empty".to_owned());
    }
    if input.contains("[[steps]]") || input.contains('=') {
        parse_toml_like_manifest(input)
    } else {
        parse_yaml_like_manifest(input)
    }
}

fn parse_toml_like_manifest(input: &str) -> Result<RunbookDefinition, String> {
    let mut runbook_id = String::new();
    let mut title = String::new();
    let mut description = String::new();
    let mut steps = Vec::<RunbookStep>::new();

    let mut step_id = String::new();
    let mut step_title = String::new();
    let mut step_check = String::new();
    let mut step_action = String::new();
    let mut step_required = true;
    let mut in_step = false;

    let flush_step = |steps: &mut Vec<RunbookStep>,
                      step_id: &mut String,
                      step_title: &mut String,
                      step_check: &mut String,
                      step_action: &mut String,
                      step_required: &mut bool,
                      in_step: &mut bool|
     -> Result<(), String> {
        if !*in_step {
            return Ok(());
        }
        let id = normalize_id(step_id);
        if id.is_empty() {
            return Err("step.id missing in TOML manifest".to_owned());
        }
        steps.push(RunbookStep {
            step_id: id,
            title: normalize_text(step_title),
            check: normalize_text(step_check),
            action: normalize_text(step_action),
            required: *step_required,
        });
        step_id.clear();
        step_title.clear();
        step_check.clear();
        step_action.clear();
        *step_required = true;
        *in_step = false;
        Ok(())
    };

    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[steps]]" {
            flush_step(
                &mut steps,
                &mut step_id,
                &mut step_title,
                &mut step_check,
                &mut step_action,
                &mut step_required,
                &mut in_step,
            )?;
            in_step = true;
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = normalize_id(key);
        let value = strip_quotes(value.trim());

        if in_step {
            match key.as_str() {
                "id" => step_id = value,
                "title" => step_title = value,
                "check" => step_check = value,
                "action" => step_action = value,
                "required" => step_required = parse_bool(&value).unwrap_or(true),
                _ => {}
            }
        } else {
            match key.as_str() {
                "id" => runbook_id = value,
                "title" => title = value,
                "description" => description = value,
                _ => {}
            }
        }
    }

    flush_step(
        &mut steps,
        &mut step_id,
        &mut step_title,
        &mut step_check,
        &mut step_action,
        &mut step_required,
        &mut in_step,
    )?;

    validate_manifest(runbook_id, title, description, steps)
}

fn parse_yaml_like_manifest(input: &str) -> Result<RunbookDefinition, String> {
    let mut runbook_id = String::new();
    let mut title = String::new();
    let mut description = String::new();
    let mut steps = Vec::<RunbookStep>::new();

    let mut in_steps = false;
    let mut step_id = String::new();
    let mut step_title = String::new();
    let mut step_check = String::new();
    let mut step_action = String::new();
    let mut step_required = true;
    let mut has_open_step = false;

    let flush_step = |steps: &mut Vec<RunbookStep>,
                      step_id: &mut String,
                      step_title: &mut String,
                      step_check: &mut String,
                      step_action: &mut String,
                      step_required: &mut bool,
                      has_open_step: &mut bool|
     -> Result<(), String> {
        if !*has_open_step {
            return Ok(());
        }
        let id = normalize_id(step_id);
        if id.is_empty() {
            return Err("step.id missing in YAML manifest".to_owned());
        }
        steps.push(RunbookStep {
            step_id: id,
            title: normalize_text(step_title),
            check: normalize_text(step_check),
            action: normalize_text(step_action),
            required: *step_required,
        });
        step_id.clear();
        step_title.clear();
        step_check.clear();
        step_action.clear();
        *step_required = true;
        *has_open_step = false;
        Ok(())
    };

    for raw in input.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed == "steps:" {
            in_steps = true;
            continue;
        }

        if !in_steps {
            if let Some((key, value)) = trimmed.split_once(':') {
                let key = normalize_id(key);
                let value = strip_quotes(value.trim());
                match key.as_str() {
                    "id" => runbook_id = value,
                    "title" => title = value,
                    "description" => description = value,
                    _ => {}
                }
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix('-') {
            flush_step(
                &mut steps,
                &mut step_id,
                &mut step_title,
                &mut step_check,
                &mut step_action,
                &mut step_required,
                &mut has_open_step,
            )?;
            has_open_step = true;
            if let Some((key, value)) = rest.trim().split_once(':') {
                assign_step_field(
                    normalize_id(key).as_str(),
                    strip_quotes(value.trim()),
                    &mut step_id,
                    &mut step_title,
                    &mut step_check,
                    &mut step_action,
                    &mut step_required,
                );
            }
            continue;
        }

        if let Some((key, value)) = trimmed.split_once(':') {
            has_open_step = true;
            assign_step_field(
                normalize_id(key).as_str(),
                strip_quotes(value.trim()),
                &mut step_id,
                &mut step_title,
                &mut step_check,
                &mut step_action,
                &mut step_required,
            );
        }
    }

    flush_step(
        &mut steps,
        &mut step_id,
        &mut step_title,
        &mut step_check,
        &mut step_action,
        &mut step_required,
        &mut has_open_step,
    )?;

    validate_manifest(runbook_id, title, description, steps)
}

fn assign_step_field(
    key: &str,
    value: String,
    step_id: &mut String,
    step_title: &mut String,
    step_check: &mut String,
    step_action: &mut String,
    step_required: &mut bool,
) {
    match key {
        "id" => *step_id = value,
        "title" => *step_title = value,
        "check" => *step_check = value,
        "action" => *step_action = value,
        "required" => *step_required = parse_bool(&value).unwrap_or(true),
        _ => {}
    }
}

fn validate_manifest(
    runbook_id: String,
    title: String,
    description: String,
    mut steps: Vec<RunbookStep>,
) -> Result<RunbookDefinition, String> {
    let runbook_id = normalize_id(&runbook_id);
    if runbook_id.is_empty() {
        return Err("runbook.id is required".to_owned());
    }
    if steps.is_empty() {
        return Err("runbook.steps must contain at least one step".to_owned());
    }

    for step in &mut steps {
        if step.title.is_empty() {
            step.title = step.step_id.clone();
        }
    }

    Ok(RunbookDefinition {
        runbook_id,
        title: normalize_text(&title),
        description: normalize_text(&description),
        steps,
    })
}

fn step(step_id: &str, title: &str, check: &str, action: &str, required: bool) -> RunbookStep {
    RunbookStep {
        step_id: normalize_id(step_id),
        title: normalize_text(title),
        check: normalize_text(check),
        action: normalize_text(action),
        required,
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match normalize_id(value).as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn strip_quotes(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
        {
            return normalize_text(&trimmed[1..trimmed.len() - 1]);
        }
    }
    normalize_text(trimmed)
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fit_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let char_count = value.chars().count();
    if char_count <= width {
        return value.to_owned();
    }
    if width <= 3 {
        return value.chars().take(width).collect();
    }
    let mut out: String = value.chars().take(width - 3).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::{
        apply_runbook_step_action, builtin_runbooks, next_recommended_step_index,
        parse_runbook_manifest, render_runbook_lines, start_runbook, RunbookStepAction,
        RunbookStepStatus,
    };

    #[test]
    fn parse_toml_manifest() {
        let manifest = r#"
id = "fleet-health"
title = "Fleet health"
description = "Check fleet integrity"
[[steps]]
id = "scan"
title = "Scan degraded loops"
check = "review error rates"
action = "open overview"
required = true
[[steps]]
id = "budget"
title = "Budget guardrail"
check = "check budget"
action = "open budget"
required = false
"#;

        let runbook = match parse_runbook_manifest(manifest) {
            Ok(runbook) => runbook,
            Err(err) => panic!("toml parse should succeed: {err}"),
        };
        assert_eq!(runbook.runbook_id, "fleet-health");
        assert_eq!(runbook.steps.len(), 2);
        assert_eq!(runbook.steps[0].step_id, "scan");
        assert!(!runbook.steps[1].required);
    }

    #[test]
    fn parse_yaml_manifest() {
        let manifest = r#"
id: shift-handoff
title: Shift Handoff
description: transfer safely
steps:
  - id: alerts
    title: Review alerts
    check: scan alerts
    action: open alert rail
    required: true
  - id: summary
    title: Publish summary
    check: collect blockers
    action: send handoff
    required: false
"#;

        let runbook = match parse_runbook_manifest(manifest) {
            Ok(runbook) => runbook,
            Err(err) => panic!("yaml parse should succeed: {err}"),
        };
        assert_eq!(runbook.runbook_id, "shift-handoff");
        assert_eq!(runbook.steps.len(), 2);
        assert_eq!(runbook.steps[1].step_id, "summary");
        assert!(!runbook.steps[1].required);
    }

    #[test]
    fn builtin_runbooks_include_required_operator_flows() {
        let runbooks = builtin_runbooks();
        let ids = runbooks
            .iter()
            .map(|item| item.runbook_id.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"shift-handoff"));
        assert!(ids.contains(&"incident-response"));
        assert!(ids.contains(&"fleet-scale-up"));
        assert!(ids.contains(&"graceful-shutdown"));
    }

    #[test]
    fn runbook_progression_moves_to_next_pending_step() {
        let runbook = builtin_runbooks()
            .into_iter()
            .find(|item| item.runbook_id == "incident-response")
            .unwrap_or_else(|| panic!("incident-response runbook should exist"));
        let mut run = start_runbook(&runbook);
        assert_eq!(run.selected_step, 0);

        if let Err(err) = apply_runbook_step_action(
            &mut run,
            "identify-blast-radius",
            RunbookStepAction::MarkDone,
            Some("impact mapped"),
        ) {
            panic!("apply action should succeed: {err}");
        }

        assert_eq!(run.steps[0].status, RunbookStepStatus::Done);
        assert_eq!(run.selected_step, 1);
        assert_eq!(next_recommended_step_index(&run), Some(1));
    }

    #[test]
    fn render_runbook_lines_contains_progress_and_step_details() {
        let runbook = builtin_runbooks()
            .into_iter()
            .find(|item| item.runbook_id == "shift-handoff")
            .unwrap_or_else(|| panic!("shift-handoff runbook should exist"));
        let mut run = start_runbook(&runbook);
        if let Err(err) = apply_runbook_step_action(
            &mut run,
            "review-alerts",
            RunbookStepAction::MarkDone,
            Some("alerts triaged"),
        ) {
            panic!("apply should succeed: {err}");
        }

        let lines = render_runbook_lines(&run, 140, 12);
        assert!(lines
            .iter()
            .any(|line| line.contains("Runbook Shift handoff")));
        assert!(lines.iter().any(|line| line.contains("progress:")));
        assert!(lines.iter().any(|line| line.contains("alerts triaged")));
    }
}
