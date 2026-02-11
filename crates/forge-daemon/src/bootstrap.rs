//! Daemon bootstrap, configuration, constants, and logging for forged.
//!
//! Mirrors Go `internal/forged/constants.go`, `internal/forged/daemon.go` (New/Run/shutdown),
//! `internal/logging/logger.go`, and `internal/logging/redact.go`.

use std::fmt;
use std::io::Write;

// ---------------------------------------------------------------------------
// Constants (mirrors Go internal/forged/constants.go)
// ---------------------------------------------------------------------------

/// Default bind host for the forged gRPC server.
pub const DEFAULT_HOST: &str = "127.0.0.1";

/// Default gRPC port for the forged server.
pub const DEFAULT_PORT: u16 = 50051;

/// Default TCP port for the Forge Mail server.
pub const DEFAULT_MAIL_PORT: u16 = 7463;

// ---------------------------------------------------------------------------
// DiskMonitorConfig (mirrors Go internal/forged/resource_monitor.go)
// ---------------------------------------------------------------------------

/// Disk usage monitoring configuration.
#[derive(Debug, Clone)]
pub struct DiskMonitorConfig {
    /// Filesystem path to monitor.
    pub path: String,
    /// Warn at or above this percentage (0 = disabled).
    pub warn_percent: f64,
    /// Critical state at or above this percentage (0 = disabled).
    pub critical_percent: f64,
    /// Resume paused agents below this percentage (0 = defaults to warn_percent).
    pub resume_percent: f64,
    /// Whether to pause agents when disk is critically full.
    pub pause_agents: bool,
}

impl Default for DiskMonitorConfig {
    fn default() -> Self {
        Self {
            path: "/".into(),
            warn_percent: 85.0,
            critical_percent: 95.0,
            resume_percent: 90.0,
            pause_agents: false,
        }
    }
}

/// Resource limits for agents.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_memory_bytes: i64,
    pub max_cpu_percent: f64,
    pub grace_period_seconds: i32,
    pub warn_threshold_percent: f64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
            max_cpu_percent: 200.0,                   // 2 CPU cores
            grace_period_seconds: 30,
            warn_threshold_percent: 80.0,
        }
    }
}

// ---------------------------------------------------------------------------
// DaemonOptions (mirrors Go internal/forged/daemon.go Options)
// ---------------------------------------------------------------------------

/// Runtime options for the daemon.
#[derive(Debug, Clone)]
pub struct DaemonOptions {
    pub hostname: String,
    pub port: u16,
    pub mail_port: i32,
    pub version: String,
    pub rate_limit_enabled: Option<bool>,
    pub resource_monitor_enabled: Option<bool>,
    pub disk_monitor_config: Option<DiskMonitorConfig>,
    pub default_resource_limits: Option<ResourceLimits>,
    pub disable_database: bool,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            hostname: DEFAULT_HOST.into(),
            port: DEFAULT_PORT,
            mail_port: DEFAULT_MAIL_PORT as i32,
            version: "dev".into(),
            rate_limit_enabled: None,
            resource_monitor_enabled: None,
            disk_monitor_config: None,
            default_resource_limits: None,
            disable_database: false,
        }
    }
}

impl DaemonOptions {
    /// Returns the effective hostname (default if empty).
    pub fn effective_hostname(&self) -> &str {
        if self.hostname.is_empty() {
            DEFAULT_HOST
        } else {
            &self.hostname
        }
    }

    /// Returns the effective port (default if zero).
    pub fn effective_port(&self) -> u16 {
        if self.port == 0 {
            DEFAULT_PORT
        } else {
            self.port
        }
    }

    /// Returns the bind address as "host:port".
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.effective_hostname(), self.effective_port())
    }
}

// ---------------------------------------------------------------------------
// Log level (mirrors Go internal/logging/logger.go)
// ---------------------------------------------------------------------------

/// Log level (mirrors Go zerolog levels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    /// Parse a log level string (case-insensitive, defaults to Info).
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "trace" => Self::Trace,
            "debug" => Self::Debug,
            "info" => Self::Info,
            "warn" | "warning" => Self::Warn,
            "error" => Self::Error,
            "fatal" => Self::Fatal,
            _ => Self::Info,
        }
    }

    /// Returns true if a message at `msg_level` should be logged given this filter level.
    pub fn should_log(self, msg_level: LogLevel) -> bool {
        msg_level >= self
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        };
        f.write_str(s)
    }
}

// ---------------------------------------------------------------------------
// Log format
// ---------------------------------------------------------------------------

/// Output format for logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Console,
    Json,
}

impl LogFormat {
    /// Parse a format string (defaults to Console).
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "json" => Self::Json,
            _ => Self::Console,
        }
    }
}

// ---------------------------------------------------------------------------
// Logger (structured, mirrors Go zerolog-based logger)
// ---------------------------------------------------------------------------

/// Configuration for the logging subsystem.
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub format: LogFormat,
    pub enable_caller: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            format: LogFormat::Console,
            enable_caller: false,
        }
    }
}

/// A structured logger with component/field context.
#[derive(Debug, Clone)]
pub struct Logger {
    level: LogLevel,
    format: LogFormat,
    #[allow(dead_code)] // Will add caller info to output in future
    enable_caller: bool,
    fields: Vec<(String, String)>,
}

impl Logger {
    /// Create a new logger from the given config.
    pub fn new(cfg: &LoggingConfig) -> Self {
        Self {
            level: cfg.level,
            format: cfg.format,
            enable_caller: cfg.enable_caller,
            fields: Vec::new(),
        }
    }

    /// Create a child logger with an additional field.
    pub fn with_field(&self, key: &str, value: &str) -> Self {
        let mut child = self.clone();
        child.fields.push((key.to_string(), value.to_string()));
        child
    }

    /// Create a child logger for a named component.
    pub fn component(&self, name: &str) -> Self {
        self.with_field("component", name)
    }

    /// Create a child logger with node context.
    pub fn with_node(&self, node_id: &str) -> Self {
        self.with_field("node_id", node_id)
    }

    /// Create a child logger with workspace context.
    pub fn with_workspace(&self, workspace_id: &str) -> Self {
        self.with_field("workspace_id", workspace_id)
    }

    /// Create a child logger with agent context.
    pub fn with_agent(&self, agent_id: &str) -> Self {
        self.with_field("agent_id", agent_id)
    }

    /// Log a message at the given level.
    pub fn log(&self, level: LogLevel, msg: &str) {
        self.log_with_fields(level, msg, &[]);
    }

    /// Log a message with extra inline fields.
    pub fn log_with_fields(&self, level: LogLevel, msg: &str, extra: &[(&str, &str)]) {
        if !self.level.should_log(level) {
            return;
        }

        let stderr = std::io::stderr();
        let mut handle = stderr.lock();

        match self.format {
            LogFormat::Console => {
                let now = chrono::Utc::now().format("%H:%M:%S");
                let _ = write!(handle, "{now} {level} ");
                for (k, v) in &self.fields {
                    let _ = write!(handle, "{k}={v} ");
                }
                for (k, v) in extra {
                    let _ = write!(handle, "{k}={v} ");
                }
                let _ = writeln!(handle, "{msg}");
            }
            LogFormat::Json => {
                // Minimal JSON structured output
                let _ = write!(
                    handle,
                    "{{\"time\":\"{}\",\"level\":\"{}\"",
                    chrono::Utc::now().to_rfc3339(),
                    level,
                );
                for (k, v) in &self.fields {
                    let _ = write!(handle, ",\"{k}\":\"{v}\"");
                }
                for (k, v) in extra {
                    let _ = write!(handle, ",\"{k}\":\"{v}\"");
                }
                let _ = writeln!(handle, ",\"message\":\"{msg}\"}}");
            }
        }
    }

    // Convenience methods

    pub fn trace(&self, msg: &str) {
        self.log(LogLevel::Trace, msg);
    }

    pub fn debug(&self, msg: &str) {
        self.log(LogLevel::Debug, msg);
    }

    pub fn info(&self, msg: &str) {
        self.log(LogLevel::Info, msg);
    }

    pub fn warn(&self, msg: &str) {
        self.log(LogLevel::Warn, msg);
    }

    pub fn error(&self, msg: &str) {
        self.log(LogLevel::Error, msg);
    }

    pub fn info_with(&self, msg: &str, fields: &[(&str, &str)]) {
        self.log_with_fields(LogLevel::Info, msg, fields);
    }

    pub fn debug_with(&self, msg: &str, fields: &[(&str, &str)]) {
        self.log_with_fields(LogLevel::Debug, msg, fields);
    }

    pub fn warn_with(&self, msg: &str, fields: &[(&str, &str)]) {
        self.log_with_fields(LogLevel::Warn, msg, fields);
    }

    pub fn error_with(&self, msg: &str, fields: &[(&str, &str)]) {
        self.log_with_fields(LogLevel::Error, msg, fields);
    }
}

// ---------------------------------------------------------------------------
// Redaction (mirrors Go internal/logging/redact.go)
// ---------------------------------------------------------------------------

/// Sensitive field names that should be redacted.
const SENSITIVE_FIELDS: &[&str] = &[
    "password",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "authorization",
    "auth",
    "credential",
    "private_key",
    "privatekey",
    "access_key",
    "accesskey",
];

/// Replacement value for redacted data.
pub const REDACTED_VALUE: &str = "[REDACTED]";

/// Check if a field name is considered sensitive.
pub fn is_sensitive_field(name: &str) -> bool {
    let lower = name.to_lowercase();
    SENSITIVE_FIELDS.iter().any(|f| lower.contains(f))
}

/// Redact sensitive patterns in a string value.
pub fn redact(s: &str) -> String {
    let mut result = s.to_string();

    // OpenAI-style keys
    if result.contains("sk-") {
        result = redact_pattern(&result, "sk-", 20);
    }
    // Anthropic-style keys
    if result.contains("anthropic-") {
        result = redact_pattern(&result, "anthropic-", 20);
    }
    // Google API keys
    if result.contains("AIza") {
        result = redact_pattern(&result, "AIza", 35);
    }
    // GitHub PATs
    for prefix in &["ghp_", "gho_", "github_pat_"] {
        if result.contains(prefix) {
            result = redact_pattern(&result, prefix, 22);
        }
    }

    result
}

/// Redact env vars, returning a safe copy.
pub fn redact_env(env: &[String]) -> Vec<String> {
    env.iter()
        .map(|e| {
            if let Some((key, val)) = e.split_once('=') {
                if is_sensitive_field(key) {
                    format!("{key}={REDACTED_VALUE}")
                } else {
                    format!("{key}={}", redact(val))
                }
            } else {
                e.clone()
            }
        })
        .collect()
}

fn redact_pattern(s: &str, prefix: &str, min_suffix_len: usize) -> String {
    let mut result = String::new();
    let mut rest = s;
    while let Some(idx) = rest.find(prefix) {
        result.push_str(&rest[..idx]);
        let after = &rest[idx + prefix.len()..];
        let token_len = after
            .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .unwrap_or(after.len());
        if token_len >= min_suffix_len {
            result.push_str(REDACTED_VALUE);
        } else {
            result.push_str(&rest[idx..idx + prefix.len() + token_len]);
        }
        rest = &rest[idx + prefix.len() + token_len..];
    }
    result.push_str(rest);
    result
}

// ---------------------------------------------------------------------------
// Version info (mirrors Go cmd/forged/main.go linker vars)
// ---------------------------------------------------------------------------

/// Build information injected at compile time or defaulting to "dev".
#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub commit: String,
    pub date: String,
}

impl Default for VersionInfo {
    fn default() -> Self {
        Self {
            version: option_env!("FORGE_VERSION").unwrap_or("dev").to_string(),
            commit: option_env!("FORGE_COMMIT").unwrap_or("none").to_string(),
            date: option_env!("FORGE_BUILD_DATE")
                .unwrap_or("unknown")
                .to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Bootstrap helper (mirrors Go cmd/forged/main.go bootstrap sequence)
// ---------------------------------------------------------------------------

/// Parsed daemon CLI arguments.
#[derive(Debug, Clone)]
pub struct DaemonArgs {
    pub hostname: String,
    pub port: u16,
    pub config_file: String,
    pub log_level: String,
    pub log_format: String,
    pub disk_path: String,
    pub disk_warn: f64,
    pub disk_critical: f64,
    pub disk_resume: f64,
    pub disk_pause: bool,
}

impl Default for DaemonArgs {
    fn default() -> Self {
        let disk = DiskMonitorConfig::default();
        Self {
            hostname: DEFAULT_HOST.into(),
            port: DEFAULT_PORT,
            config_file: String::new(),
            log_level: String::new(),
            log_format: String::new(),
            disk_path: String::new(),
            disk_warn: disk.warn_percent,
            disk_critical: disk.critical_percent,
            disk_resume: disk.resume_percent,
            disk_pause: disk.pause_agents,
        }
    }
}

/// Build a [`DaemonOptions`] and [`LoggingConfig`] from parsed CLI args and
/// the loaded Forge config. This mirrors the Go `main()` bootstrap:
///   1. CLI args override config-file values for log-level/format.
///   2. Disk monitor config is assembled from config + CLI flags.
pub fn build_daemon_options(
    args: &DaemonArgs,
    cfg: &forge_core::config::Config,
) -> (DaemonOptions, LoggingConfig) {
    // Resolve effective log level/format (CLI overrides config).
    let log_level_str = if args.log_level.is_empty() {
        &cfg.logging.level
    } else {
        &args.log_level
    };
    let log_format_str = if args.log_format.is_empty() {
        &cfg.logging.format
    } else {
        &args.log_format
    };

    let log_cfg = LoggingConfig {
        level: LogLevel::parse(log_level_str),
        format: LogFormat::parse(log_format_str),
        enable_caller: cfg.logging.enable_caller,
    };

    // Disk monitor config
    let mut disk = DiskMonitorConfig::default();
    if !cfg.global.data_dir.is_empty() {
        disk.path = cfg.global.data_dir.clone();
    }
    if !args.disk_path.is_empty() {
        disk.path = args.disk_path.clone();
    }
    disk.warn_percent = args.disk_warn;
    disk.critical_percent = args.disk_critical;
    disk.resume_percent = args.disk_resume;
    disk.pause_agents = args.disk_pause;

    let opts = DaemonOptions {
        hostname: args.hostname.clone(),
        port: args.port,
        disk_monitor_config: Some(disk),
        ..DaemonOptions::default()
    };

    (opts, log_cfg)
}

/// Convenience: create a logger from the bootstrap config, with component "forged".
pub fn init_logger(cfg: &LoggingConfig) -> Logger {
    Logger::new(cfg).component("forged")
}

// ---------------------------------------------------------------------------
// Shutdown ordering
// ---------------------------------------------------------------------------

/// Describes the ordered shutdown sequence for the daemon.
/// Components should be stopped in this order (mirrors Go daemon.shutdown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownPhase {
    Scheduler,
    EventWatcher,
    StatePoller,
    LoopRunners,
    GrpcServer,
    MailServers,
    ResourceMonitor,
    Database,
}

impl ShutdownPhase {
    /// Returns all phases in the correct shutdown order.
    pub fn ordered() -> &'static [ShutdownPhase] {
        &[
            Self::Scheduler,
            Self::EventWatcher,
            Self::StatePoller,
            Self::LoopRunners,
            Self::GrpcServer,
            Self::MailServers,
            Self::ResourceMonitor,
            Self::Database,
        ]
    }
}

impl fmt::Display for ShutdownPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Scheduler => "scheduler",
            Self::EventWatcher => "event watcher",
            Self::StatePoller => "state poller",
            Self::LoopRunners => "loop runners",
            Self::GrpcServer => "gRPC server",
            Self::MailServers => "mail servers",
            Self::ResourceMonitor => "resource monitor",
            Self::Database => "database",
        };
        f.write_str(name)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_constants() {
        assert_eq!(DEFAULT_HOST, "127.0.0.1");
        assert_eq!(DEFAULT_PORT, 50051);
        assert_eq!(DEFAULT_MAIL_PORT, 7463);
    }

    #[test]
    fn daemon_options_defaults() {
        let opts = DaemonOptions::default();
        assert_eq!(opts.hostname, DEFAULT_HOST);
        assert_eq!(opts.port, DEFAULT_PORT);
        assert_eq!(opts.mail_port, DEFAULT_MAIL_PORT as i32);
        assert_eq!(opts.version, "dev");
        assert!(!opts.disable_database);
    }

    #[test]
    fn bind_addr_default() {
        let opts = DaemonOptions::default();
        assert_eq!(opts.bind_addr(), "127.0.0.1:50051");
    }

    #[test]
    fn bind_addr_custom() {
        let opts = DaemonOptions {
            hostname: "0.0.0.0".into(),
            port: 9090,
            ..DaemonOptions::default()
        };
        assert_eq!(opts.bind_addr(), "0.0.0.0:9090");
    }

    #[test]
    fn bind_addr_empty_hostname_uses_default() {
        let opts = DaemonOptions {
            hostname: String::new(),
            ..DaemonOptions::default()
        };
        assert_eq!(opts.effective_hostname(), DEFAULT_HOST);
    }

    #[test]
    fn bind_addr_zero_port_uses_default() {
        let opts = DaemonOptions {
            port: 0,
            ..DaemonOptions::default()
        };
        assert_eq!(opts.effective_port(), DEFAULT_PORT);
    }

    #[test]
    fn disk_monitor_defaults() {
        let cfg = DiskMonitorConfig::default();
        assert_eq!(cfg.path, "/");
        assert!((cfg.warn_percent - 85.0).abs() < f64::EPSILON);
        assert!((cfg.critical_percent - 95.0).abs() < f64::EPSILON);
        assert!((cfg.resume_percent - 90.0).abs() < f64::EPSILON);
        assert!(!cfg.pause_agents);
    }

    #[test]
    fn resource_limits_defaults() {
        let lim = ResourceLimits::default();
        assert_eq!(lim.max_memory_bytes, 2 * 1024 * 1024 * 1024);
        assert!((lim.max_cpu_percent - 200.0).abs() < f64::EPSILON);
        assert_eq!(lim.grace_period_seconds, 30);
        assert!((lim.warn_threshold_percent - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn log_level_parse() {
        assert_eq!(LogLevel::parse("debug"), LogLevel::Debug);
        assert_eq!(LogLevel::parse("INFO"), LogLevel::Info);
        assert_eq!(LogLevel::parse("warn"), LogLevel::Warn);
        assert_eq!(LogLevel::parse("warning"), LogLevel::Warn);
        assert_eq!(LogLevel::parse("error"), LogLevel::Error);
        assert_eq!(LogLevel::parse("fatal"), LogLevel::Fatal);
        assert_eq!(LogLevel::parse("trace"), LogLevel::Trace);
        assert_eq!(LogLevel::parse("bogus"), LogLevel::Info);
        assert_eq!(LogLevel::parse(""), LogLevel::Info);
    }

    #[test]
    fn log_level_filtering() {
        assert!(LogLevel::Info.should_log(LogLevel::Info));
        assert!(LogLevel::Info.should_log(LogLevel::Warn));
        assert!(LogLevel::Info.should_log(LogLevel::Error));
        assert!(!LogLevel::Info.should_log(LogLevel::Debug));
        assert!(!LogLevel::Info.should_log(LogLevel::Trace));
    }

    #[test]
    fn log_format_parse() {
        assert_eq!(LogFormat::parse("json"), LogFormat::Json);
        assert_eq!(LogFormat::parse("JSON"), LogFormat::Json);
        assert_eq!(LogFormat::parse("console"), LogFormat::Console);
        assert_eq!(LogFormat::parse("anything"), LogFormat::Console);
    }

    #[test]
    fn sensitive_field_detection() {
        assert!(is_sensitive_field("api_key"));
        assert!(is_sensitive_field("API_KEY"));
        assert!(is_sensitive_field("my_secret_token"));
        assert!(is_sensitive_field("password"));
        assert!(is_sensitive_field("AUTHORIZATION"));
        assert!(!is_sensitive_field("username"));
        assert!(!is_sensitive_field("hostname"));
    }

    #[test]
    fn redact_api_keys() {
        let input = "key is sk-abcdefghijklmnopqrstuvwxyz1234567890 here";
        let out = redact(input);
        assert!(out.contains(REDACTED_VALUE));
        assert!(!out.contains("sk-abcdefghijklmnopqrstuvwxyz1234567890"));
    }

    #[test]
    fn redact_env_vars() {
        let env = vec![
            "HOME=/home/user".to_string(),
            "API_KEY=sk-abcdefghijklmnopqrstuvwxyz1234567890".to_string(),
            "MY_SECRET=super-secret-value-that-is-long-enough".to_string(),
        ];
        let redacted = redact_env(&env);
        assert_eq!(redacted[0], "HOME=/home/user");
        assert!(redacted[1].contains(REDACTED_VALUE));
        assert!(redacted[2].contains(REDACTED_VALUE));
    }

    #[test]
    fn redact_short_tokens_untouched() {
        let input = "sk-short";
        let out = redact(input);
        assert_eq!(out, "sk-short");
    }

    #[test]
    fn version_info_defaults() {
        let v = VersionInfo::default();
        assert!(!v.version.is_empty());
        assert!(!v.commit.is_empty());
        assert!(!v.date.is_empty());
    }

    #[test]
    fn daemon_args_defaults() {
        let args = DaemonArgs::default();
        assert_eq!(args.hostname, DEFAULT_HOST);
        assert_eq!(args.port, DEFAULT_PORT);
        assert!(args.config_file.is_empty());
        assert!(args.log_level.is_empty());
        assert!(args.log_format.is_empty());
    }

    #[test]
    fn build_daemon_options_cli_overrides_config() {
        let cfg = forge_core::config::Config::default();
        let args = DaemonArgs {
            log_level: "debug".into(),
            log_format: "json".into(),
            disk_path: "/data".into(),
            ..DaemonArgs::default()
        };
        let (opts, log_cfg) = build_daemon_options(&args, &cfg);
        assert_eq!(log_cfg.level, LogLevel::Debug);
        assert_eq!(log_cfg.format, LogFormat::Json);
        match &opts.disk_monitor_config {
            Some(d) => assert_eq!(d.path, "/data"),
            None => panic!("expected disk monitor config"),
        }
    }

    #[test]
    fn build_daemon_options_uses_config_when_cli_empty() {
        let cfg = forge_core::config::Config::default();
        let args = DaemonArgs::default();
        let (_opts, log_cfg) = build_daemon_options(&args, &cfg);
        assert_eq!(log_cfg.level, LogLevel::Info);
        assert_eq!(log_cfg.format, LogFormat::Console);
    }

    #[test]
    fn shutdown_phase_ordering() {
        let phases = ShutdownPhase::ordered();
        assert_eq!(phases.len(), 8);
        assert_eq!(phases[0], ShutdownPhase::Scheduler);
        assert_eq!(phases[7], ShutdownPhase::Database);
    }

    #[test]
    fn shutdown_phase_display() {
        assert_eq!(ShutdownPhase::GrpcServer.to_string(), "gRPC server");
        assert_eq!(ShutdownPhase::Database.to_string(), "database");
    }

    #[test]
    fn logger_creates_child() {
        let cfg = LoggingConfig::default();
        let logger = Logger::new(&cfg);
        let child = logger.component("test");
        assert_eq!(child.fields.len(), 1);
        assert_eq!(child.fields[0].0, "component");
        assert_eq!(child.fields[0].1, "test");
    }

    #[test]
    fn logger_with_multiple_fields() {
        let cfg = LoggingConfig::default();
        let logger = Logger::new(&cfg)
            .component("forged")
            .with_node("node-1")
            .with_workspace("ws-1")
            .with_agent("agent-1");
        assert_eq!(logger.fields.len(), 4);
    }

    #[test]
    fn logger_level_filtering() {
        // Logger at Warn level should not emit Debug
        let cfg = LoggingConfig {
            level: LogLevel::Warn,
            ..LoggingConfig::default()
        };
        let logger = Logger::new(&cfg);
        assert!(!logger.level.should_log(LogLevel::Debug));
        assert!(logger.level.should_log(LogLevel::Warn));
        assert!(logger.level.should_log(LogLevel::Error));
    }

    #[test]
    fn init_logger_returns_forged_component() {
        let cfg = LoggingConfig::default();
        let logger = init_logger(&cfg);
        assert!(logger
            .fields
            .iter()
            .any(|(k, v)| k == "component" && v == "forged"));
    }
}
