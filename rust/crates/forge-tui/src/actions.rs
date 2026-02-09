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
pub enum ConfirmInputResult {
    Cancelled,
    OpenHelp,
    Submit(ActionRequest),
    Noop,
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
        action_status, confirm_to_request, enter_confirm, handle_confirm_input, loop_display_id,
        ActionKind, ConfirmInputResult, LoopState,
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
}
