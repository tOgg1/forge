#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopState {
    Pending,
    Running,
    Sleeping,
    Waiting,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionEvent {
    StartLoop,
    PauseBeforeRun,
    PauseAfterRun,
    ProfileWait,
    Resume,
    RunStarted,
    RunCompleted,
    StopRequested,
    KillRequested,
    ContextCancelled,
    LoopLimitReached,
    SingleRunComplete,
    ExecutionError,
    QueuePlanningError,
    PromptResolutionError,
    ProfileSelectionError,
}

pub fn next_state(_current: LoopState, event: TransitionEvent) -> LoopState {
    match event {
        TransitionEvent::StartLoop => LoopState::Running,
        TransitionEvent::PauseBeforeRun | TransitionEvent::PauseAfterRun => LoopState::Sleeping,
        TransitionEvent::ProfileWait => LoopState::Waiting,
        TransitionEvent::Resume => LoopState::Running,
        TransitionEvent::RunStarted => LoopState::Running,
        TransitionEvent::RunCompleted => LoopState::Sleeping,
        TransitionEvent::StopRequested
        | TransitionEvent::KillRequested
        | TransitionEvent::ContextCancelled
        | TransitionEvent::LoopLimitReached
        | TransitionEvent::SingleRunComplete => LoopState::Stopped,
        TransitionEvent::ExecutionError
        | TransitionEvent::QueuePlanningError
        | TransitionEvent::PromptResolutionError
        | TransitionEvent::ProfileSelectionError => LoopState::Error,
    }
}

pub fn transition(current: LoopState, event: TransitionEvent) -> (LoopState, bool) {
    let next = next_state(current, event);
    (next, next != current)
}

#[cfg(test)]
mod tests {
    use super::{next_state, transition, LoopState, TransitionEvent};

    #[test]
    fn start_loop_moves_to_running_from_any_state() {
        let states = [
            LoopState::Pending,
            LoopState::Running,
            LoopState::Sleeping,
            LoopState::Waiting,
            LoopState::Stopped,
            LoopState::Error,
        ];
        for state in states {
            assert_eq!(
                next_state(state, TransitionEvent::StartLoop),
                LoopState::Running
            );
        }
    }

    #[test]
    fn queue_pause_transitions_to_sleeping() {
        assert_eq!(
            next_state(LoopState::Running, TransitionEvent::PauseBeforeRun),
            LoopState::Sleeping
        );
        assert_eq!(
            next_state(LoopState::Running, TransitionEvent::PauseAfterRun),
            LoopState::Sleeping
        );
    }

    #[test]
    fn waiting_and_resume_semantics_match_runner_flow() {
        assert_eq!(
            next_state(LoopState::Running, TransitionEvent::ProfileWait),
            LoopState::Waiting
        );
        assert_eq!(
            next_state(LoopState::Waiting, TransitionEvent::Resume),
            LoopState::Running
        );
    }

    #[test]
    fn run_lifecycle_maps_to_running_then_sleeping() {
        assert_eq!(
            next_state(LoopState::Sleeping, TransitionEvent::RunStarted),
            LoopState::Running
        );
        assert_eq!(
            next_state(LoopState::Running, TransitionEvent::RunCompleted),
            LoopState::Sleeping
        );
    }

    #[test]
    fn terminal_stop_events_map_to_stopped() {
        let events = [
            TransitionEvent::StopRequested,
            TransitionEvent::KillRequested,
            TransitionEvent::ContextCancelled,
            TransitionEvent::LoopLimitReached,
            TransitionEvent::SingleRunComplete,
        ];
        for event in events {
            assert_eq!(next_state(LoopState::Running, event), LoopState::Stopped);
        }
    }

    #[test]
    fn failure_events_map_to_error() {
        let events = [
            TransitionEvent::ExecutionError,
            TransitionEvent::QueuePlanningError,
            TransitionEvent::PromptResolutionError,
            TransitionEvent::ProfileSelectionError,
        ];
        for event in events {
            assert_eq!(next_state(LoopState::Running, event), LoopState::Error);
        }
    }

    #[test]
    fn transition_reports_when_state_changes() {
        let (next, changed) = transition(LoopState::Waiting, TransitionEvent::Resume);
        assert_eq!(next, LoopState::Running);
        assert!(changed);

        let (next, changed) = transition(LoopState::Running, TransitionEvent::RunStarted);
        assert_eq!(next, LoopState::Running);
        assert!(!changed);
    }
}
