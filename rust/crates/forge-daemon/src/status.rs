use std::process::Command;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

use forge_rpc::forged::v1 as proto;

type TmuxHealthProbe = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

#[derive(Clone)]
pub struct StatusService {
    version: String,
    hostname: String,
    started_at: DateTime<Utc>,
    tmux_health_probe: TmuxHealthProbe,
}

impl StatusService {
    pub fn new(version: impl Into<String>, hostname: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            hostname: hostname.into(),
            started_at: Utc::now(),
            tmux_health_probe: Arc::new(default_tmux_health_probe),
        }
    }

    pub fn with_started_at(mut self, started_at: DateTime<Utc>) -> Self {
        self.started_at = started_at;
        self
    }

    pub fn with_tmux_health_probe<F>(mut self, probe: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.tmux_health_probe = Arc::new(probe);
        self
    }

    pub fn ping(&self) -> proto::PingResponse {
        proto::PingResponse {
            timestamp: Some(datetime_to_timestamp(Utc::now())),
            version: self.version.clone(),
        }
    }

    pub fn get_status(&self, agent_count: usize) -> proto::GetStatusResponse {
        let uptime = Utc::now() - self.started_at;
        let agent_count = i32::try_from(agent_count).unwrap_or(i32::MAX);

        proto::GetStatusResponse {
            status: Some(proto::DaemonStatus {
                version: self.version.clone(),
                hostname: self.hostname.clone(),
                started_at: Some(datetime_to_timestamp(self.started_at)),
                uptime: Some(duration_to_prost(uptime)),
                agent_count,
                resources: Some(self.get_resource_usage()),
                health: Some(self.get_health_status()),
            }),
        }
    }

    pub fn get_resource_usage(&self) -> proto::ResourceUsage {
        current_resource_usage()
    }

    pub fn get_health_status(&self) -> proto::HealthStatus {
        let now = Utc::now();
        let mut checks = vec![proto::HealthCheck {
            name: "tmux".to_string(),
            health: proto::Health::Healthy as i32,
            message: "tmux available".to_string(),
            last_check: Some(datetime_to_timestamp(now)),
        }];

        if let Err(err) = (self.tmux_health_probe)() {
            checks[0].health = proto::Health::Unhealthy as i32;
            checks[0].message = format!("tmux error: {err}");
        }

        proto::HealthStatus {
            health: overall_health(&checks) as i32,
            checks,
        }
    }
}

fn overall_health(checks: &[proto::HealthCheck]) -> proto::Health {
    let mut overall = proto::Health::Healthy;

    for check in checks {
        if check.health == proto::Health::Unhealthy as i32 {
            return proto::Health::Unhealthy;
        }
        if check.health == proto::Health::Degraded as i32 && overall == proto::Health::Healthy {
            overall = proto::Health::Degraded;
        }
    }

    overall
}

#[cfg(unix)]
fn current_resource_usage() -> proto::ResourceUsage {
    use nix::sys::resource::{getrusage, UsageWho};

    let usage = match getrusage(UsageWho::RUSAGE_SELF) {
        Ok(usage) => usage,
        Err(_) => return proto::ResourceUsage::default(),
    };

    let max_rss = usage.max_rss();

    let memory_bytes = match max_rss.checked_mul(1024) {
        Some(value) => value,
        None => return proto::ResourceUsage::default(),
    };

    proto::ResourceUsage {
        memory_bytes,
        ..proto::ResourceUsage::default()
    }
}

#[cfg(not(unix))]
fn current_resource_usage() -> proto::ResourceUsage {
    proto::ResourceUsage::default()
}

fn datetime_to_timestamp(dt: DateTime<Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

fn duration_to_prost(d: Duration) -> prost_types::Duration {
    // Chrono supports negative durations; split via nanoseconds with canonical remainder.
    let total_nanos = d.num_nanoseconds().unwrap_or_else(|| {
        if d < Duration::zero() {
            i64::MIN
        } else {
            i64::MAX
        }
    });

    let seconds = total_nanos / 1_000_000_000;
    let nanos = (total_nanos % 1_000_000_000) as i32;

    prost_types::Duration { seconds, nanos }
}

fn default_tmux_health_probe() -> Result<(), String> {
    let output = Command::new("tmux")
        .arg("list-sessions")
        .output()
        .map_err(|e| format!("failed to execute tmux: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        return Err(format!("tmux list-sessions failed: {}", output.status));
    }

    Err(stderr.to_string())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{overall_health, proto, StatusService};
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn ping_returns_version_and_timestamp() {
        let service =
            StatusService::new("test-version", "test-host").with_tmux_health_probe(|| Ok(()));
        let before = Utc::now();
        let resp = service.ping();
        let after = Utc::now();

        assert_eq!(resp.version, "test-version");
        let ts = resp.timestamp.expect("timestamp");
        let dt = Utc
            .timestamp_opt(ts.seconds, ts.nanos as u32)
            .single()
            .expect("timestamp valid");
        assert!(dt >= before);
        assert!(dt <= after);
    }

    #[test]
    fn get_status_reports_core_fields() {
        let started_at = Utc::now() - Duration::seconds(3);
        let service = StatusService::new("v1.2.3", "node-1")
            .with_started_at(started_at)
            .with_tmux_health_probe(|| Ok(()));

        let status = service.get_status(7).status.expect("status");

        assert_eq!(status.version, "v1.2.3");
        assert_eq!(status.hostname, "node-1");
        assert_eq!(status.agent_count, 7);
        let started = status.started_at.expect("started_at");
        assert_eq!(started.seconds, started_at.timestamp());
        assert!(status.uptime.expect("uptime").seconds >= 2);
        let health = status.health.expect("health");
        assert_eq!(health.health, proto::Health::Healthy as i32);
        assert_eq!(health.checks.len(), 1);
        assert_eq!(health.checks[0].name, "tmux");
        assert_eq!(health.checks[0].message, "tmux available");
    }

    #[test]
    fn get_status_marks_tmux_unhealthy_on_probe_error() {
        let service = StatusService::new("dev", "node")
            .with_tmux_health_probe(|| Err("dial timeout".to_string()));

        let health = service
            .get_status(0)
            .status
            .expect("status")
            .health
            .expect("health");

        assert_eq!(health.health, proto::Health::Unhealthy as i32);
        assert_eq!(health.checks.len(), 1);
        assert_eq!(health.checks[0].health, proto::Health::Unhealthy as i32);
        assert_eq!(health.checks[0].message, "tmux error: dial timeout");
    }

    #[test]
    fn overall_health_prefers_unhealthy_then_degraded() {
        let healthy = proto::HealthCheck {
            name: "a".to_string(),
            health: proto::Health::Healthy as i32,
            message: String::new(),
            last_check: None,
        };
        let degraded = proto::HealthCheck {
            name: "b".to_string(),
            health: proto::Health::Degraded as i32,
            message: String::new(),
            last_check: None,
        };
        let unhealthy = proto::HealthCheck {
            name: "c".to_string(),
            health: proto::Health::Unhealthy as i32,
            message: String::new(),
            last_check: None,
        };

        assert_eq!(
            overall_health(std::slice::from_ref(&healthy)),
            proto::Health::Healthy
        );
        assert_eq!(
            overall_health(&[healthy.clone(), degraded.clone()]),
            proto::Health::Degraded
        );
        assert_eq!(
            overall_health(&[healthy, degraded, unhealthy]),
            proto::Health::Unhealthy
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_resource_usage_reports_non_negative_memory() {
        let service = StatusService::new("dev", "node").with_tmux_health_probe(|| Ok(()));
        let usage = service.get_resource_usage();
        assert!(usage.memory_bytes >= 0);
    }
}
