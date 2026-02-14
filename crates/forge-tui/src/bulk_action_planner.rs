//! Bulk action planner for stop/scale/msg/inject workflows.
//!
//! Produces dry-run summaries, conflict diagnostics, rollback hints,
//! and transparent queued command previews before execution.

use std::collections::{BTreeMap, BTreeSet};

use crate::fleet_selection::FleetLoopRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkPlannerAction {
    Stop,
    Scale { target_count: usize },
    Message { body: String },
    Inject { body: String },
    AckThread,
    ReplyThread { body: String },
}

impl BulkPlannerAction {
    fn command_name(&self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Scale { .. } => "scale",
            Self::Message { .. } => "msg",
            Self::Inject { .. } => "inject",
            Self::AckThread => "thread-ack",
            Self::ReplyThread { .. } => "thread-reply",
        }
    }

    fn payload(&self) -> Option<&str> {
        match self {
            Self::Message { body } | Self::Inject { body } | Self::ReplyThread { body } => {
                Some(body)
            }
            Self::Stop | Self::Scale { .. } | Self::AckThread => None,
        }
    }

    fn requires_loop_targets(&self) -> bool {
        matches!(
            self,
            Self::Stop | Self::Scale { .. } | Self::Message { .. } | Self::Inject { .. }
        )
    }

    fn requires_thread_targets(&self) -> bool {
        matches!(self, Self::AckThread | Self::ReplyThread { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkThreadRecord {
    pub thread_key: String,
    pub subject: String,
    pub unread_count: usize,
    pub pending_ack_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkPlannerTarget {
    Loop(FleetLoopRecord),
    Thread(BulkThreadRecord),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkActionConflict {
    pub severity: ConflictSeverity,
    pub code: String,
    pub target: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueEntryStatus {
    Ready,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedQueueItem {
    pub queue_index: usize,
    pub target: String,
    pub command: String,
    pub rollback_hint: String,
    pub status: QueueEntryStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkActionPlan {
    pub action: BulkPlannerAction,
    pub dry_run_summary: String,
    pub total_targets: usize,
    pub ready_targets: usize,
    pub blocked_targets: usize,
    pub conflicts: Vec<BulkActionConflict>,
    pub queue: Vec<PlannedQueueItem>,
}

#[must_use]
pub fn plan_bulk_action(
    action: BulkPlannerAction,
    selected: &[FleetLoopRecord],
    preview_limit: usize,
) -> BulkActionPlan {
    let mut conflicts = Vec::new();
    let mut queue = Vec::new();

    if selected.is_empty() {
        conflicts.push(BulkActionConflict {
            severity: ConflictSeverity::Error,
            code: "empty-selection".to_owned(),
            target: None,
            message: "no loops selected for bulk action".to_owned(),
        });
        return finalize_plan(action, selected.len(), queue, conflicts, preview_limit);
    }

    let payload_missing = action
        .payload()
        .is_some_and(|payload| payload.trim().is_empty());
    if payload_missing {
        conflicts.push(BulkActionConflict {
            severity: ConflictSeverity::Error,
            code: "missing-payload".to_owned(),
            target: None,
            message: format!(
                "{} action requires non-empty payload",
                action.command_name()
            ),
        });
    }

    let mut ordered: Vec<&FleetLoopRecord> = selected.iter().collect();
    ordered.sort_by(|a, b| {
        normalize(&a.id)
            .cmp(&normalize(&b.id))
            .then(normalize(&a.name).cmp(&normalize(&b.name)))
    });

    match &action {
        BulkPlannerAction::Scale { target_count } => {
            plan_scale_action(*target_count, &ordered, &mut queue, &mut conflicts);
        }
        BulkPlannerAction::Stop
        | BulkPlannerAction::Message { .. }
        | BulkPlannerAction::Inject { .. } => {
            plan_per_loop_action(
                &action,
                payload_missing,
                &ordered,
                &mut queue,
                &mut conflicts,
            );
        }
        BulkPlannerAction::AckThread | BulkPlannerAction::ReplyThread { .. } => {
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Error,
                code: "unsupported-target-kind".to_owned(),
                target: None,
                message: "thread actions require plan_bulk_action_mixed with thread targets"
                    .to_owned(),
            });
        }
    }

    finalize_plan(action, selected.len(), queue, conflicts, preview_limit)
}

#[must_use]
pub fn plan_bulk_action_mixed(
    action: BulkPlannerAction,
    selected: &[BulkPlannerTarget],
    preview_limit: usize,
) -> BulkActionPlan {
    let mut loops = Vec::new();
    let mut threads = Vec::new();
    for target in selected {
        match target {
            BulkPlannerTarget::Loop(loop_record) => loops.push(loop_record.clone()),
            BulkPlannerTarget::Thread(thread_record) => threads.push(thread_record.clone()),
        }
    }

    let mut conflicts = Vec::new();
    let mut queue = Vec::new();

    if action.requires_loop_targets() {
        if loops.is_empty() {
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Error,
                code: "empty-loop-selection".to_owned(),
                target: None,
                message: "no loops selected for loop action".to_owned(),
            });
            return finalize_plan(action, selected.len(), queue, conflicts, preview_limit);
        }

        let payload_missing = action
            .payload()
            .is_some_and(|payload| payload.trim().is_empty());
        if payload_missing {
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Error,
                code: "missing-payload".to_owned(),
                target: None,
                message: format!(
                    "{} action requires non-empty payload",
                    action.command_name()
                ),
            });
        }

        let mut ordered: Vec<&FleetLoopRecord> = loops.iter().collect();
        ordered.sort_by(|a, b| {
            normalize(&a.id)
                .cmp(&normalize(&b.id))
                .then(normalize(&a.name).cmp(&normalize(&b.name)))
        });

        match &action {
            BulkPlannerAction::Scale { target_count } => {
                plan_scale_action(*target_count, &ordered, &mut queue, &mut conflicts);
            }
            BulkPlannerAction::Stop
            | BulkPlannerAction::Message { .. }
            | BulkPlannerAction::Inject { .. } => {
                plan_per_loop_action(
                    &action,
                    payload_missing,
                    &ordered,
                    &mut queue,
                    &mut conflicts,
                );
            }
            BulkPlannerAction::AckThread | BulkPlannerAction::ReplyThread { .. } => {}
        }

        for thread in &threads {
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Warning,
                code: "ignored-thread-target".to_owned(),
                target: Some(normalize_thread_target(thread)),
                message: "thread target ignored by loop action".to_owned(),
            });
            push_queue_item(
                &mut queue,
                normalize_thread_target(thread),
                format!("{} <thread-target>", action.command_name()),
                "no-op rollback: target ignored".to_owned(),
                Some("target is thread; action requires loop targets".to_owned()),
            );
        }

        return finalize_plan(action, selected.len(), queue, conflicts, preview_limit);
    }

    if action.requires_thread_targets() {
        if threads.is_empty() {
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Error,
                code: "empty-thread-selection".to_owned(),
                target: None,
                message: "no threads selected for thread action".to_owned(),
            });
            return finalize_plan(action, selected.len(), queue, conflicts, preview_limit);
        }

        let payload_missing = action
            .payload()
            .is_some_and(|payload| payload.trim().is_empty());
        if payload_missing {
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Error,
                code: "missing-payload".to_owned(),
                target: None,
                message: format!(
                    "{} action requires non-empty payload",
                    action.command_name()
                ),
            });
        }

        plan_thread_action(
            &action,
            payload_missing,
            &threads,
            &mut queue,
            &mut conflicts,
        );

        for loop_entry in &loops {
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Warning,
                code: "ignored-loop-target".to_owned(),
                target: Some(loop_entry.id.clone()),
                message: "loop target ignored by thread action".to_owned(),
            });
            push_queue_item(
                &mut queue,
                loop_entry.id.clone(),
                format!("{} <loop-target>", action.command_name()),
                "no-op rollback: target ignored".to_owned(),
                Some("target is loop; action requires thread targets".to_owned()),
            );
        }

        return finalize_plan(action, selected.len(), queue, conflicts, preview_limit);
    }

    finalize_plan(action, selected.len(), queue, conflicts, preview_limit)
}

#[must_use]
pub fn queue_transparency_lines(plan: &BulkActionPlan, max_rows: usize) -> Vec<String> {
    if max_rows == 0 {
        return Vec::new();
    }

    let mut lines = vec![plan.dry_run_summary.clone()];
    let detail_rows = max_rows.saturating_sub(1);
    let shown = plan.queue.len().min(detail_rows);

    for item in plan.queue.iter().take(shown) {
        let status = match item.status {
            QueueEntryStatus::Ready => "ready",
            QueueEntryStatus::Blocked => "blocked",
        };
        let detail = if let Some(reason) = &item.reason {
            format!(
                "{}. [{}] target={} cmd={} reason={} rollback={}",
                item.queue_index, status, item.target, item.command, reason, item.rollback_hint
            )
        } else {
            format!(
                "{}. [{}] target={} cmd={} rollback={}",
                item.queue_index, status, item.target, item.command, item.rollback_hint
            )
        };
        lines.push(detail);
    }

    if plan.queue.len() > shown {
        lines.push(format!(
            "... +{} more queued item(s)",
            plan.queue.len().saturating_sub(shown)
        ));
    }

    lines
}

fn plan_per_loop_action(
    action: &BulkPlannerAction,
    payload_missing: bool,
    ordered: &[&FleetLoopRecord],
    queue: &mut Vec<PlannedQueueItem>,
    conflicts: &mut Vec<BulkActionConflict>,
) {
    let mut seen_ids = BTreeSet::new();

    for loop_entry in ordered {
        let loop_id = loop_entry.id.trim().to_owned();
        let target = if loop_id.is_empty() {
            "<missing-loop-id>".to_owned()
        } else {
            loop_id.clone()
        };

        let mut blocked_reason = None;
        if loop_id.is_empty() {
            blocked_reason = Some("target loop id is empty".to_owned());
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Error,
                code: "missing-loop-id".to_owned(),
                target: Some(target.clone()),
                message: "selected row does not have a loop id".to_owned(),
            });
        } else if !seen_ids.insert(normalize(&loop_id)) {
            blocked_reason = Some("duplicate loop id in selection".to_owned());
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Warning,
                code: "duplicate-loop-id".to_owned(),
                target: Some(loop_id.clone()),
                message: "selection contains duplicate loop id; dedupe before execution".to_owned(),
            });
        }

        if blocked_reason.is_none() && payload_missing {
            blocked_reason = Some("action payload is empty".to_owned());
        }

        if blocked_reason.is_none() {
            let state = normalize(&loop_entry.state);
            match action {
                BulkPlannerAction::Stop if state == "stopped" || state == "error" => {
                    blocked_reason = Some(format!(
                        "stop is a no-op for terminal loop state '{}'",
                        loop_entry.state.trim()
                    ));
                    conflicts.push(BulkActionConflict {
                        severity: ConflictSeverity::Warning,
                        code: "stop-noop-target".to_owned(),
                        target: Some(loop_id.clone()),
                        message: format!(
                            "loop state '{}' is already terminal",
                            loop_entry.state.trim()
                        ),
                    });
                }
                BulkPlannerAction::Inject { .. } if state == "stopped" || state == "error" => {
                    blocked_reason = Some(format!(
                        "inject requires active loop; found '{}'",
                        loop_entry.state.trim()
                    ));
                    conflicts.push(BulkActionConflict {
                        severity: ConflictSeverity::Error,
                        code: "inject-inactive-target".to_owned(),
                        target: Some(loop_id.clone()),
                        message: "inject cannot target stopped/errored loops".to_owned(),
                    });
                }
                BulkPlannerAction::Stop
                | BulkPlannerAction::Message { .. }
                | BulkPlannerAction::Inject { .. } => {}
                BulkPlannerAction::Scale { .. }
                | BulkPlannerAction::AckThread
                | BulkPlannerAction::ReplyThread { .. } => {}
            }
        }

        let command = build_loop_command(action, &loop_id);
        let rollback_hint = rollback_hint_for_loop(action, &loop_id);
        push_queue_item(queue, target, command, rollback_hint, blocked_reason);
    }
}

fn plan_scale_action(
    target_count: usize,
    ordered: &[&FleetLoopRecord],
    queue: &mut Vec<PlannedQueueItem>,
    conflicts: &mut Vec<BulkActionConflict>,
) {
    let mut cohorts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut seen_ids = BTreeSet::new();

    for loop_entry in ordered {
        let loop_id = loop_entry.id.trim().to_owned();
        if loop_id.is_empty() {
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Error,
                code: "missing-loop-id".to_owned(),
                target: Some("<missing-loop-id>".to_owned()),
                message: "cannot derive scale cohort from row with empty loop id".to_owned(),
            });
            push_queue_item(
                queue,
                "<missing-loop-id>".to_owned(),
                format!("forge scale --pool <pool> --count {target_count}"),
                "rerun with resolved pool and prior count after validation".to_owned(),
                Some("target loop id is empty".to_owned()),
            );
            continue;
        }

        if !seen_ids.insert(normalize(&loop_id)) {
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Warning,
                code: "duplicate-loop-id".to_owned(),
                target: Some(loop_id.clone()),
                message: "duplicate loop id ignored for scale cohort planning".to_owned(),
            });
            continue;
        }

        let pool = loop_entry.pool.trim();
        if pool.is_empty() {
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Error,
                code: "missing-pool".to_owned(),
                target: Some(loop_id.clone()),
                message: "scale planning requires pool on every selected loop".to_owned(),
            });
            push_queue_item(
                queue,
                loop_id.clone(),
                format!("forge scale --pool <pool> --count {target_count}"),
                "rerun with resolved pool and prior count after validation".to_owned(),
                Some("loop is missing pool value".to_owned()),
            );
            continue;
        }

        cohorts
            .entry(pool.to_owned())
            .or_default()
            .insert(loop_id.clone());
    }

    if cohorts.len() > 1 {
        conflicts.push(BulkActionConflict {
            severity: ConflictSeverity::Warning,
            code: "multi-pool-scale".to_owned(),
            target: None,
            message: format!(
                "selection spans {} pools; planner enqueues one scale command per pool",
                cohorts.len()
            ),
        });
    }

    for (pool, loop_ids) in cohorts {
        let target = format!("pool={} ({} selected loop(s))", pool, loop_ids.len());
        let command = format!(
            "forge scale --pool {} --count {}",
            shell_quote(&pool),
            target_count
        );
        let rollback_hint = format!(
            "capture current pool count before execution; rollback with forge scale --pool {} --count <previous>",
            shell_quote(&pool)
        );
        push_queue_item(queue, target, command, rollback_hint, None);
    }
}

fn plan_thread_action(
    action: &BulkPlannerAction,
    payload_missing: bool,
    selected: &[BulkThreadRecord],
    queue: &mut Vec<PlannedQueueItem>,
    conflicts: &mut Vec<BulkActionConflict>,
) {
    let mut seen_threads = BTreeSet::new();

    for thread in selected {
        let target = normalize_thread_target(thread);
        let mut blocked_reason = None;

        if thread.thread_key.trim().is_empty() {
            blocked_reason = Some("thread key is empty".to_owned());
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Error,
                code: "missing-thread-key".to_owned(),
                target: Some(target.clone()),
                message: "selected row does not have thread key".to_owned(),
            });
        } else if !seen_threads.insert(normalize(&thread.thread_key)) {
            blocked_reason = Some("duplicate thread target".to_owned());
            conflicts.push(BulkActionConflict {
                severity: ConflictSeverity::Warning,
                code: "duplicate-thread-target".to_owned(),
                target: Some(target.clone()),
                message: "duplicate thread target in selection".to_owned(),
            });
        }

        if blocked_reason.is_none() && payload_missing {
            blocked_reason = Some("action payload is empty".to_owned());
        }

        if blocked_reason.is_none() {
            match action {
                BulkPlannerAction::AckThread if thread.pending_ack_count == 0 => {
                    blocked_reason = Some("thread has no pending acknowledgements".to_owned());
                    conflicts.push(BulkActionConflict {
                        severity: ConflictSeverity::Warning,
                        code: "thread-ack-noop".to_owned(),
                        target: Some(target.clone()),
                        message: "ack action is a no-op for thread with no pending ack".to_owned(),
                    });
                }
                BulkPlannerAction::AckThread | BulkPlannerAction::ReplyThread { .. } => {}
                BulkPlannerAction::Stop
                | BulkPlannerAction::Scale { .. }
                | BulkPlannerAction::Message { .. }
                | BulkPlannerAction::Inject { .. } => {}
            }
        }

        let command = build_thread_command(action, &thread.thread_key);
        let rollback_hint = rollback_hint_for_thread(action, &thread.thread_key);
        push_queue_item(queue, target, command, rollback_hint, blocked_reason);
    }
}

fn finalize_plan(
    action: BulkPlannerAction,
    total_targets: usize,
    queue: Vec<PlannedQueueItem>,
    conflicts: Vec<BulkActionConflict>,
    preview_limit: usize,
) -> BulkActionPlan {
    let ready_targets = queue
        .iter()
        .filter(|item| item.status == QueueEntryStatus::Ready)
        .count();
    let blocked_targets = queue.len().saturating_sub(ready_targets);
    let dry_run_summary = build_dry_run_summary(
        &action,
        total_targets,
        ready_targets,
        blocked_targets,
        &queue,
        preview_limit,
    );

    BulkActionPlan {
        action,
        dry_run_summary,
        total_targets,
        ready_targets,
        blocked_targets,
        conflicts,
        queue,
    }
}

fn build_dry_run_summary(
    action: &BulkPlannerAction,
    total_targets: usize,
    ready_targets: usize,
    blocked_targets: usize,
    queue: &[PlannedQueueItem],
    preview_limit: usize,
) -> String {
    let limit = preview_limit.max(1);
    let command_preview: Vec<&str> = queue
        .iter()
        .filter(|item| item.status == QueueEntryStatus::Ready)
        .take(limit)
        .map(|item| item.command.as_str())
        .collect();
    let hidden = ready_targets.saturating_sub(command_preview.len());

    let preview_text = if command_preview.is_empty() {
        "none".to_owned()
    } else if hidden == 0 {
        command_preview.join(" | ")
    } else {
        format!("{} | +{} more", command_preview.join(" | "), hidden)
    };

    format!(
        "dry-run {}: targets={} ready={} blocked={} preview={} ; rollback hints attached per queued item",
        action.command_name(),
        total_targets,
        ready_targets,
        blocked_targets,
        preview_text
    )
}

fn build_loop_command(action: &BulkPlannerAction, loop_id: &str) -> String {
    let loop_token = if loop_id.trim().is_empty() {
        "<id>".to_owned()
    } else {
        shell_quote(loop_id)
    };

    match action {
        BulkPlannerAction::Stop => format!("forge stop --loop {loop_token}"),
        BulkPlannerAction::Message { body } => {
            format!(
                "forge msg --loop {} -- {}",
                loop_token,
                shell_quote(body.trim())
            )
        }
        BulkPlannerAction::Inject { body } => {
            format!(
                "forge inject --loop {} -- {}",
                loop_token,
                shell_quote(body.trim())
            )
        }
        BulkPlannerAction::Scale { target_count } => {
            format!("forge scale --count {target_count}")
        }
        BulkPlannerAction::AckThread | BulkPlannerAction::ReplyThread { .. } => {
            format!("{} <thread-target>", action.command_name())
        }
    }
}

fn rollback_hint_for_loop(action: &BulkPlannerAction, loop_id: &str) -> String {
    let loop_token = if loop_id.trim().is_empty() {
        "<id>".to_owned()
    } else {
        shell_quote(loop_id)
    };
    match action {
        BulkPlannerAction::Stop => format!("rollback: forge resume --loop {loop_token}"),
        BulkPlannerAction::Scale { .. } => {
            "rollback: rerun scale with previous target count after verifying baseline".to_owned()
        }
        BulkPlannerAction::Message { .. } => format!(
            "rollback: send corrective msg via forge msg --loop {} -- <correction>",
            loop_token
        ),
        BulkPlannerAction::Inject { .. } => format!(
            "rollback: prefer queued correction via forge msg --loop {} -- <correction>",
            loop_token
        ),
        BulkPlannerAction::AckThread | BulkPlannerAction::ReplyThread { .. } => {
            "rollback: unsupported for loop target".to_owned()
        }
    }
}

fn build_thread_command(action: &BulkPlannerAction, thread_key: &str) -> String {
    let thread_token = if thread_key.trim().is_empty() {
        "<thread>".to_owned()
    } else {
        shell_quote(thread_key)
    };

    match action {
        BulkPlannerAction::AckThread => format!("fmail ack --thread {thread_token}"),
        BulkPlannerAction::ReplyThread { body } => format!(
            "fmail send --thread {} -- {}",
            thread_token,
            shell_quote(body.trim())
        ),
        BulkPlannerAction::Stop
        | BulkPlannerAction::Scale { .. }
        | BulkPlannerAction::Message { .. }
        | BulkPlannerAction::Inject { .. } => format!("{} <loop-target>", action.command_name()),
    }
}

fn rollback_hint_for_thread(action: &BulkPlannerAction, thread_key: &str) -> String {
    let thread_token = if thread_key.trim().is_empty() {
        "<thread>".to_owned()
    } else {
        shell_quote(thread_key)
    };
    match action {
        BulkPlannerAction::AckThread => format!(
            "rollback: post corrective follow-up in thread {} if ack was premature",
            thread_token
        ),
        BulkPlannerAction::ReplyThread { .. } => format!(
            "rollback: send corrective follow-up via fmail send --thread {} -- <correction>",
            thread_token
        ),
        BulkPlannerAction::Stop
        | BulkPlannerAction::Scale { .. }
        | BulkPlannerAction::Message { .. }
        | BulkPlannerAction::Inject { .. } => "rollback: unsupported for thread target".to_owned(),
    }
}

fn normalize_thread_target(thread: &BulkThreadRecord) -> String {
    let key = thread.thread_key.trim();
    let subject = thread.subject.trim();
    if key.is_empty() {
        "<missing-thread-key>".to_owned()
    } else if subject.is_empty() {
        format!("thread={key}")
    } else {
        format!("thread={key} ({subject})")
    }
}

fn push_queue_item(
    queue: &mut Vec<PlannedQueueItem>,
    target: String,
    command: String,
    rollback_hint: String,
    reason: Option<String>,
) {
    let status = if reason.is_some() {
        QueueEntryStatus::Blocked
    } else {
        QueueEntryStatus::Ready
    };
    queue.push(PlannedQueueItem {
        queue_index: queue.len() + 1,
        target,
        command,
        rollback_hint,
        status,
        reason,
    });
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn shell_quote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "''".to_owned();
    }
    if trimmed
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-_./:".contains(&byte))
    {
        return trimmed.to_owned();
    }
    format!("'{}'", trimmed.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::{
        plan_bulk_action, plan_bulk_action_mixed, queue_transparency_lines, BulkPlannerAction,
        BulkPlannerTarget, BulkThreadRecord, ConflictSeverity, QueueEntryStatus,
    };
    use crate::fleet_selection::FleetLoopRecord;

    fn sample_loops() -> Vec<FleetLoopRecord> {
        vec![
            FleetLoopRecord {
                id: "loop-aa11".to_owned(),
                name: "frontend-a".to_owned(),
                repo: "/repos/forge".to_owned(),
                profile: "codex3".to_owned(),
                pool: "day".to_owned(),
                state: "running".to_owned(),
                tags: vec!["ui".to_owned()],
                stale: false,
            },
            FleetLoopRecord {
                id: "loop-bb22".to_owned(),
                name: "frontend-b".to_owned(),
                repo: "/repos/forge".to_owned(),
                profile: "codex3".to_owned(),
                pool: "day".to_owned(),
                state: "waiting".to_owned(),
                tags: vec!["ui".to_owned()],
                stale: false,
            },
            FleetLoopRecord {
                id: "loop-cc33".to_owned(),
                name: "backend-c".to_owned(),
                repo: "/repos/forge".to_owned(),
                profile: "claude".to_owned(),
                pool: "nightly".to_owned(),
                state: "stopped".to_owned(),
                tags: vec!["infra".to_owned()],
                stale: true,
            },
        ]
    }

    fn sample_threads() -> Vec<BulkThreadRecord> {
        vec![
            BulkThreadRecord {
                thread_key: "task-forge-1".to_owned(),
                subject: "handoff request".to_owned(),
                unread_count: 2,
                pending_ack_count: 1,
            },
            BulkThreadRecord {
                thread_key: "task-forge-2".to_owned(),
                subject: "ci failure".to_owned(),
                unread_count: 1,
                pending_ack_count: 0,
            },
        ]
    }

    #[test]
    fn stop_plan_includes_dry_run_summary_and_rollback_hints() {
        let loops = sample_loops();
        let plan = plan_bulk_action(BulkPlannerAction::Stop, &loops[0..2], 1);

        assert_eq!(plan.total_targets, 2);
        assert_eq!(plan.ready_targets, 2);
        assert_eq!(plan.blocked_targets, 0);
        assert!(plan.dry_run_summary.starts_with("dry-run stop:"));
        assert!(plan.dry_run_summary.contains("+1 more"));
        assert_eq!(plan.queue[0].status, QueueEntryStatus::Ready);
        assert!(plan.queue[0].rollback_hint.contains("forge resume --loop"));
    }

    #[test]
    fn stop_plan_flags_duplicates_and_terminal_targets() {
        let mut loops = sample_loops();
        loops.push(loops[0].clone());
        let plan = plan_bulk_action(BulkPlannerAction::Stop, &loops, 3);

        assert_eq!(plan.total_targets, 4);
        assert_eq!(plan.ready_targets, 2);
        assert_eq!(plan.blocked_targets, 2);
        assert!(plan
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "duplicate-loop-id"));
        assert!(plan
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "stop-noop-target"));
    }

    #[test]
    fn message_plan_requires_non_empty_payload() {
        let loops = sample_loops();
        let plan = plan_bulk_action(
            BulkPlannerAction::Message {
                body: "   ".to_owned(),
            },
            &loops[0..1],
            2,
        );

        assert_eq!(plan.ready_targets, 0);
        assert_eq!(plan.blocked_targets, 1);
        assert!(plan
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "missing-payload"
                && conflict.severity == ConflictSeverity::Error));
    }

    #[test]
    fn scale_plan_builds_per_pool_queue_with_warning() {
        let loops = sample_loops();
        let plan = plan_bulk_action(
            BulkPlannerAction::Scale { target_count: 3 },
            &loops[0..3],
            5,
        );

        assert_eq!(plan.total_targets, 3);
        assert_eq!(plan.ready_targets, 2);
        assert_eq!(plan.blocked_targets, 0);
        assert_eq!(plan.queue.len(), 2);
        assert!(plan.queue[0].command.contains("forge scale --pool"));
        assert!(plan
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "multi-pool-scale"));
    }

    #[test]
    fn inject_plan_blocks_terminal_targets() {
        let loops = sample_loops();
        let plan = plan_bulk_action(
            BulkPlannerAction::Inject {
                body: "urgent context".to_owned(),
            },
            &loops[1..3],
            3,
        );

        assert_eq!(plan.ready_targets, 1);
        assert_eq!(plan.blocked_targets, 1);
        assert!(plan.queue[1]
            .reason
            .as_ref()
            .is_some_and(|reason| reason.contains("requires active loop")));
        assert!(plan
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "inject-inactive-target"));
    }

    #[test]
    fn queue_transparency_lines_show_queue_status_and_reason() {
        let loops = sample_loops();
        let plan = plan_bulk_action(
            BulkPlannerAction::Inject {
                body: "urgent context".to_owned(),
            },
            &loops[1..3],
            2,
        );

        let lines = queue_transparency_lines(&plan, 5);
        assert!(lines[0].starts_with("dry-run inject:"));
        assert!(lines.iter().any(|line| line.contains("[blocked]")));
        assert!(lines.iter().any(|line| line.contains("rollback")));
    }

    #[test]
    fn mixed_plan_for_loop_action_blocks_thread_targets() {
        let loops = sample_loops();
        let threads = sample_threads();
        let selected = vec![
            BulkPlannerTarget::Loop(loops[0].clone()),
            BulkPlannerTarget::Thread(threads[0].clone()),
        ];

        let plan = plan_bulk_action_mixed(BulkPlannerAction::Stop, &selected, 3);
        assert_eq!(plan.total_targets, 2);
        assert_eq!(plan.ready_targets, 1);
        assert_eq!(plan.blocked_targets, 1);
        assert!(plan
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "ignored-thread-target"));
        assert!(plan.queue.iter().any(|item| item
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("requires loop targets"))));
    }

    #[test]
    fn ack_thread_plan_uses_pending_ack_and_noop_when_clear() {
        let loops = sample_loops();
        let threads = sample_threads();
        let selected = vec![
            BulkPlannerTarget::Loop(loops[0].clone()),
            BulkPlannerTarget::Thread(threads[0].clone()),
            BulkPlannerTarget::Thread(threads[1].clone()),
        ];

        let plan = plan_bulk_action_mixed(BulkPlannerAction::AckThread, &selected, 5);
        assert_eq!(plan.total_targets, 3);
        assert_eq!(plan.ready_targets, 1);
        assert_eq!(plan.blocked_targets, 2);
        assert!(plan
            .queue
            .iter()
            .any(|item| item.command.contains("fmail ack --thread")));
        assert!(plan
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "thread-ack-noop"));
        assert!(plan
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "ignored-loop-target"));
    }

    #[test]
    fn reply_thread_requires_payload_in_mixed_planner() {
        let threads = sample_threads();
        let selected = vec![BulkPlannerTarget::Thread(threads[0].clone())];
        let plan = plan_bulk_action_mixed(
            BulkPlannerAction::ReplyThread {
                body: "   ".to_owned(),
            },
            &selected,
            4,
        );
        assert_eq!(plan.ready_targets, 0);
        assert_eq!(plan.blocked_targets, 1);
        assert!(plan
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "missing-payload"));
    }
}
