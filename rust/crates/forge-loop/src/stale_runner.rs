pub const LOOP_SPAWN_OWNER_DAEMON: &str = "daemon";
pub const LOOP_STALE_RUNNER_REASON: &str = "stale_runner";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopState {
    Pending,
    Running,
    Sleeping,
    Waiting,
    Stopped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerLiveness {
    pub owner: String,
    pub instance_id: String,
    pub pid_alive: Option<bool>,
    pub daemon_alive: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonRunnerState {
    Running,
    Starting,
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonRunner {
    pub instance_id: String,
    pub state: DaemonRunnerState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleReconciliationRecord {
    pub pid_alive: bool,
    pub daemon_runner_alive: bool,
    pub reconciled_at_rfc3339: String,
    pub reason: String,
}

pub fn should_mark_loop_stale(
    loop_state: &LoopState,
    info: &RunnerLiveness,
    daemon_reachable: bool,
) -> bool {
    if *loop_state != LoopState::Running {
        return false;
    }

    let pid_missing_or_dead = info.pid_alive != Some(true);
    if !pid_missing_or_dead {
        return false;
    }

    if daemon_reachable {
        return info.daemon_alive != Some(true);
    }

    info.owner != LOOP_SPAWN_OWNER_DAEMON
}

pub fn daemon_runner_alive(runner: Option<&DaemonRunner>, expected_instance_id: &str) -> bool {
    let Some(runner) = runner else {
        return false;
    };

    if !expected_instance_id.trim().is_empty()
        && runner.instance_id.trim() != expected_instance_id.trim()
    {
        return false;
    }

    runner.state == DaemonRunnerState::Running
}

pub fn stale_reconciliation_record(
    info: &RunnerLiveness,
    reconciled_at_rfc3339: &str,
) -> StaleReconciliationRecord {
    StaleReconciliationRecord {
        pid_alive: info.pid_alive.unwrap_or(false),
        daemon_runner_alive: info.daemon_alive.unwrap_or(false),
        reconciled_at_rfc3339: reconciled_at_rfc3339.to_string(),
        reason: LOOP_STALE_RUNNER_REASON.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        daemon_runner_alive, should_mark_loop_stale, stale_reconciliation_record, DaemonRunner,
        DaemonRunnerState, LoopState, RunnerLiveness, LOOP_SPAWN_OWNER_DAEMON,
        LOOP_STALE_RUNNER_REASON,
    };

    #[test]
    fn mark_stale_when_running_and_pid_dead_and_daemon_dead() {
        let info = RunnerLiveness {
            owner: "local".to_string(),
            instance_id: "inst-1".to_string(),
            pid_alive: Some(false),
            daemon_alive: Some(false),
        };
        assert!(should_mark_loop_stale(&LoopState::Running, &info, true));
    }

    #[test]
    fn running_with_live_pid_is_not_stale() {
        let info = RunnerLiveness {
            owner: "local".to_string(),
            instance_id: "inst-1".to_string(),
            pid_alive: Some(true),
            daemon_alive: Some(false),
        };
        assert!(!should_mark_loop_stale(&LoopState::Running, &info, true));
    }

    #[test]
    fn daemon_owned_loop_skips_stale_when_daemon_unreachable() {
        let info = RunnerLiveness {
            owner: LOOP_SPAWN_OWNER_DAEMON.to_string(),
            instance_id: "inst-1".to_string(),
            pid_alive: None,
            daemon_alive: None,
        };
        assert!(!should_mark_loop_stale(&LoopState::Running, &info, false));
    }

    #[test]
    fn non_running_loop_never_marked_stale() {
        let info = RunnerLiveness {
            owner: "local".to_string(),
            instance_id: "inst-1".to_string(),
            pid_alive: Some(false),
            daemon_alive: Some(false),
        };
        assert!(!should_mark_loop_stale(&LoopState::Stopped, &info, true));
    }

    #[test]
    fn daemon_runner_alive_requires_running_state_and_matching_instance() {
        let running = DaemonRunner {
            instance_id: "inst-1".to_string(),
            state: DaemonRunnerState::Running,
        };
        assert!(daemon_runner_alive(Some(&running), "inst-1"));
        assert!(!daemon_runner_alive(Some(&running), "inst-2"));

        let stopped = DaemonRunner {
            instance_id: "inst-1".to_string(),
            state: DaemonRunnerState::Stopped,
        };
        assert!(!daemon_runner_alive(Some(&stopped), "inst-1"));
        assert!(!daemon_runner_alive(None, "inst-1"));
    }

    #[test]
    fn stale_record_defaults_missing_flags_to_false() {
        let info = RunnerLiveness {
            owner: "local".to_string(),
            instance_id: "inst-1".to_string(),
            pid_alive: None,
            daemon_alive: Some(true),
        };

        let record = stale_reconciliation_record(&info, "2026-02-09T18:00:00Z");
        assert!(!record.pid_alive);
        assert!(record.daemon_runner_alive);
        assert_eq!(record.reconciled_at_rfc3339, "2026-02-09T18:00:00Z");
        assert_eq!(record.reason, LOOP_STALE_RUNNER_REASON);
    }

    #[test]
    fn reconnect_recovery_clears_stale_marking_when_daemon_runner_is_alive() {
        let mut info = RunnerLiveness {
            owner: "local".to_string(),
            instance_id: "inst-42".to_string(),
            pid_alive: Some(false),
            daemon_alive: Some(false),
        };

        assert!(should_mark_loop_stale(&LoopState::Running, &info, true));

        let daemon_runner = DaemonRunner {
            instance_id: "inst-42".to_string(),
            state: DaemonRunnerState::Running,
        };
        info.daemon_alive = Some(daemon_runner_alive(Some(&daemon_runner), "inst-42"));

        assert!(!should_mark_loop_stale(&LoopState::Running, &info, true));
    }
}
