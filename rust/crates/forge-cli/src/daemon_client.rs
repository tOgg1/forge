use std::future::Future;
use std::time::Duration;

use forge_rpc::forged::v1 as proto;
use forge_rpc::forged::v1::forged_service_client::ForgedServiceClient;
use tonic::transport::{Channel, Endpoint};

const DEFAULT_DAEMON_ADDR: &str = "127.0.0.1:50051";
const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonClientConfig {
    pub target: Option<String>,
    pub timeout: Duration,
}

impl Default for DaemonClientConfig {
    fn default() -> Self {
        Self {
            target: None,
            timeout: DEFAULT_RPC_TIMEOUT,
        }
    }
}

pub struct DaemonClient {
    target: String,
    timeout: Duration,
    runtime: tokio::runtime::Runtime,
    client: ForgedServiceClient<Channel>,
}

impl DaemonClient {
    pub fn connect(config: DaemonClientConfig) -> Result<Self, String> {
        let target = resolve_daemon_target(config.target.as_deref())?;
        let timeout = normalize_timeout(config.timeout);
        let runtime = build_runtime()?;
        let client = runtime.block_on(connect_client(&target, timeout))?;
        Ok(Self {
            target,
            timeout,
            runtime,
            client,
        })
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn start_loop_runner(
        &mut self,
        request: proto::StartLoopRunnerRequest,
    ) -> Result<proto::StartLoopRunnerResponse, String> {
        self.runtime.block_on(async {
            with_timeout(self.timeout, "StartLoopRunner RPC", async {
                self.client
                    .start_loop_runner(request)
                    .await
                    .map(|response| response.into_inner())
                    .map_err(|err| {
                        format!(
                            "failed to start loop runner via forged daemon at {}: {err}",
                            self.target
                        )
                    })
            })
            .await
        })
    }
}

pub fn resolve_daemon_target(explicit_target: Option<&str>) -> Result<String, String> {
    let env_target = std::env::var("FORGED_ADDR").ok();
    resolve_daemon_target_with_env(explicit_target, env_target.as_deref())
}

fn resolve_daemon_target_with_env(
    explicit_target: Option<&str>,
    env_target: Option<&str>,
) -> Result<String, String> {
    let candidate = explicit_target
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| env_target.map(str::trim).filter(|value| !value.is_empty()))
        .unwrap_or(DEFAULT_DAEMON_ADDR);
    normalize_target(candidate)
}

fn normalize_target(target: &str) -> Result<String, String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err("daemon target cannot be empty".to_string());
    }
    let normalized = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    Endpoint::from_shared(normalized.clone())
        .map_err(|err| format!("invalid daemon target {trimmed:?}: {err}"))?;
    Ok(normalized)
}

fn normalize_timeout(timeout: Duration) -> Duration {
    if timeout.is_zero() {
        DEFAULT_RPC_TIMEOUT
    } else {
        timeout
    }
}

fn build_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("initialize daemon RPC runtime: {err}"))
}

async fn connect_client(
    target: &str,
    timeout: Duration,
) -> Result<ForgedServiceClient<Channel>, String> {
    let endpoint = Endpoint::from_shared(target.to_string())
        .map_err(|err| format!("invalid daemon target {target:?}: {err}"))?
        .connect_timeout(timeout)
        .timeout(timeout);
    let channel = endpoint
        .connect()
        .await
        .map_err(|err| format!("failed to connect to forged daemon at {target}: {err}"))?;
    let mut client = ForgedServiceClient::new(channel);
    preflight_ping(&mut client, timeout).await?;
    Ok(client)
}

async fn preflight_ping(
    client: &mut ForgedServiceClient<Channel>,
    timeout: Duration,
) -> Result<(), String> {
    with_timeout(timeout, "forged daemon ping", async {
        client
            .ping(proto::PingRequest {})
            .await
            .map(|_| ())
            .map_err(|err| format!("forged daemon ping failed: {err}"))
    })
    .await
}

async fn with_timeout<T, F>(timeout: Duration, operation: &str, fut: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    match tokio::time::timeout(timeout, fut).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(err),
        Err(_) => Err(format!(
            "{operation} timed out after {}ms",
            timeout.as_millis()
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::time::Duration;

    use super::{
        build_runtime, resolve_daemon_target_with_env, with_timeout, DaemonClient,
        DaemonClientConfig,
    };

    #[test]
    fn target_resolution_prefers_explicit_over_env() {
        let target = match resolve_daemon_target_with_env(Some("127.0.0.1:6001"), Some("env:1")) {
            Ok(value) => value,
            Err(err) => panic!("resolve target should succeed: {err}"),
        };
        assert_eq!(target, "http://127.0.0.1:6001");
    }

    #[test]
    fn target_resolution_uses_env_then_default() {
        let from_env = match resolve_daemon_target_with_env(None, Some("127.0.0.1:6002")) {
            Ok(value) => value,
            Err(err) => panic!("resolve target from env should succeed: {err}"),
        };
        assert_eq!(from_env, "http://127.0.0.1:6002");

        let default_target = match resolve_daemon_target_with_env(None, None) {
            Ok(value) => value,
            Err(err) => panic!("resolve default target should succeed: {err}"),
        };
        assert_eq!(default_target, "http://127.0.0.1:50051");
    }

    #[test]
    fn target_resolution_preserves_scheme() {
        let target = match resolve_daemon_target_with_env(Some("http://127.0.0.1:50051"), None) {
            Ok(value) => value,
            Err(err) => panic!("resolve target with scheme should succeed: {err}"),
        };
        assert_eq!(target, "http://127.0.0.1:50051");
    }

    #[test]
    fn connect_surfaces_dial_errors() {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(value) => value,
            Err(err) => panic!("reserve test port: {err}"),
        };
        let addr = match listener.local_addr() {
            Ok(value) => value,
            Err(err) => panic!("read test port: {err}"),
        };
        drop(listener);

        let err = match DaemonClient::connect(DaemonClientConfig {
            target: Some(addr.to_string()),
            timeout: Duration::from_millis(100),
        }) {
            Ok(client) => panic!("expected dial error, connected to {}", client.target()),
            Err(err) => err,
        };
        assert!(
            err.contains("failed to connect to forged daemon"),
            "unexpected dial error: {err}"
        );
    }

    #[test]
    fn timeout_handling_reports_elapsed_operation() {
        let runtime = match build_runtime() {
            Ok(value) => value,
            Err(err) => panic!("runtime should build: {err}"),
        };
        let err = runtime.block_on(async {
            match with_timeout(Duration::from_millis(10), "test operation", async {
                tokio::time::sleep(Duration::from_millis(40)).await;
                Ok::<(), String>(())
            })
            .await
            {
                Ok(()) => panic!("expected timeout"),
                Err(err) => err,
            }
        });
        assert!(
            err.contains("test operation timed out"),
            "unexpected timeout error: {err}"
        );
    }
}
