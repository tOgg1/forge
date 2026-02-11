//! Shared entrypoint implementation for daemon binaries (`forged`, `rforged`).

use std::future::Future;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;

use forge_daemon::agent::AgentManager;
use forge_daemon::bootstrap::{build_daemon_options, init_logger, DaemonArgs, VersionInfo};
use forge_daemon::server::ForgedAgentService;
use forge_daemon::tmux::ShellTmuxClient;
use forge_rpc::forged::v1::forged_service_server::ForgedServiceServer;
use serde::Deserialize;
use tonic::transport::Server;

pub fn run(process_label: &str) {
    let version = VersionInfo::default();
    let args = parse_args();

    // Load merged config: defaults < config file < environment overrides.
    let (cfg, config_file_used) = match load_forge_config(&args.config_file) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("{process_label} failed to load config: {err}");
            std::process::exit(1);
        }
    };

    // Build daemon options and logging config from CLI args + config.
    let (opts, log_cfg) = build_daemon_options(&args, &cfg);
    let logger = init_logger(&log_cfg);
    let config_source = config_file_used
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "defaults+env".to_string());

    // Ensure data/config directories exist.
    if let Err(e) = cfg.ensure_directories() {
        logger.warn_with("failed to create directories", &[("error", &e.to_string())]);
    }

    // Log startup.
    logger.info_with(
        &format!("{process_label} starting"),
        &[
            ("version", &version.version),
            ("commit", &version.commit),
            ("built", &version.date),
        ],
    );

    logger.info_with(
        &format!("{process_label} ready"),
        &[("bind", &opts.bind_addr()), ("config", &config_source)],
    );

    if let Err(err) = run_grpc_server(process_label, &opts.bind_addr(), &logger) {
        logger.error_with(
            &format!("{process_label} failed"),
            &[("error", err.as_str())],
        );
        eprintln!("{process_label}: {err}");
        std::process::exit(1);
    }
}

fn run_grpc_server(
    process_label: &str,
    bind_addr: &str,
    logger: &forge_daemon::bootstrap::Logger,
) -> Result<(), String> {
    let resolved_addr = resolve_bind_addr(bind_addr)?;

    // Pre-check: try to bind the address to detect conflicts early with a
    // clear diagnostic instead of a generic tonic transport error.
    check_bind_available(resolved_addr)?;

    let service = ForgedAgentService::new(AgentManager::new(), Arc::new(ShellTmuxClient));
    let loop_runners = service.loop_runner_manager();
    let shutdown_logger = logger.clone();
    let shutdown_label = process_label.to_string();

    logger.info_with(
        &format!("{process_label} gRPC serving"),
        &[("bind", &resolved_addr.to_string())],
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("failed to initialize tokio runtime: {err}"))?;

    runtime.block_on(async move {
        let shutdown = async move {
            wait_for_shutdown_signal().await;
            shutdown_logger.info_with(
                &format!("{shutdown_label} shutdown signal received"),
                &[("signal", "SIGINT/SIGTERM")],
            );
            loop_runners.stop_all_loop_runners(true);
            shutdown_logger.info(&format!("{shutdown_label} loop runners drained"));
        };

        serve_with_shutdown(service, resolved_addr, shutdown).await
    })
}

fn check_bind_available(addr: SocketAddr) -> Result<(), String> {
    match std::net::TcpListener::bind(addr) {
        Ok(_listener) => {
            // Listener is dropped here, freeing the port for tonic to bind.
            Ok(())
        }
        Err(err) => {
            if err.kind() == std::io::ErrorKind::AddrInUse {
                Err(format!(
                    "failed to listen on {addr}: address already in use (another process may be bound to this port)"
                ))
            } else if err.kind() == std::io::ErrorKind::PermissionDenied {
                Err(format!("failed to listen on {addr}: permission denied"))
            } else {
                Err(format!("failed to listen on {addr}: {err}"))
            }
        }
    }
}

fn resolve_bind_addr(bind_addr: &str) -> Result<SocketAddr, String> {
    let mut addrs = bind_addr
        .to_socket_addrs()
        .map_err(|err| format!("failed to resolve bind address {bind_addr}: {err}"))?;
    addrs
        .next()
        .ok_or_else(|| format!("no addresses resolved for {bind_addr}"))
}

async fn serve_with_shutdown<F>(
    service: ForgedAgentService,
    bind_addr: SocketAddr,
    shutdown: F,
) -> Result<(), String>
where
    F: Future<Output = ()> + Send + 'static,
{
    Server::builder()
        .add_service(ForgedServiceServer::new(service))
        .serve_with_shutdown(bind_addr, shutdown)
        .await
        .map_err(|err| format!("gRPC server failed: {err}"))
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(stream) => stream,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn load_forge_config(
    config_file: &str,
) -> Result<(forge_core::config::Config, Option<PathBuf>), String> {
    load_forge_config_with_env(config_file, |key| std::env::var(key).ok())
}

fn load_forge_config_with_env<F>(
    config_file: &str,
    mut env_lookup: F,
) -> Result<(forge_core::config::Config, Option<PathBuf>), String>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut cfg = forge_core::config::Config::default();
    let explicit = (!config_file.trim().is_empty()).then(|| PathBuf::from(config_file.trim()));
    let path_to_try = explicit
        .clone()
        .or_else(forge_core::config::find_config_file);
    let mut loaded_path: Option<PathBuf> = None;

    if let Some(path) = path_to_try {
        match std::fs::read_to_string(&path) {
            Ok(raw) => {
                apply_partial_yaml_config(&mut cfg, &raw)?;
                loaded_path = Some(path);
            }
            Err(err) => {
                if explicit.is_some() || err.kind() != std::io::ErrorKind::NotFound {
                    return Err(format!("failed to load config file: {err}"));
                }
            }
        }
    }

    apply_env_overrides_with(&mut cfg, &mut env_lookup);
    cfg.expand_paths();
    cfg.validate()
        .map_err(|err| format!("config validation failed: {err}"))?;

    Ok((cfg, loaded_path))
}

fn apply_partial_yaml_config(
    cfg: &mut forge_core::config::Config,
    raw: &str,
) -> Result<(), String> {
    let parsed: PartialConfig =
        serde_yaml::from_str(raw).map_err(|err| format!("parse config: {err}"))?;

    if !parsed.global.data_dir.trim().is_empty() {
        cfg.global.data_dir = parsed.global.data_dir;
    }
    if !parsed.global.config_dir.trim().is_empty() {
        cfg.global.config_dir = parsed.global.config_dir;
    }
    if let Some(value) = parsed.global.auto_register_local_node {
        cfg.global.auto_register_local_node = value;
    }

    if !parsed.logging.level.trim().is_empty() {
        cfg.logging.level = parsed.logging.level;
    }
    if !parsed.logging.format.trim().is_empty() {
        cfg.logging.format = parsed.logging.format;
    }
    if !parsed.logging.file.trim().is_empty() {
        cfg.logging.file = parsed.logging.file;
    }
    if let Some(value) = parsed.logging.enable_caller {
        cfg.logging.enable_caller = value;
    }

    Ok(())
}

fn apply_env_overrides_with<F>(cfg: &mut forge_core::config::Config, env_lookup: &mut F)
where
    F: FnMut(&str) -> Option<String>,
{
    if let Some(value) = env_value(
        env_lookup,
        &["FORGE_GLOBAL_DATA_DIR", "SWARM_GLOBAL_DATA_DIR"],
    ) {
        cfg.global.data_dir = value;
    }
    if let Some(value) = env_value(
        env_lookup,
        &["FORGE_GLOBAL_CONFIG_DIR", "SWARM_GLOBAL_CONFIG_DIR"],
    ) {
        cfg.global.config_dir = value;
    }
    if let Some(value) = env_value(
        env_lookup,
        &[
            "FORGE_GLOBAL_AUTO_REGISTER_LOCAL_NODE",
            "SWARM_GLOBAL_AUTO_REGISTER_LOCAL_NODE",
        ],
    ) {
        if let Some(parsed) = parse_bool_value(&value) {
            cfg.global.auto_register_local_node = parsed;
        }
    }

    if let Some(value) = env_value(env_lookup, &["FORGE_LOGGING_LEVEL", "SWARM_LOGGING_LEVEL"]) {
        cfg.logging.level = value;
    }
    if let Some(value) = env_value(
        env_lookup,
        &["FORGE_LOGGING_FORMAT", "SWARM_LOGGING_FORMAT"],
    ) {
        cfg.logging.format = value;
    }
    if let Some(value) = env_value(env_lookup, &["FORGE_LOGGING_FILE", "SWARM_LOGGING_FILE"]) {
        cfg.logging.file = value;
    }
    if let Some(value) = env_value(
        env_lookup,
        &["FORGE_LOGGING_ENABLE_CALLER", "SWARM_LOGGING_ENABLE_CALLER"],
    ) {
        if let Some(parsed) = parse_bool_value(&value) {
            cfg.logging.enable_caller = parsed;
        }
    }
}

fn env_value<F>(env_lookup: &mut F, keys: &[&str]) -> Option<String>
where
    F: FnMut(&str) -> Option<String>,
{
    for key in keys {
        if let Some(value) = env_lookup(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn parse_bool_value(value: &str) -> Option<bool> {
    match value.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[derive(Debug, Default, Deserialize)]
struct PartialConfig {
    #[serde(default)]
    global: PartialGlobalConfig,
    #[serde(default)]
    logging: PartialLoggingConfig,
}

#[derive(Debug, Default, Deserialize)]
struct PartialGlobalConfig {
    #[serde(default)]
    data_dir: String,
    #[serde(default)]
    config_dir: String,
    auto_register_local_node: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialLoggingConfig {
    #[serde(default)]
    level: String,
    #[serde(default)]
    format: String,
    #[serde(default)]
    file: String,
    enable_caller: Option<bool>,
}

fn parse_args() -> DaemonArgs {
    let mut args = DaemonArgs::default();
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--hostname" => {
                if let Some(v) = iter.next() {
                    args.hostname = v;
                }
            }
            "--port" => {
                if let Some(v) = iter.next() {
                    if let Ok(p) = v.parse::<u16>() {
                        args.port = p;
                    }
                }
            }
            "--config" => {
                if let Some(v) = iter.next() {
                    args.config_file = v;
                }
            }
            "--log-level" => {
                if let Some(v) = iter.next() {
                    args.log_level = v;
                }
            }
            "--log-format" => {
                if let Some(v) = iter.next() {
                    args.log_format = v;
                }
            }
            "--disk-path" => {
                if let Some(v) = iter.next() {
                    args.disk_path = v;
                }
            }
            "--disk-warn" => {
                if let Some(v) = iter.next() {
                    if let Ok(f) = v.parse::<f64>() {
                        args.disk_warn = f;
                    }
                }
            }
            "--disk-critical" => {
                if let Some(v) = iter.next() {
                    if let Ok(f) = v.parse::<f64>() {
                        args.disk_critical = f;
                    }
                }
            }
            "--disk-resume" => {
                if let Some(v) = iter.next() {
                    if let Ok(f) = v.parse::<f64>() {
                        args.disk_resume = f;
                    }
                }
            }
            "--disk-pause" => {
                args.disk_pause = true;
            }
            _ => {} // Ignore unknown flags for forward-compatibility.
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use forge_daemon::agent::AgentManager;
    use forge_daemon::server::ForgedAgentService;
    use forge_daemon::tmux::TmuxClient;
    use forge_rpc::forged::v1 as proto;
    use forge_rpc::forged::v1::forged_service_client::ForgedServiceClient;
    use tonic::transport::Channel;

    use super::{
        check_bind_available, load_forge_config_with_env, resolve_bind_addr, serve_with_shutdown,
    };

    struct NoopTmux;

    impl TmuxClient for NoopTmux {
        fn send_keys(&self, _: &str, _: &str, _: bool, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn send_special_key(&self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn capture_pane(&self, _: &str, _: bool) -> Result<String, String> {
            Ok(String::new())
        }
        fn has_session(&self, _: &str) -> Result<bool, String> {
            Ok(true)
        }
        fn new_session(&self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn split_window(&self, _: &str, _: bool, _: &str) -> Result<String, String> {
            Ok("noop:0.1".to_string())
        }
        fn get_pane_pid(&self, _: &str) -> Result<i32, String> {
            Ok(0)
        }
        fn send_interrupt(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn kill_pane(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn resolve_bind_addr_accepts_ip_port() {
        let resolved = match resolve_bind_addr("127.0.0.1:50051") {
            Ok(addr) => addr,
            Err(err) => panic!("expected valid bind address, got error: {err}"),
        };

        assert_eq!(resolved.port(), 50051);
        assert_eq!(resolved.ip().to_string(), "127.0.0.1");
    }

    #[test]
    fn resolve_bind_addr_rejects_invalid_port() {
        let err = match resolve_bind_addr("127.0.0.1:bad-port") {
            Ok(addr) => panic!("expected parse failure, got address: {addr}"),
            Err(err) => err,
        };

        assert!(err.contains("failed to resolve bind address"));
    }

    #[test]
    fn load_config_explicit_file_merges_with_defaults() {
        let file = write_temp_config(
            r#"
global:
  data_dir: /tmp/forge-data
logging:
  level: debug
  format: json
"#,
        );

        let (cfg, used_path) = match load_forge_config_with_env(&file.to_string_lossy(), |_| None) {
            Ok(value) => value,
            Err(err) => panic!("expected config load to succeed: {err}"),
        };

        assert_eq!(cfg.global.data_dir, "/tmp/forge-data");
        assert_eq!(cfg.logging.level, "debug");
        assert_eq!(cfg.logging.format, "json");
        assert_eq!(cfg.database.busy_timeout_ms, 5000);

        let used = match used_path {
            Some(path) => path,
            None => panic!("expected config path to be recorded"),
        };
        assert_eq!(used, file);

        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn load_config_env_overrides_file_values() {
        let file = write_temp_config(
            r#"
global:
  data_dir: /tmp/from-file
logging:
  level: info
"#,
        );

        let env_map = std::collections::HashMap::from([
            (
                "FORGE_GLOBAL_DATA_DIR".to_string(),
                "/tmp/from-env".to_string(),
            ),
            ("FORGE_LOGGING_LEVEL".to_string(), "warn".to_string()),
        ]);

        let (cfg, _) = match load_forge_config_with_env(&file.to_string_lossy(), |key| {
            env_map.get(key).cloned()
        }) {
            Ok(value) => value,
            Err(err) => panic!("expected config load to succeed: {err}"),
        };

        assert_eq!(cfg.global.data_dir, "/tmp/from-env");
        assert_eq!(cfg.logging.level, "warn");

        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn load_config_explicit_missing_file_returns_error() {
        let missing_path = unique_temp_path("missing-config");
        let err = match load_forge_config_with_env(&missing_path.to_string_lossy(), |_| None) {
            Ok(_) => panic!("expected load to fail for missing explicit file"),
            Err(err) => err,
        };

        assert!(err.contains("failed to load config file"));
    }

    #[tokio::test]
    async fn serve_with_shutdown_handles_ping_then_exits_cleanly() {
        let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(err) => panic!("failed to reserve local test port: {err}"),
        };
        let bind_addr = match listener.local_addr() {
            Ok(addr) => addr,
            Err(err) => panic!("failed to inspect reserved local test port: {err}"),
        };
        drop(listener);

        let service = ForgedAgentService::new(AgentManager::new(), Arc::new(NoopTmux));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let serve_task = tokio::spawn(async move {
            serve_with_shutdown(service, bind_addr, async move {
                let _ = shutdown_rx.await;
            })
            .await
        });

        let mut client = match connect_with_retry(bind_addr).await {
            Ok(client) => client,
            Err(err) => panic!("failed to connect test client: {err}"),
        };

        if let Err(err) = client.ping(proto::PingRequest {}).await {
            panic!("ping request failed: {err}");
        }

        if shutdown_tx.send(()).is_err() {
            panic!("failed to trigger test shutdown");
        }

        let serve_result = match serve_task.await {
            Ok(res) => res,
            Err(err) => panic!("serve task panicked: {err}"),
        };

        if let Err(err) = serve_result {
            panic!("serve_with_shutdown returned error: {err}");
        }
    }

    #[test]
    fn check_bind_available_succeeds_on_free_port() {
        // Bind to port 0 to get a free port, then check that port is available.
        let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(l) => l,
            Err(err) => panic!("failed to bind ephemeral port: {err}"),
        };
        let addr = match listener.local_addr() {
            Ok(a) => a,
            Err(err) => panic!("failed to get local addr: {err}"),
        };
        // Drop listener to free the port.
        drop(listener);

        if let Err(err) = check_bind_available(addr) {
            panic!("expected free port to be available: {err}");
        }
    }

    #[test]
    fn check_bind_available_detects_port_conflict() {
        // Hold a listener on a port, then verify check_bind_available reports
        // a clear "address already in use" diagnostic.
        let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(l) => l,
            Err(err) => panic!("failed to bind ephemeral port: {err}"),
        };
        let addr = match listener.local_addr() {
            Ok(a) => a,
            Err(err) => panic!("failed to get local addr: {err}"),
        };

        // Port is still held by `listener` — check should fail.
        let err = match check_bind_available(addr) {
            Ok(()) => panic!("expected bind conflict, but check succeeded"),
            Err(err) => err,
        };

        assert!(
            err.contains("address already in use"),
            "expected 'address already in use' in error, got: {err}"
        );
        assert!(
            err.contains(&addr.to_string()),
            "expected address in error, got: {err}"
        );

        drop(listener);
    }

    #[test]
    fn load_config_invalid_yaml_returns_parse_error() {
        let file = write_temp_config("{{invalid yaml: [unbalanced");

        let err = match load_forge_config_with_env(&file.to_string_lossy(), |_| None) {
            Ok(_) => panic!("expected parse error for invalid YAML"),
            Err(err) => err,
        };

        assert!(
            err.contains("parse config"),
            "expected 'parse config' in error, got: {err}"
        );

        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn load_config_validation_failure_returns_clear_error() {
        // Env override sets an invalid logging level which triggers validation.
        let env_map = std::collections::HashMap::from([(
            "FORGE_LOGGING_LEVEL".to_string(),
            "bogus-level".to_string(),
        )]);

        let err = match load_forge_config_with_env("", |key| env_map.get(key).cloned()) {
            Ok(_) => panic!("expected validation error for invalid logging level"),
            Err(err) => err,
        };

        assert!(
            err.contains("config validation failed"),
            "expected 'config validation failed' in error, got: {err}"
        );
    }

    async fn connect_with_retry(
        bind_addr: SocketAddr,
    ) -> Result<ForgedServiceClient<Channel>, String> {
        let endpoint = format!("http://{bind_addr}");

        for _ in 0..20 {
            let channel = match Channel::from_shared(endpoint.clone()) {
                Ok(channel) => channel,
                Err(err) => return Err(format!("failed to build endpoint: {err}")),
            };

            match channel.connect().await {
                Ok(connection) => return Ok(ForgedServiceClient::new(connection)),
                Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
            }
        }

        Err(format!("timed out waiting for daemon on {bind_addr}"))
    }

    fn write_temp_config(raw: &str) -> PathBuf {
        let path = unique_temp_path("daemon-config");
        if let Err(err) = std::fs::write(&path, raw) {
            panic!("failed to write temp config: {err}");
        }
        path
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("{prefix}-{}.yaml", uuid::Uuid::new_v4()));
        path
    }
}
