use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Unspecified,
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthCheck {
    pub name: String,
    pub health: Health,
    pub message: String,
    pub last_check: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthStatus {
    pub health: Health,
    pub checks: Vec<HealthCheck>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_bytes: i64,
    pub memory_limit_bytes: i64,
    pub open_fds: i32,
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            cpu_percent: 0.0,
            memory_bytes: 0,
            memory_limit_bytes: 0,
            open_fds: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DaemonStatus {
    pub version: String,
    pub hostname: String,
    pub started_at: DateTime<Utc>,
    pub uptime: Duration,
    pub agent_count: i32,
    pub resources: ResourceUsage,
    pub health: HealthStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetStatusResponse {
    pub status: DaemonStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingResponse {
    pub timestamp: DateTime<Utc>,
    pub version: String,
}

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
            tmux_health_probe: Arc::new(|| Ok(())),
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

    pub fn ping(&self) -> PingResponse {
        PingResponse {
            timestamp: Utc::now(),
            version: self.version.clone(),
        }
    }

    pub fn get_status(&self, agent_count: usize) -> GetStatusResponse {
        let mut uptime = Utc::now() - self.started_at;
        if uptime < Duration::zero() {
            uptime = Duration::zero();
        }

        let agent_count = i32::try_from(agent_count).unwrap_or(i32::MAX);

        GetStatusResponse {
            status: DaemonStatus {
                version: self.version.clone(),
                hostname: self.hostname.clone(),
                started_at: self.started_at,
                uptime,
                agent_count,
                resources: self.get_resource_usage(),
                health: self.get_health_status(),
            },
        }
    }

    pub fn get_resource_usage(&self) -> ResourceUsage {
        current_resource_usage()
    }

    pub fn get_health_status(&self) -> HealthStatus {
        let now = Utc::now();
        let mut checks = vec![HealthCheck {
            name: "tmux".to_string(),
            health: Health::Healthy,
            message: "tmux available".to_string(),
            last_check: now,
        }];

        if let Err(err) = (self.tmux_health_probe)() {
            checks[0].health = Health::Unhealthy;
            checks[0].message = format!("tmux error: {err}");
        }

        HealthStatus {
            health: overall_health(&checks),
            checks,
        }
    }
}

fn overall_health(checks: &[HealthCheck]) -> Health {
    let mut overall = Health::Healthy;

    for check in checks {
        if check.health == Health::Unhealthy {
            return Health::Unhealthy;
        }
        if check.health == Health::Degraded && overall == Health::Healthy {
            overall = Health::Degraded;
        }
    }

    overall
}

#[cfg(unix)]
fn current_resource_usage() -> ResourceUsage {
    use nix::sys::resource::{getrusage, UsageWho};

    let usage = match getrusage(UsageWho::RUSAGE_SELF) {
        Ok(usage) => usage,
        Err(_) => return ResourceUsage::default(),
    };

    let max_rss = usage.max_rss();

    let memory_bytes = match max_rss.checked_mul(1024) {
        Some(value) => value,
        None => return ResourceUsage::default(),
    };

    ResourceUsage {
        memory_bytes,
        ..ResourceUsage::default()
    }
}

#[cfg(not(unix))]
fn current_resource_usage() -> ResourceUsage {
    ResourceUsage::default()
}

#[cfg(test)]
mod tests {
    use super::{overall_health, Health, HealthCheck, StatusService};
    use chrono::{Duration, Utc};

    #[test]
    fn ping_returns_version_and_timestamp() {
        let service = StatusService::new("test-version", "test-host");
        let before = Utc::now();
        let resp = service.ping();
        let after = Utc::now();

        assert_eq!(resp.version, "test-version");
        assert!(resp.timestamp >= before);
        assert!(resp.timestamp <= after);
    }

    #[test]
    fn get_status_reports_core_fields() {
        let started_at = Utc::now() - Duration::seconds(3);
        let service = StatusService::new("v1.2.3", "node-1").with_started_at(started_at);

        let status = service.get_status(7).status;

        assert_eq!(status.version, "v1.2.3");
        assert_eq!(status.hostname, "node-1");
        assert_eq!(status.started_at, started_at);
        assert_eq!(status.agent_count, 7);
        assert!(status.uptime >= Duration::seconds(2));
        assert_eq!(status.health.health, Health::Healthy);
        assert_eq!(status.health.checks.len(), 1);
        assert_eq!(status.health.checks[0].name, "tmux");
        assert_eq!(status.health.checks[0].message, "tmux available");
    }

    #[test]
    fn get_status_marks_tmux_unhealthy_on_probe_error() {
        let service = StatusService::new("dev", "node")
            .with_tmux_health_probe(|| Err("dial timeout".to_string()));

        let health = service.get_status(0).status.health;

        assert_eq!(health.health, Health::Unhealthy);
        assert_eq!(health.checks.len(), 1);
        assert_eq!(health.checks[0].health, Health::Unhealthy);
        assert_eq!(health.checks[0].message, "tmux error: dial timeout");
    }

    #[test]
    fn overall_health_prefers_unhealthy_then_degraded() {
        let healthy = HealthCheck {
            name: "a".to_string(),
            health: Health::Healthy,
            message: String::new(),
            last_check: Utc::now(),
        };
        let degraded = HealthCheck {
            name: "b".to_string(),
            health: Health::Degraded,
            message: String::new(),
            last_check: Utc::now(),
        };
        let unhealthy = HealthCheck {
            name: "c".to_string(),
            health: Health::Unhealthy,
            message: String::new(),
            last_check: Utc::now(),
        };

        assert_eq!(
            overall_health(std::slice::from_ref(&healthy)),
            Health::Healthy
        );
        assert_eq!(
            overall_health(&[healthy.clone(), degraded.clone()]),
            Health::Degraded
        );
        assert_eq!(
            overall_health(&[healthy, degraded, unhealthy]),
            Health::Unhealthy
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_resource_usage_reports_non_negative_memory() {
        let service = StatusService::new("dev", "node");
        let usage = service.get_resource_usage();
        assert!(usage.memory_bytes >= 0);
    }
}
