#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueControlItem {
    MessageAppend,
    NextPromptOverride,
    Pause,
    StopGraceful,
    KillNow,
    SteerMessage,
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueueInteractionPlan {
    pub has_messages: bool,
    pub has_prompt_override: bool,
    pub pause_requested: bool,
    pub pause_before_run: bool,
    pub stop_requested: bool,
    pub kill_requested: bool,
}

pub fn build_queue_interaction_plan(
    items: &[QueueControlItem],
    pending_steer_messages: usize,
) -> Result<QueueInteractionPlan, String> {
    let mut plan = QueueInteractionPlan {
        has_messages: pending_steer_messages > 0,
        ..QueueInteractionPlan::default()
    };

    for item in items {
        match item {
            QueueControlItem::MessageAppend | QueueControlItem::SteerMessage => {
                plan.has_messages = true;
            }
            QueueControlItem::NextPromptOverride => {
                if !plan.has_prompt_override {
                    plan.has_prompt_override = true;
                }
            }
            QueueControlItem::Pause => {
                plan.pause_requested = true;
                plan.pause_before_run = !plan.has_prompt_override && !plan.has_messages;
                return Ok(plan);
            }
            QueueControlItem::StopGraceful => {
                plan.stop_requested = true;
                return Ok(plan);
            }
            QueueControlItem::KillNow => {
                plan.kill_requested = true;
                return Ok(plan);
            }
            QueueControlItem::Unsupported(value) => {
                return Err(format!("unsupported queue item type '{value}'"));
            }
        }
    }

    Ok(plan)
}

pub fn should_inject_qualitative_stop(
    qual_due: bool,
    single_run: bool,
    plan: &QueueInteractionPlan,
) -> bool {
    if !qual_due || single_run {
        return false;
    }
    if plan.has_prompt_override {
        return false;
    }
    if plan.stop_requested || plan.kill_requested {
        return false;
    }
    if plan.pause_requested && plan.pause_before_run {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{
        build_queue_interaction_plan, should_inject_qualitative_stop, QueueControlItem,
        QueueInteractionPlan,
    };

    #[test]
    fn pending_steer_marks_messages() {
        let plan = match build_queue_interaction_plan(&[], 1) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan.has_messages);
    }

    #[test]
    fn first_prompt_override_wins_and_suppresses_qual() {
        let plan = match build_queue_interaction_plan(
            &[
                QueueControlItem::NextPromptOverride,
                QueueControlItem::NextPromptOverride,
            ],
            0,
        ) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan.has_prompt_override);
        assert!(!should_inject_qualitative_stop(true, false, &plan));
    }

    #[test]
    fn pause_is_before_run_when_no_messages_or_override() {
        let plan = match build_queue_interaction_plan(&[QueueControlItem::Pause], 0) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan.pause_requested);
        assert!(plan.pause_before_run);
    }

    #[test]
    fn pause_is_after_run_when_message_exists() {
        let plan = match build_queue_interaction_plan(
            &[QueueControlItem::MessageAppend, QueueControlItem::Pause],
            0,
        ) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan.pause_requested);
        assert!(!plan.pause_before_run);
    }

    #[test]
    fn pause_is_after_run_when_override_exists() {
        let plan = match build_queue_interaction_plan(
            &[
                QueueControlItem::NextPromptOverride,
                QueueControlItem::Pause,
            ],
            0,
        ) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan.pause_requested);
        assert!(!plan.pause_before_run);
    }

    #[test]
    fn stop_short_circuits_following_items() {
        let plan = match build_queue_interaction_plan(
            &[
                QueueControlItem::StopGraceful,
                QueueControlItem::NextPromptOverride,
                QueueControlItem::MessageAppend,
            ],
            0,
        ) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan.stop_requested);
        assert!(!plan.has_prompt_override);
        assert!(!plan.has_messages);
    }

    #[test]
    fn kill_short_circuits_following_items() {
        let plan = match build_queue_interaction_plan(
            &[
                QueueControlItem::KillNow,
                QueueControlItem::NextPromptOverride,
                QueueControlItem::MessageAppend,
            ],
            0,
        ) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan.kill_requested);
        assert!(!plan.has_prompt_override);
        assert!(!plan.has_messages);
    }

    #[test]
    fn single_run_never_injects_qualitative_stop() {
        let plan = QueueInteractionPlan::default();
        assert!(!should_inject_qualitative_stop(true, true, &plan));
    }

    #[test]
    fn pause_before_run_suppresses_qualitative_stop() {
        let plan = match build_queue_interaction_plan(&[QueueControlItem::Pause], 0) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(!should_inject_qualitative_stop(true, false, &plan));
    }

    #[test]
    fn injects_qualitative_stop_when_due_and_not_overridden() {
        let plan = QueueInteractionPlan::default();
        assert!(should_inject_qualitative_stop(true, false, &plan));
    }

    #[test]
    fn unsupported_queue_item_returns_error() {
        let err = match build_queue_interaction_plan(
            &[QueueControlItem::Unsupported("unknown".to_string())],
            0,
        ) {
            Ok(_) => panic!("expected error"),
            Err(err) => err,
        };
        assert_eq!(err, "unsupported queue item type 'unknown'");
    }
}
