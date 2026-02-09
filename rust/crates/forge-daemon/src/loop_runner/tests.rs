use super::{LoopRunnerError, LoopRunnerManager, LoopRunnerState, StartLoopRunnerRequest};
use std::sync::{Arc, Mutex};

type Calls = Arc<Mutex<Vec<(String, Vec<String>)>>>;

#[test]
fn start_requires_loop_id() {
    let mgr = LoopRunnerManager::new();
    let err = mgr
        .start_loop_runner(StartLoopRunnerRequest {
            loop_id: "   ".to_string(),
            config_path: "".to_string(),
            command_path: "".to_string(),
        })
        .err();
    assert_eq!(err, Some(LoopRunnerError::InvalidArgument));
}

#[test]
fn start_invokes_builder_and_sets_default_command_path() {
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let calls_clone = Arc::clone(&calls);
    let mgr = LoopRunnerManager::with_command_builder(Arc::new(move |cmd, args| {
        let mut guard = match calls_clone.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.push((cmd.to_string(), args.to_vec()));

        let mut c = std::process::Command::new("sh");
        c.args(["-c", "sleep 60"]);
        c
    }));

    let runner = match mgr.start_loop_runner(StartLoopRunnerRequest {
        loop_id: " loop-1 ".to_string(),
        config_path: " cfg.toml ".to_string(),
        command_path: "   ".to_string(),
    }) {
        Ok(runner) => runner,
        Err(err) => panic!("start error: {err:?}"),
    };

    assert_eq!(runner.loop_id, "loop-1");
    assert_eq!(runner.command_path, "forge");
    assert_eq!(runner.config_path, "cfg.toml");
    assert!(runner.pid > 0);
    assert_eq!(runner.state, LoopRunnerState::Running);
    assert!(!runner.instance_id.is_empty());

    let calls_snapshot = match calls.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    assert_eq!(calls_snapshot.len(), 1);
    assert_eq!(calls_snapshot[0].0, "forge");
    assert_eq!(
        calls_snapshot[0].1,
        vec![
            "--config".to_string(),
            "cfg.toml".to_string(),
            "loop".to_string(),
            "run".to_string(),
            "loop-1".to_string(),
        ]
    );

    let stop_res = mgr.stop_loop_runner("loop-1", true);
    match stop_res {
        Ok(res) => {
            assert!(res.success);
        }
        Err(err) => panic!("stop error: {err:?}"),
    }
}

#[test]
fn start_twice_while_running_returns_already_exists() {
    let mgr = LoopRunnerManager::with_command_builder(Arc::new(|_, _| {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", "sleep 60"]);
        c
    }));

    let _ = match mgr.start_loop_runner(StartLoopRunnerRequest {
        loop_id: "loop-2".to_string(),
        config_path: "".to_string(),
        command_path: "forge".to_string(),
    }) {
        Ok(runner) => runner,
        Err(err) => panic!("start error: {err:?}"),
    };

    let err = mgr
        .start_loop_runner(StartLoopRunnerRequest {
            loop_id: "loop-2".to_string(),
            config_path: "".to_string(),
            command_path: "forge".to_string(),
        })
        .err();
    assert_eq!(
        err,
        Some(LoopRunnerError::AlreadyExists("loop-2".to_string()))
    );

    let _ = mgr.stop_loop_runner("loop-2", true);
}

#[test]
fn stop_unknown_is_not_found() {
    let mgr = LoopRunnerManager::new();
    let err = mgr.stop_loop_runner("missing", true).err();
    assert_eq!(err, Some(LoopRunnerError::NotFound("missing".to_string())));
}

#[test]
fn stop_marks_runner_stopped_and_get_reflects_state() {
    let mgr = LoopRunnerManager::with_command_builder(Arc::new(|_, _| {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", "sleep 60"]);
        c
    }));

    let _ = match mgr.start_loop_runner(StartLoopRunnerRequest {
        loop_id: "loop-3".to_string(),
        config_path: "".to_string(),
        command_path: "forge".to_string(),
    }) {
        Ok(runner) => runner,
        Err(err) => panic!("start error: {err:?}"),
    };

    let stop_res = match mgr.stop_loop_runner("loop-3", false) {
        Ok(res) => res,
        Err(err) => panic!("stop error: {err:?}"),
    };
    assert!(stop_res.success);
    assert_eq!(stop_res.runner.state, LoopRunnerState::Stopped);

    let got = match mgr.get_loop_runner("loop-3") {
        Ok(runner) => runner,
        Err(err) => panic!("get error: {err:?}"),
    };
    assert_eq!(got.state, LoopRunnerState::Stopped);
}

#[test]
fn list_returns_sorted_by_loop_id() {
    let mgr = LoopRunnerManager::with_command_builder(Arc::new(|_, _| {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", "sleep 60"]);
        c
    }));

    let _ = mgr.start_loop_runner(StartLoopRunnerRequest {
        loop_id: "b-loop".to_string(),
        config_path: "".to_string(),
        command_path: "forge".to_string(),
    });
    let _ = mgr.start_loop_runner(StartLoopRunnerRequest {
        loop_id: "a-loop".to_string(),
        config_path: "".to_string(),
        command_path: "forge".to_string(),
    });

    let runners = mgr.list_loop_runners();
    let ids: Vec<String> = runners.into_iter().map(|r| r.loop_id).collect();
    assert_eq!(ids, vec!["a-loop".to_string(), "b-loop".to_string()]);

    mgr.stop_all_loop_runners(true);
}
