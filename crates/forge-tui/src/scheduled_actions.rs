//! Scheduled actions queue for delayed/snoozed operator workflows.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduledActionKind {
    SnoozeThread { thread_id: String },
    RestartLoop { loop_id: String },
    AutoAcknowledgeThread { thread_id: String },
}

impl ScheduledActionKind {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::SnoozeThread { .. } => "snooze-thread",
            Self::RestartLoop { .. } => "restart-loop",
            Self::AutoAcknowledgeThread { .. } => "auto-ack-thread",
        }
    }

    #[must_use]
    fn target(&self) -> &str {
        match self {
            Self::SnoozeThread { thread_id } | Self::AutoAcknowledgeThread { thread_id } => {
                thread_id.as_str()
            }
            Self::RestartLoop { loop_id } => loop_id.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledAction {
    pub schedule_id: String,
    pub kind: ScheduledActionKind,
    pub created_at_epoch_s: i64,
    pub due_at_epoch_s: i64,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueScheduledAction {
    pub schedule_id: String,
    pub kind: ScheduledActionKind,
    pub due_at_epoch_s: i64,
    pub overdue_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScheduledActionQueue {
    sequence: u64,
    items: Vec<ScheduledAction>,
}

impl ScheduledActionQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn pending(&self) -> &[ScheduledAction] {
        &self.items
    }

    pub fn schedule_at(
        &mut self,
        kind: ScheduledActionKind,
        created_at_epoch_s: i64,
        due_at_epoch_s: i64,
        note: &str,
    ) -> Result<String, String> {
        let target = normalize_required(kind.target());
        if target.is_empty() {
            return Err("scheduled action target is required".to_owned());
        }

        let created_at_epoch_s = created_at_epoch_s.max(0);
        let due_at_epoch_s = due_at_epoch_s.max(0);
        if due_at_epoch_s <= created_at_epoch_s {
            return Err("due time must be in the future".to_owned());
        }

        self.sequence = self.sequence.saturating_add(1);
        let schedule_id = format!("sched-{}", self.sequence);
        let note = note.trim().to_owned();

        self.items.push(ScheduledAction {
            schedule_id: schedule_id.clone(),
            kind,
            created_at_epoch_s,
            due_at_epoch_s,
            note,
        });
        sort_queue(&mut self.items);
        Ok(schedule_id)
    }

    pub fn schedule_after(
        &mut self,
        kind: ScheduledActionKind,
        now_epoch_s: i64,
        delay_secs: u64,
        note: &str,
    ) -> Result<String, String> {
        if delay_secs == 0 {
            return Err("delay must be greater than zero seconds".to_owned());
        }
        let now_epoch_s = now_epoch_s.max(0);
        let due_at_epoch_s = now_epoch_s.saturating_add(delay_secs as i64);
        self.schedule_at(kind, now_epoch_s, due_at_epoch_s, note)
    }

    pub fn cancel(&mut self, schedule_id: &str) -> bool {
        let schedule_id = normalize_required(schedule_id);
        let Some(index) = self
            .items
            .iter()
            .position(|item| item.schedule_id == schedule_id)
        else {
            return false;
        };
        self.items.remove(index);
        true
    }

    #[must_use]
    pub fn due_actions(&self, now_epoch_s: i64) -> Vec<DueScheduledAction> {
        let now_epoch_s = now_epoch_s.max(0);
        self.items
            .iter()
            .filter(|item| item.due_at_epoch_s <= now_epoch_s)
            .map(|item| DueScheduledAction {
                schedule_id: item.schedule_id.clone(),
                kind: item.kind.clone(),
                due_at_epoch_s: item.due_at_epoch_s,
                overdue_secs: now_epoch_s.saturating_sub(item.due_at_epoch_s) as u64,
            })
            .collect()
    }

    pub fn pop_due_actions(&mut self, now_epoch_s: i64) -> Vec<DueScheduledAction> {
        let now_epoch_s = now_epoch_s.max(0);
        let mut due = Vec::new();
        let mut pending = Vec::new();
        for item in self.items.drain(..) {
            if item.due_at_epoch_s <= now_epoch_s {
                due.push(DueScheduledAction {
                    schedule_id: item.schedule_id,
                    kind: item.kind,
                    due_at_epoch_s: item.due_at_epoch_s,
                    overdue_secs: now_epoch_s.saturating_sub(item.due_at_epoch_s) as u64,
                });
            } else {
                pending.push(item);
            }
        }
        self.items = pending;
        sort_queue(&mut self.items);
        due
    }

    #[must_use]
    pub fn status_line(&self, now_epoch_s: i64, max_items: usize) -> String {
        if self.items.is_empty() {
            return "timers=0".to_owned();
        }

        let now_epoch_s = now_epoch_s.max(0);
        let max_items = max_items.max(1);
        let mut parts = Vec::new();
        parts.push(format!("timers={}", self.items.len()));
        for item in self.items.iter().take(max_items) {
            let remaining = item.due_at_epoch_s.saturating_sub(now_epoch_s).max(0) as u64;
            parts.push(format!(
                "{}:{}@+{}s",
                item.kind.label(),
                item.kind.target(),
                remaining
            ));
        }
        if self.items.len() > max_items {
            parts.push(format!("+{}", self.items.len() - max_items));
        }
        parts.join("  ")
    }
}

fn sort_queue(items: &mut [ScheduledAction]) {
    items.sort_by(|a, b| {
        a.due_at_epoch_s
            .cmp(&b.due_at_epoch_s)
            .then(a.schedule_id.cmp(&b.schedule_id))
    });
}

fn normalize_required(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{ScheduledActionKind, ScheduledActionQueue};

    #[test]
    fn schedule_after_and_pop_due_actions_in_order() {
        let mut queue = ScheduledActionQueue::new();
        queue
            .schedule_after(
                ScheduledActionKind::RestartLoop {
                    loop_id: "loop-b".to_owned(),
                },
                1_000,
                20,
                "retry later",
            )
            .expect("schedule loop-b");
        queue
            .schedule_after(
                ScheduledActionKind::RestartLoop {
                    loop_id: "loop-a".to_owned(),
                },
                1_000,
                10,
                "retry first",
            )
            .expect("schedule loop-a");

        let due = queue.pop_due_actions(1_015);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].kind.label(), "restart-loop");
        match &due[0].kind {
            ScheduledActionKind::RestartLoop { loop_id } => assert_eq!(loop_id, "loop-a"),
            other => panic!("unexpected kind {other:?}"),
        }
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn status_line_shows_timer_count_and_countdown() {
        let mut queue = ScheduledActionQueue::new();
        queue
            .schedule_after(
                ScheduledActionKind::SnoozeThread {
                    thread_id: "thread-1".to_owned(),
                },
                2_000,
                45,
                "quiet period",
            )
            .expect("schedule");

        let line = queue.status_line(2_010, 3);
        assert!(line.contains("timers=1"));
        assert!(line.contains("snooze-thread:thread-1@+35s"));
    }

    #[test]
    fn cancel_removes_item_by_schedule_id() {
        let mut queue = ScheduledActionQueue::new();
        let id = queue
            .schedule_after(
                ScheduledActionKind::AutoAcknowledgeThread {
                    thread_id: "thread-1".to_owned(),
                },
                1_000,
                60,
                "ack after timeout",
            )
            .expect("schedule");
        assert!(queue.cancel(&id));
        assert!(!queue.cancel(&id));
        assert!(queue.is_empty());
    }

    #[test]
    fn schedule_validates_target_and_future_due() {
        let mut queue = ScheduledActionQueue::new();
        assert!(queue
            .schedule_after(
                ScheduledActionKind::SnoozeThread {
                    thread_id: String::new(),
                },
                1_000,
                5,
                "",
            )
            .is_err());
        assert!(queue
            .schedule_after(
                ScheduledActionKind::RestartLoop {
                    loop_id: "loop-1".to_owned(),
                },
                1_000,
                0,
                "",
            )
            .is_err());
    }

    #[test]
    fn due_actions_peek_does_not_pop() {
        let mut queue = ScheduledActionQueue::new();
        queue
            .schedule_after(
                ScheduledActionKind::RestartLoop {
                    loop_id: "loop-1".to_owned(),
                },
                1_000,
                5,
                "",
            )
            .expect("schedule");

        let peek = queue.due_actions(1_006);
        assert_eq!(peek.len(), 1);
        assert_eq!(queue.len(), 1);
        let popped = queue.pop_due_actions(1_006);
        assert_eq!(popped.len(), 1);
        assert_eq!(queue.len(), 0);
    }
}
