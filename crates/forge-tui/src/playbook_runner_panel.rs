//! Guided incident playbook runner state + panel rendering.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybookStepStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybookTemplateStep {
    pub step_id: String,
    pub title: String,
    pub instructions: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybookTemplate {
    pub playbook_id: String,
    pub title: String,
    pub steps: Vec<PlaybookTemplateStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybookRunStep {
    pub step_id: String,
    pub title: String,
    pub instructions: String,
    pub depends_on: Vec<String>,
    pub status: PlaybookStepStatus,
    pub started_at_epoch_s: Option<i64>,
    pub finished_at_epoch_s: Option<i64>,
    pub last_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybookRunState {
    pub playbook_id: String,
    pub title: String,
    pub steps: Vec<PlaybookRunStep>,
    pub selected_step: usize,
    pub started_at_epoch_s: i64,
    pub updated_at_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybookProgress {
    pub total_steps: usize,
    pub done_steps: usize,
    pub in_progress_steps: usize,
    pub blocked_steps: usize,
    pub skipped_steps: usize,
    pub pending_steps: usize,
    pub completion_percent: u8,
    pub next_step_id: Option<String>,
}

#[must_use]
pub fn start_playbook_run(template: &PlaybookTemplate, now_epoch_s: i64) -> PlaybookRunState {
    let now_epoch_s = now_epoch_s.max(0);
    let mut run = PlaybookRunState {
        playbook_id: normalize_id(&template.playbook_id),
        title: normalize_title(&template.title, &template.playbook_id),
        steps: template
            .steps
            .iter()
            .map(|step| PlaybookRunStep {
                step_id: normalize_id(&step.step_id),
                title: normalize_title(&step.title, &step.step_id),
                instructions: step.instructions.trim().to_owned(),
                depends_on: step
                    .depends_on
                    .iter()
                    .map(|dep| normalize_id(dep))
                    .collect(),
                status: PlaybookStepStatus::Pending,
                started_at_epoch_s: None,
                finished_at_epoch_s: None,
                last_note: String::new(),
            })
            .collect(),
        selected_step: 0,
        started_at_epoch_s: now_epoch_s,
        updated_at_epoch_s: now_epoch_s,
    };

    if let Some(index) = promote_next_ready_step(&mut run, now_epoch_s) {
        run.selected_step = index;
    }
    run
}

pub fn update_playbook_step(
    run: &mut PlaybookRunState,
    step_id: &str,
    next_status: PlaybookStepStatus,
    note: Option<&str>,
    now_epoch_s: i64,
) -> Result<(), String> {
    let step_id = normalize_id(step_id);
    if step_id.is_empty() {
        return Err("step_id is required".to_owned());
    }
    let now_epoch_s = now_epoch_s.max(0);

    let Some(index) = run.steps.iter().position(|step| step.step_id == step_id) else {
        return Err(format!("step {:?} not found in playbook run", step_id));
    };

    let dependencies_ready = dependencies_done(run, index);
    if matches!(
        next_status,
        PlaybookStepStatus::InProgress | PlaybookStepStatus::Done
    ) && !dependencies_ready
    {
        return Err(format!(
            "step {:?} dependencies are not complete",
            run.steps[index].step_id
        ));
    }

    let step = &mut run.steps[index];
    step.status = next_status;
    if let Some(note) = note {
        step.last_note = note.trim().to_owned();
    }

    match next_status {
        PlaybookStepStatus::Pending => {
            step.finished_at_epoch_s = None;
        }
        PlaybookStepStatus::InProgress => {
            if step.started_at_epoch_s.is_none() {
                step.started_at_epoch_s = Some(now_epoch_s);
            }
            step.finished_at_epoch_s = None;
        }
        PlaybookStepStatus::Done | PlaybookStepStatus::Skipped => {
            if step.started_at_epoch_s.is_none() {
                step.started_at_epoch_s = Some(now_epoch_s);
            }
            step.finished_at_epoch_s = Some(now_epoch_s);
        }
        PlaybookStepStatus::Blocked => {
            if step.started_at_epoch_s.is_none() {
                step.started_at_epoch_s = Some(now_epoch_s);
            }
            step.finished_at_epoch_s = None;
        }
    }

    run.updated_at_epoch_s = now_epoch_s;
    run.selected_step = index;

    if !run
        .steps
        .iter()
        .any(|step| step.status == PlaybookStepStatus::InProgress)
    {
        if let Some(next_index) = promote_next_ready_step(run, now_epoch_s) {
            run.selected_step = next_index;
        }
    }

    Ok(())
}

#[must_use]
pub fn compute_playbook_progress(run: &PlaybookRunState) -> PlaybookProgress {
    let total_steps = run.steps.len();
    let done_steps = run
        .steps
        .iter()
        .filter(|step| step.status == PlaybookStepStatus::Done)
        .count();
    let in_progress_steps = run
        .steps
        .iter()
        .filter(|step| step.status == PlaybookStepStatus::InProgress)
        .count();
    let blocked_steps = run
        .steps
        .iter()
        .filter(|step| step.status == PlaybookStepStatus::Blocked)
        .count();
    let skipped_steps = run
        .steps
        .iter()
        .filter(|step| step.status == PlaybookStepStatus::Skipped)
        .count();
    let pending_steps = total_steps.saturating_sub(
        done_steps
            .saturating_add(in_progress_steps)
            .saturating_add(blocked_steps)
            .saturating_add(skipped_steps),
    );
    let completed_count = done_steps.saturating_add(skipped_steps);
    let completion_percent = if total_steps == 0 {
        0
    } else {
        ((completed_count * 100) / total_steps) as u8
    };

    let next_step_id = run
        .steps
        .iter()
        .enumerate()
        .find(|(index, step)| {
            step.status == PlaybookStepStatus::Pending && dependencies_done(run, *index)
        })
        .map(|(_, step)| step.step_id.clone());

    PlaybookProgress {
        total_steps,
        done_steps,
        in_progress_steps,
        blocked_steps,
        skipped_steps,
        pending_steps,
        completion_percent,
        next_step_id,
    }
}

#[must_use]
pub fn render_playbook_panel_lines(
    run: &PlaybookRunState,
    width: usize,
    height: usize,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let progress = compute_playbook_progress(run);
    let mut lines = vec![
        fit_width(&format!("PLAYBOOK {}", run.title), width),
        fit_width(
            &format!(
                "progress:{}% done:{} in-progress:{} blocked:{} pending:{}",
                progress.completion_percent,
                progress.done_steps,
                progress.in_progress_steps,
                progress.blocked_steps,
                progress.pending_steps
            ),
            width,
        ),
        fit_width(
            &format!(
                "next:{}",
                progress.next_step_id.as_deref().unwrap_or("none")
            ),
            width,
        ),
    ];

    for (index, step) in run.steps.iter().enumerate() {
        if lines.len() >= height {
            break;
        }
        let selected_marker = if index == run.selected_step { ">" } else { " " };
        let blocked_by_dep =
            step.status == PlaybookStepStatus::Pending && !dependencies_done(run, index);
        let mut line = format!(
            "{selected_marker}{} {}",
            step_status_marker(step.status),
            step.title
        );
        if blocked_by_dep {
            line.push_str(" [deps]");
        }
        if !step.last_note.trim().is_empty() {
            line.push_str(&format!(" ({})", step.last_note.trim()));
        }
        lines.push(fit_width(&line, width));
    }

    lines.truncate(height);
    lines
}

fn promote_next_ready_step(run: &mut PlaybookRunState, now_epoch_s: i64) -> Option<usize> {
    for index in 0..run.steps.len() {
        if run.steps[index].status != PlaybookStepStatus::Pending {
            continue;
        }
        if !dependencies_done(run, index) {
            continue;
        }
        run.steps[index].status = PlaybookStepStatus::InProgress;
        if run.steps[index].started_at_epoch_s.is_none() {
            run.steps[index].started_at_epoch_s = Some(now_epoch_s);
        }
        return Some(index);
    }
    None
}

fn dependencies_done(run: &PlaybookRunState, index: usize) -> bool {
    let Some(step) = run.steps.get(index) else {
        return false;
    };
    if step.depends_on.is_empty() {
        return true;
    }

    step.depends_on.iter().all(|dependency| {
        run.steps
            .iter()
            .find(|candidate| candidate.step_id == *dependency)
            .is_some_and(|candidate| candidate.status == PlaybookStepStatus::Done)
    })
}

fn step_status_marker(status: PlaybookStepStatus) -> &'static str {
    match status {
        PlaybookStepStatus::Pending => "[ ]",
        PlaybookStepStatus::InProgress => "[>]",
        PlaybookStepStatus::Done => "[x]",
        PlaybookStepStatus::Blocked => "[!]",
        PlaybookStepStatus::Skipped => "[-]",
    }
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_title(title: &str, fallback: &str) -> String {
    let normalized = title.trim();
    if normalized.is_empty() {
        fallback.trim().to_owned()
    } else {
        normalized.to_owned()
    }
}

fn fit_width(value: &str, width: usize) -> String {
    if value.len() <= width {
        value.to_owned()
    } else {
        value.chars().take(width).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compute_playbook_progress, render_playbook_panel_lines, start_playbook_run,
        update_playbook_step, PlaybookStepStatus, PlaybookTemplate, PlaybookTemplateStep,
    };

    fn template() -> PlaybookTemplate {
        PlaybookTemplate {
            playbook_id: "incident-1".to_owned(),
            title: "Incident Mitigation".to_owned(),
            steps: vec![
                PlaybookTemplateStep {
                    step_id: "triage".to_owned(),
                    title: "Triage incident".to_owned(),
                    instructions: "Collect blast radius".to_owned(),
                    depends_on: Vec::new(),
                },
                PlaybookTemplateStep {
                    step_id: "stabilize".to_owned(),
                    title: "Stabilize service".to_owned(),
                    instructions: "Apply safe rollback".to_owned(),
                    depends_on: vec!["triage".to_owned()],
                },
                PlaybookTemplateStep {
                    step_id: "verify".to_owned(),
                    title: "Verify recovery".to_owned(),
                    instructions: "Check SLO/alerts".to_owned(),
                    depends_on: vec!["stabilize".to_owned()],
                },
            ],
        }
    }

    #[test]
    fn run_initializes_with_first_step_in_progress() {
        let run = start_playbook_run(&template(), 1_000);
        assert_eq!(run.steps[0].status, PlaybookStepStatus::InProgress);
        assert_eq!(run.selected_step, 0);
        assert_eq!(run.steps[1].status, PlaybookStepStatus::Pending);
    }

    #[test]
    fn completing_step_promotes_next_ready_step() {
        let mut run = start_playbook_run(&template(), 1_000);
        if let Err(err) = update_playbook_step(
            &mut run,
            "triage",
            PlaybookStepStatus::Done,
            Some("triage complete"),
            1_020,
        ) {
            panic!("update triage: {err}");
        }
        assert_eq!(run.steps[0].status, PlaybookStepStatus::Done);
        assert_eq!(run.steps[1].status, PlaybookStepStatus::InProgress);
    }

    #[test]
    fn blocks_transition_when_dependencies_missing() {
        let mut run = start_playbook_run(&template(), 1_000);
        let err = match update_playbook_step(
            &mut run,
            "verify",
            PlaybookStepStatus::InProgress,
            None,
            1_010,
        ) {
            Ok(_) => panic!("verify should be blocked until stabilize done"),
            Err(err) => err,
        };
        assert!(err.contains("dependencies are not complete"));
    }

    #[test]
    fn progress_counts_reflect_statuses() {
        let mut run = start_playbook_run(&template(), 1_000);
        let _ = update_playbook_step(&mut run, "triage", PlaybookStepStatus::Done, None, 1_010);
        let _ = update_playbook_step(
            &mut run,
            "stabilize",
            PlaybookStepStatus::Blocked,
            Some("waiting for approval"),
            1_020,
        );
        let progress = compute_playbook_progress(&run);
        assert_eq!(progress.total_steps, 3);
        assert_eq!(progress.done_steps, 1);
        assert_eq!(progress.blocked_steps, 1);
        assert_eq!(progress.pending_steps, 1);
        assert_eq!(progress.completion_percent, 33);
    }

    #[test]
    fn render_includes_markers_and_notes() {
        let mut run = start_playbook_run(&template(), 1_000);
        let _ = update_playbook_step(
            &mut run,
            "triage",
            PlaybookStepStatus::Done,
            Some("scope set"),
            1_010,
        );
        let lines = render_playbook_panel_lines(&run, 80, 10);
        assert!(lines[0].contains("PLAYBOOK"));
        assert!(lines
            .iter()
            .any(|line| line.contains("[x] Triage incident")));
        assert!(lines
            .iter()
            .any(|line| line.contains("[>] Stabilize service")));
        assert!(lines.iter().any(|line| line.contains("[deps]")));
    }
}
