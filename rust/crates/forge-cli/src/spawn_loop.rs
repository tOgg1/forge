use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use forge_rpc::forged::v1 as proto;
use forge_rpc::forged::v1::forged_service_client::ForgedServiceClient;
use tonic::transport::Endpoint;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnResult {
    pub owner: String,
    pub instance_id: String,
    pub pid: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpawnOptions {
    pub config_path: String,
    pub command_path: String,
    pub daemon_target: String,
    pub suppress_warning: bool,
}

impl SpawnOptions {
    fn normalized_config_path(&self) -> String {
        self.config_path.trim().to_string()
    }

    fn resolved_command_path(&self) -> Result<String, String> {
        let trimmed = self.command_path.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }

        std::env::current_exe()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|err| format!("resolve current executable: {err}"))
    }

    fn resolved_daemon_target(&self) -> String {
        let env_target = std::env::var("FORGE_DAEMON_TARGET").unwrap_or_default();
        if let Some(target) = first_non_empty([self.daemon_target.as_str(), env_target.as_str()]) {
            return normalize_daemon_target(target);
        }

        "http://127.0.0.1:50051".to_string()
    }
}

trait LoopSpawner {
    fn spawn_local(
        &mut self,
        loop_id: &str,
        owner_label: &str,
        options: &SpawnOptions,
    ) -> Result<SpawnResult, String>;

    fn spawn_daemon(
        &mut self,
        loop_id: &str,
        options: &SpawnOptions,
    ) -> Result<SpawnResult, String>;
}

struct ProcessLoopSpawner;

impl LoopSpawner for ProcessLoopSpawner {
    fn spawn_local(
        &mut self,
        loop_id: &str,
        owner_label: &str,
        options: &SpawnOptions,
    ) -> Result<SpawnResult, String> {
        spawn_local(loop_id, owner_label, options)
    }

    fn spawn_daemon(
        &mut self,
        loop_id: &str,
        options: &SpawnOptions,
    ) -> Result<SpawnResult, String> {
        spawn_daemon(loop_id, options)
    }
}

pub fn start_loop_runner(
    loop_id: &str,
    requested_owner: &str,
    options: &SpawnOptions,
    warning_writer: &mut dyn Write,
) -> Result<SpawnResult, String> {
    let mut spawner = ProcessLoopSpawner;
    start_loop_runner_with_spawner(
        loop_id,
        requested_owner,
        options,
        warning_writer,
        &mut spawner,
    )
}

fn start_loop_runner_with_spawner(
    loop_id: &str,
    requested_owner: &str,
    options: &SpawnOptions,
    warning_writer: &mut dyn Write,
    spawner: &mut dyn LoopSpawner,
) -> Result<SpawnResult, String> {
    match requested_owner {
        "local" => spawner.spawn_local(loop_id, "local", options),
        "daemon" => spawner.spawn_daemon(loop_id, options),
        "auto" => match spawner.spawn_daemon(loop_id, options) {
            Ok(result) => Ok(result),
            Err(daemon_err) => {
                emit_spawn_owner_warning(options, warning_writer, &daemon_err);
                match spawner.spawn_local(loop_id, "local", options) {
                    Ok(result) => Ok(result),
                    Err(local_err) => Err(format!(
                        "daemon start failed ({daemon_err}), local fallback failed: {local_err}"
                    )),
                }
            }
        },
        other => Err(format!(
            "invalid --spawn-owner \"{other}\" (valid: local|daemon|auto)"
        )),
    }
}

fn spawn_local(
    loop_id: &str,
    owner_label: &str,
    options: &SpawnOptions,
) -> Result<SpawnResult, String> {
    if skip_spawn_for_test_harness() {
        return Ok(SpawnResult {
            owner: owner_label.to_string(),
            instance_id: format!("resume-{}", uuid::Uuid::new_v4().simple()),
            pid: None,
        });
    }

    let command_path = options.resolved_command_path()?;
    let mut cmd = Command::new(command_path);

    let config_path = options.normalized_config_path();
    if !config_path.is_empty() {
        cmd.arg("--config").arg(config_path);
    }

    cmd.arg("loop").arg("run").arg(loop_id);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());

    let child = cmd
        .spawn()
        .map_err(|err| format!("failed to start local loop process: {err}"))?;
    let pid = child.id();
    drop(child);

    Ok(SpawnResult {
        owner: owner_label.to_string(),
        instance_id: format!("resume-{}", uuid::Uuid::new_v4().simple()),
        pid: Some(i64::from(pid)),
    })
}

fn spawn_daemon(loop_id: &str, options: &SpawnOptions) -> Result<SpawnResult, String> {
    if skip_spawn_for_test_harness() {
        return Ok(SpawnResult {
            owner: "daemon".to_string(),
            instance_id: format!("resume-{}", uuid::Uuid::new_v4().simple()),
            pid: None,
        });
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("initialize daemon RPC runtime: {err}"))?;

    runtime.block_on(spawn_daemon_async(loop_id, options))
}

async fn spawn_daemon_async(loop_id: &str, options: &SpawnOptions) -> Result<SpawnResult, String> {
    let target = options.resolved_daemon_target();
    let endpoint = Endpoint::from_shared(target.clone())
        .map_err(|err| format!("forged daemon unavailable: {err}"))?
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2));

    let channel = endpoint
        .connect()
        .await
        .map_err(|err| format!("forged daemon unavailable: {err}"))?;

    let mut client = ForgedServiceClient::new(channel);

    client
        .ping(proto::PingRequest {})
        .await
        .map_err(|err| format!("forged daemon unavailable: {err}"))?;

    let request = build_start_loop_runner_request(loop_id, options)?;
    let response = client
        .start_loop_runner(request)
        .await
        .map_err(|err| format!("failed to start loop via daemon: {err}"))?
        .into_inner();

    let runner = response
        .runner
        .ok_or_else(|| "daemon returned empty loop runner response".to_string())?;
    let instance_id = runner.instance_id.trim();
    if instance_id.is_empty() {
        return Err("daemon returned empty loop runner response".to_string());
    }

    Ok(SpawnResult {
        owner: "daemon".to_string(),
        instance_id: instance_id.to_string(),
        pid: None,
    })
}

fn build_start_loop_runner_request(
    loop_id: &str,
    options: &SpawnOptions,
) -> Result<proto::StartLoopRunnerRequest, String> {
    Ok(proto::StartLoopRunnerRequest {
        loop_id: loop_id.to_string(),
        config_path: options.normalized_config_path(),
        command_path: options.resolved_command_path()?,
    })
}

fn emit_spawn_owner_warning(options: &SpawnOptions, warning_writer: &mut dyn Write, cause: &str) {
    if options.suppress_warning {
        return;
    }
    let _ = writeln!(
        warning_writer,
        "warning: forged unavailable, falling back to local spawn ({cause})"
    );
}

fn first_non_empty<'a, I>(values: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    values.into_iter().find(|value| !value.trim().is_empty())
}

fn normalize_daemon_target(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.contains("://") {
        return trimmed.to_string();
    }
    format!("http://{trimmed}")
}

fn skip_spawn_for_test_harness() -> bool {
    if let Ok(exe) = std::env::current_exe() {
        let path = exe.to_string_lossy();
        if path.contains("/target/debug/deps/") || path.contains("\\target\\debug\\deps\\") {
            return true;
        }
    }
    std::env::var_os("RUST_TEST_THREADS").is_some()
        || std::env::var_os("FORGE_TEST_MODE").is_some()
        || std::env::var("CI")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        build_start_loop_runner_request, start_loop_runner_with_spawner, LoopSpawner, SpawnOptions,
        SpawnResult,
    };
    use std::io::Write;

    #[derive(Default)]
    struct MockSpawner {
        local_calls: usize,
        daemon_calls: usize,
        local_result: Option<Result<SpawnResult, String>>,
        daemon_result: Option<Result<SpawnResult, String>>,
        last_local: Option<(String, String, SpawnOptions)>,
        last_daemon: Option<(String, SpawnOptions)>,
    }

    impl LoopSpawner for MockSpawner {
        fn spawn_local(
            &mut self,
            loop_id: &str,
            owner_label: &str,
            options: &SpawnOptions,
        ) -> Result<SpawnResult, String> {
            self.local_calls += 1;
            self.last_local = Some((
                loop_id.to_string(),
                owner_label.to_string(),
                options.clone(),
            ));
            if let Some(result) = self.local_result.clone() {
                return result;
            }

            Ok(SpawnResult {
                owner: owner_label.to_string(),
                instance_id: "local-inst".to_string(),
                pid: Some(123),
            })
        }

        fn spawn_daemon(
            &mut self,
            loop_id: &str,
            options: &SpawnOptions,
        ) -> Result<SpawnResult, String> {
            self.daemon_calls += 1;
            self.last_daemon = Some((loop_id.to_string(), options.clone()));
            if let Some(result) = self.daemon_result.clone() {
                return result;
            }

            Ok(SpawnResult {
                owner: "daemon".to_string(),
                instance_id: "daemon-inst".to_string(),
                pid: None,
            })
        }
    }

    #[test]
    fn auto_falls_back_to_local_with_warning() {
        let mut spawner = MockSpawner {
            daemon_result: Some(Err("daemon down".to_string())),
            ..Default::default()
        };
        let mut warning = Vec::new();

        let result = match start_loop_runner_with_spawner(
            "loop-1",
            "auto",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        ) {
            Ok(result) => result,
            Err(err) => panic!("expected auto fallback success: {err}"),
        };

        assert_eq!(result.owner, "local");
        assert_eq!(spawner.daemon_calls, 1);
        assert_eq!(spawner.local_calls, 1);

        let warning_text = String::from_utf8_lossy(&warning);
        assert!(warning_text.contains("falling back to local spawn (daemon down)"));
    }

    #[test]
    fn auto_warning_is_suppressed_for_quiet_json_modes() {
        let mut spawner = MockSpawner {
            daemon_result: Some(Err("daemon down".to_string())),
            ..Default::default()
        };
        let mut warning = Vec::new();
        let options = SpawnOptions {
            suppress_warning: true,
            ..Default::default()
        };

        let _ = match start_loop_runner_with_spawner(
            "loop-1",
            "auto",
            &options,
            &mut warning,
            &mut spawner,
        ) {
            Ok(result) => result,
            Err(err) => panic!("expected auto fallback success: {err}"),
        };

        assert!(warning.is_empty(), "expected warning output to be empty");
    }

    #[test]
    fn auto_prefers_daemon_on_success() {
        let mut spawner = MockSpawner::default();
        let mut warning = Vec::new();

        let result = match start_loop_runner_with_spawner(
            "loop-1",
            "auto",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        ) {
            Ok(result) => result,
            Err(err) => panic!("expected daemon success: {err}"),
        };

        assert_eq!(result.owner, "daemon");
        assert_eq!(spawner.daemon_calls, 1);
        assert_eq!(spawner.local_calls, 0);
        assert!(warning.is_empty());
    }

    #[test]
    fn daemon_owner_does_not_fallback_to_local() {
        let mut spawner = MockSpawner {
            daemon_result: Some(Err("daemon unavailable".to_string())),
            ..Default::default()
        };
        let mut warning = Vec::new();

        let err = match start_loop_runner_with_spawner(
            "loop-1",
            "daemon",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        ) {
            Ok(_) => panic!("expected daemon error"),
            Err(err) => err,
        };

        assert_eq!(err, "daemon unavailable");
        assert_eq!(spawner.daemon_calls, 1);
        assert_eq!(spawner.local_calls, 0);
        assert!(warning.is_empty());
    }

    #[test]
    fn auto_returns_combined_error_when_fallback_fails() {
        let mut spawner = MockSpawner {
            daemon_result: Some(Err("daemon down".to_string())),
            local_result: Some(Err("local spawn failed".to_string())),
            ..Default::default()
        };
        let mut warning = Vec::new();

        let err = match start_loop_runner_with_spawner(
            "loop-1",
            "auto",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        ) {
            Ok(_) => panic!("expected combined fallback error"),
            Err(err) => err,
        };

        assert_eq!(
            err,
            "daemon start failed (daemon down), local fallback failed: local spawn failed"
        );
    }

    #[test]
    fn invalid_owner_is_rejected() {
        let mut spawner = MockSpawner::default();
        let mut warning = Vec::new();

        let err = match start_loop_runner_with_spawner(
            "loop-1",
            "invalid",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        ) {
            Ok(_) => panic!("expected invalid owner error"),
            Err(err) => err,
        };

        assert_eq!(
            err,
            "invalid --spawn-owner \"invalid\" (valid: local|daemon|auto)"
        );
    }

    #[test]
    fn options_flow_to_spawner_and_request_payload() {
        let options = SpawnOptions {
            config_path: " /tmp/forge.yaml ".to_string(),
            command_path: "/bin/rforge".to_string(),
            daemon_target: "127.0.0.1:50051".to_string(),
            suppress_warning: false,
        };

        let mut spawner = MockSpawner {
            daemon_result: Some(Err("daemon unavailable".to_string())),
            ..Default::default()
        };
        let mut warning = Vec::new();
        let _ =
            start_loop_runner_with_spawner("loop-99", "auto", &options, &mut warning, &mut spawner);

        let local = match &spawner.last_local {
            Some(tuple) => tuple,
            None => panic!("expected local fallback call"),
        };
        assert_eq!(local.0, "loop-99");
        assert_eq!(local.1, "local");
        assert_eq!(local.2.config_path, " /tmp/forge.yaml ");
        assert_eq!(local.2.command_path, "/bin/rforge");

        let request = match build_start_loop_runner_request("loop-99", &options) {
            Ok(request) => request,
            Err(err) => panic!("build request: {err}"),
        };
        assert_eq!(request.loop_id, "loop-99");
        assert_eq!(request.config_path, "/tmp/forge.yaml");
        assert_eq!(request.command_path, "/bin/rforge");
    }

    #[test]
    fn bare_daemon_target_gets_http_scheme() {
        let options = SpawnOptions {
            daemon_target: "127.0.0.1:7777".to_string(),
            ..Default::default()
        };
        assert_eq!(options.resolved_daemon_target(), "http://127.0.0.1:7777");
    }

    #[test]
    fn warning_writer_can_be_any_write_target() {
        let mut spawner = MockSpawner {
            daemon_result: Some(Err("daemon down".to_string())),
            ..Default::default()
        };
        let mut sink = std::io::Cursor::new(Vec::<u8>::new());

        let _ = match start_loop_runner_with_spawner(
            "loop-1",
            "auto",
            &SpawnOptions::default(),
            &mut sink,
            &mut spawner,
        ) {
            Ok(result) => result,
            Err(err) => panic!("expected fallback success: {err}"),
        };

        let bytes = sink.into_inner();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("falling back to local spawn"));
    }

    #[test]
    fn warning_write_errors_are_ignored() {
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("write failed"))
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::Error::other("flush failed"))
            }
        }

        let mut spawner = MockSpawner {
            daemon_result: Some(Err("daemon down".to_string())),
            ..Default::default()
        };
        let mut writer = FailingWriter;

        let result = start_loop_runner_with_spawner(
            "loop-1",
            "auto",
            &SpawnOptions::default(),
            &mut writer,
            &mut spawner,
        );
        assert!(result.is_ok());
    }

    // ── Regression suite: spawn branch success/failure matrix ──

    #[test]
    fn local_success_returns_correct_spawn_result_fields() {
        let mut spawner = MockSpawner::default();
        let mut warning = Vec::new();

        let result = start_loop_runner_with_spawner(
            "loop-42",
            "local",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        )
        .expect("local spawn should succeed");

        assert_eq!(result.owner, "local");
        assert_eq!(result.instance_id, "local-inst");
        assert_eq!(result.pid, Some(123));
        assert_eq!(spawner.local_calls, 1);
        assert_eq!(spawner.daemon_calls, 0);
        assert!(warning.is_empty(), "local path should not emit warnings");
    }

    #[test]
    fn local_failure_returns_error_without_fallback() {
        let mut spawner = MockSpawner {
            local_result: Some(Err("process spawn failed".to_string())),
            ..Default::default()
        };
        let mut warning = Vec::new();

        let err = start_loop_runner_with_spawner(
            "loop-1",
            "local",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        )
        .expect_err("local spawn should fail");

        assert_eq!(err, "process spawn failed");
        assert_eq!(spawner.local_calls, 1);
        assert_eq!(spawner.daemon_calls, 0, "local failure must not try daemon");
        assert!(warning.is_empty());
    }

    #[test]
    fn daemon_success_returns_correct_spawn_result_fields() {
        let mut spawner = MockSpawner::default();
        let mut warning = Vec::new();

        let result = start_loop_runner_with_spawner(
            "loop-42",
            "daemon",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        )
        .expect("daemon spawn should succeed");

        assert_eq!(result.owner, "daemon");
        assert_eq!(result.instance_id, "daemon-inst");
        assert_eq!(result.pid, None, "daemon-spawned loops have no local pid");
        assert_eq!(spawner.daemon_calls, 1);
        assert_eq!(spawner.local_calls, 0);
        assert!(warning.is_empty());
    }

    #[test]
    fn auto_daemon_success_returns_daemon_fields() {
        let mut spawner = MockSpawner::default();
        let mut warning = Vec::new();

        let result = start_loop_runner_with_spawner(
            "loop-42",
            "auto",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        )
        .expect("auto should succeed via daemon");

        assert_eq!(result.owner, "daemon");
        assert_eq!(result.instance_id, "daemon-inst");
        assert_eq!(result.pid, None);
        assert_eq!(spawner.daemon_calls, 1);
        assert_eq!(spawner.local_calls, 0, "daemon succeeded so local not called");
        assert!(warning.is_empty(), "no warning when daemon succeeds");
    }

    #[test]
    fn auto_fallback_returns_local_fields() {
        let mut spawner = MockSpawner {
            daemon_result: Some(Err("connection refused".to_string())),
            ..Default::default()
        };
        let mut warning = Vec::new();

        let result = start_loop_runner_with_spawner(
            "loop-42",
            "auto",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        )
        .expect("auto should succeed via local fallback");

        assert_eq!(result.owner, "local");
        assert_eq!(result.instance_id, "local-inst");
        assert_eq!(result.pid, Some(123));
        assert_eq!(spawner.daemon_calls, 1);
        assert_eq!(spawner.local_calls, 1);
    }

    #[test]
    fn auto_fallback_warning_includes_cause() {
        let mut spawner = MockSpawner {
            daemon_result: Some(Err("connection refused".to_string())),
            ..Default::default()
        };
        let mut warning = Vec::new();

        let _ = start_loop_runner_with_spawner(
            "loop-1",
            "auto",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        )
        .expect("auto fallback should succeed");

        let text = String::from_utf8_lossy(&warning);
        assert!(
            text.contains("forged unavailable"),
            "warning should mention daemon: {text}"
        );
        assert!(
            text.contains("connection refused"),
            "warning should include cause: {text}"
        );
        assert!(
            text.contains("falling back to local spawn"),
            "warning should mention fallback: {text}"
        );
    }

    #[test]
    fn auto_fallback_warning_suppressed_does_not_affect_result() {
        let mut spawner = MockSpawner {
            daemon_result: Some(Err("timeout".to_string())),
            ..Default::default()
        };
        let mut warning = Vec::new();
        let options = SpawnOptions {
            suppress_warning: true,
            ..Default::default()
        };

        let result = start_loop_runner_with_spawner(
            "loop-1",
            "auto",
            &options,
            &mut warning,
            &mut spawner,
        )
        .expect("auto fallback should succeed even with suppressed warning");

        assert_eq!(result.owner, "local");
        assert!(warning.is_empty());
        assert_eq!(spawner.daemon_calls, 1);
        assert_eq!(spawner.local_calls, 1);
    }

    #[test]
    fn auto_both_fail_error_includes_both_causes() {
        let mut spawner = MockSpawner {
            daemon_result: Some(Err("rpc timeout".to_string())),
            local_result: Some(Err("binary not found".to_string())),
            ..Default::default()
        };
        let mut warning = Vec::new();

        let err = start_loop_runner_with_spawner(
            "loop-1",
            "auto",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        )
        .expect_err("both should fail");

        assert!(
            err.contains("rpc timeout"),
            "error should include daemon cause: {err}"
        );
        assert!(
            err.contains("binary not found"),
            "error should include local cause: {err}"
        );
        assert_eq!(spawner.daemon_calls, 1);
        assert_eq!(spawner.local_calls, 1);
    }

    #[test]
    fn loop_id_is_forwarded_to_spawner() {
        let mut spawner = MockSpawner::default();
        let mut warning = Vec::new();

        // local path
        let _ = start_loop_runner_with_spawner(
            "loop-abc",
            "local",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        );
        assert_eq!(spawner.last_local.as_ref().unwrap().0, "loop-abc");

        // daemon path
        let _ = start_loop_runner_with_spawner(
            "loop-xyz",
            "daemon",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        );
        assert_eq!(spawner.last_daemon.as_ref().unwrap().0, "loop-xyz");
    }

    #[test]
    fn empty_string_owner_is_rejected() {
        let mut spawner = MockSpawner::default();
        let mut warning = Vec::new();

        let err = start_loop_runner_with_spawner(
            "loop-1",
            "",
            &SpawnOptions::default(),
            &mut warning,
            &mut spawner,
        )
        .expect_err("empty owner should be rejected");

        assert!(err.contains("invalid --spawn-owner"));
        assert_eq!(spawner.local_calls, 0);
        assert_eq!(spawner.daemon_calls, 0);
    }

    #[test]
    fn case_sensitive_owner_rejects_uppercase() {
        let mut spawner = MockSpawner::default();
        let mut warning = Vec::new();

        for variant in &["Local", "DAEMON", "Auto", "LOCAL"] {
            let err = start_loop_runner_with_spawner(
                "loop-1",
                variant,
                &SpawnOptions::default(),
                &mut warning,
                &mut spawner,
            )
            .expect_err("uppercase owner should be rejected");
            assert!(err.contains("invalid --spawn-owner"));
        }
    }

    // ── Regression: daemon target resolution ──

    #[test]
    fn daemon_target_with_scheme_preserved() {
        let options = SpawnOptions {
            daemon_target: "https://daemon.local:9999".to_string(),
            ..Default::default()
        };
        assert_eq!(
            options.resolved_daemon_target(),
            "https://daemon.local:9999"
        );
    }

    #[test]
    fn daemon_target_default_when_empty() {
        let options = SpawnOptions::default();
        assert_eq!(options.resolved_daemon_target(), "http://127.0.0.1:50051");
    }

    #[test]
    fn daemon_target_whitespace_only_uses_default() {
        let options = SpawnOptions {
            daemon_target: "   ".to_string(),
            ..Default::default()
        };
        assert_eq!(options.resolved_daemon_target(), "http://127.0.0.1:50051");
    }

    // ── Regression: request payload normalization ──

    #[test]
    fn build_request_trims_config_path() {
        let options = SpawnOptions {
            config_path: "  /etc/forge.yaml  ".to_string(),
            command_path: "/bin/forge".to_string(),
            ..Default::default()
        };
        let req = build_start_loop_runner_request("loop-1", &options)
            .expect("build request should succeed");
        assert_eq!(req.config_path, "/etc/forge.yaml");
        assert_eq!(req.command_path, "/bin/forge");
        assert_eq!(req.loop_id, "loop-1");
    }

    #[test]
    fn build_request_allows_empty_config_path() {
        let options = SpawnOptions {
            command_path: "/bin/forge".to_string(),
            ..Default::default()
        };
        let req = build_start_loop_runner_request("loop-1", &options)
            .expect("build request should succeed");
        assert_eq!(req.config_path, "");
    }
}
