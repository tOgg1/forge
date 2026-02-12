//! Configuration types for the Forge system.
//!
//! Mirrors Go `internal/config/` — root configuration struct and nested
//! section types with full defaults, validation, YAML file loading,
//! environment variable overrides, and tilde path expansion.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Root config
// ---------------------------------------------------------------------------

/// Root configuration for the Forge system.
#[derive(Debug, Clone)]
pub struct Config {
    pub global: GlobalConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
    pub accounts: Vec<AccountConfig>,
    pub profiles: Vec<ProfileConfig>,
    pub pools: Vec<PoolConfig>,
    pub default_pool: String,
    pub node_defaults: NodeConfig,
    pub workspace_defaults: WorkspaceConfig,
    pub workspace_overrides: Vec<WorkspaceOverrideConfig>,
    pub agent_defaults: AgentConfig,
    pub scheduler: SchedulerConfig,
    pub loop_defaults: LoopDefaultsConfig,
    pub tui: TuiConfig,
    pub mail: MailConfig,
    pub event_retention: EventRetentionConfig,
}

impl Default for Config {
    fn default() -> Self {
        let home = home_dir();
        Self {
            global: GlobalConfig {
                data_dir: home.join(".local/share/forge").display().to_string(),
                config_dir: home.join(".config/forge").display().to_string(),
                auto_register_local_node: true,
            },
            database: DatabaseConfig::default(),
            logging: LoggingConfig::default(),
            accounts: Vec::new(),
            profiles: Vec::new(),
            pools: Vec::new(),
            default_pool: String::new(),
            node_defaults: NodeConfig::default(),
            workspace_defaults: WorkspaceConfig::default(),
            workspace_overrides: Vec::new(),
            agent_defaults: AgentConfig::default(),
            scheduler: SchedulerConfig::default(),
            loop_defaults: LoopDefaultsConfig::default(),
            tui: TuiConfig::default(),
            mail: MailConfig::default(),
            event_retention: EventRetentionConfig::default(),
        }
    }
}

impl Config {
    /// Returns the effective database path (explicit or derived from data_dir).
    pub fn database_path(&self) -> String {
        if !self.database.path.is_empty() {
            return self.database.path.clone();
        }
        let p: PathBuf = [&self.global.data_dir, "forge.db"].iter().collect();
        p.display().to_string()
    }

    /// Returns the effective archive directory path.
    pub fn archive_path(&self) -> String {
        if !self.event_retention.archive_dir.is_empty() {
            return self.event_retention.archive_dir.clone();
        }
        let p: PathBuf = [&self.global.data_dir, "archives"].iter().collect();
        p.display().to_string()
    }

    /// Creates required directories (data_dir, config_dir).
    pub fn ensure_directories(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.global.data_dir)?;
        std::fs::create_dir_all(&self.global.config_dir)?;
        Ok(())
    }

    /// Validates the entire configuration, returning an error message on failure.
    pub fn validate(&self) -> Result<(), String> {
        // Global
        if self.global.data_dir.trim().is_empty() {
            return Err("global.data_dir is required".into());
        }
        if self.global.config_dir.trim().is_empty() {
            return Err("global.config_dir is required".into());
        }

        // Database
        if self.database.max_connections < 1 {
            return Err("database.max_connections must be at least 1".into());
        }
        if self.database.busy_timeout_ms < 0 {
            return Err("database.busy_timeout_ms must be zero or greater".into());
        }

        // Logging
        match self.logging.level.to_lowercase().trim() {
            "debug" | "info" | "warn" | "error" => {}
            _ => return Err("logging.level must be one of debug, info, warn, error".into()),
        }
        match self.logging.format.to_lowercase().trim() {
            "console" | "json" => {}
            _ => return Err("logging.format must be one of console, json".into()),
        }

        // Node defaults
        match self.node_defaults.ssh_backend.as_str() {
            "native" | "system" | "auto" => {}
            _ => return Err("node_defaults.ssh_backend must be native, system, or auto".into()),
        }
        if self.node_defaults.ssh_timeout.is_zero() {
            return Err("node_defaults.ssh_timeout must be greater than 0".into());
        }
        if self.node_defaults.health_check_interval.is_zero() {
            return Err("node_defaults.health_check_interval must be greater than 0".into());
        }

        // Workspace defaults
        if self.workspace_defaults.tmux_prefix.trim().is_empty() {
            return Err("workspace_defaults.tmux_prefix is required".into());
        }
        if !is_valid_agent_type(&self.workspace_defaults.default_agent_type) {
            return Err(
                "workspace_defaults.default_agent_type must be one of opencode, claude-code, codex, gemini, generic"
                    .into(),
            );
        }

        // Agent defaults
        if self.agent_defaults.state_polling_interval < Duration::from_millis(100) {
            return Err("agent_defaults.state_polling_interval must be at least 100ms".into());
        }
        if self.agent_defaults.idle_timeout.is_zero() {
            return Err("agent_defaults.idle_timeout must be greater than 0".into());
        }
        if self.agent_defaults.transcript_buffer_size < 1 {
            return Err("agent_defaults.transcript_buffer_size must be at least 1".into());
        }
        if !is_valid_agent_type(&self.agent_defaults.default_type) {
            return Err(
                "agent_defaults.default_type must be one of opencode, claude-code, codex, gemini, generic"
                    .into(),
            );
        }
        validate_approval_policy(
            "agent_defaults",
            &self.agent_defaults.approval_policy,
            &self.agent_defaults.approval_rules,
        )?;

        // Mail relay
        if self
            .mail
            .relay
            .dial_timeout
            .checked_sub(Duration::ZERO)
            .is_none()
        {
            return Err("mail.relay.dial_timeout must be zero or greater".into());
        }
        if self
            .mail
            .relay
            .reconnect_interval
            .checked_sub(Duration::ZERO)
            .is_none()
        {
            return Err("mail.relay.reconnect_interval must be zero or greater".into());
        }

        // Workspace overrides
        for (i, ov) in self.workspace_overrides.iter().enumerate() {
            let path = format!("workspace_overrides[{i}]");
            if ov.workspace_id.trim().is_empty()
                && ov.name.trim().is_empty()
                && ov.repo_path.trim().is_empty()
            {
                return Err(format!(
                    "{path} must include workspace_id, name, or repo_path"
                ));
            }
            if ov.approval_policy.trim().is_empty() && ov.approval_rules.is_empty() {
                return Err(format!("{path} must set approval_policy or approval_rules"));
            }
            validate_approval_policy(&path, &ov.approval_policy, &ov.approval_rules)?;
        }

        // Scheduler
        if self.scheduler.dispatch_interval < Duration::from_millis(100) {
            return Err("scheduler.dispatch_interval must be at least 100ms".into());
        }
        if self.scheduler.max_retries < 0 {
            return Err("scheduler.max_retries must be zero or greater".into());
        }
        if self.scheduler.retry_backoff.is_zero() {
            return Err("scheduler.retry_backoff must be greater than 0".into());
        }
        if self.scheduler.default_cooldown_duration.is_zero() {
            return Err("scheduler.default_cooldown_duration must be greater than 0".into());
        }

        // TUI
        if self.tui.refresh_interval.is_zero() {
            return Err("tui.refresh_interval must be greater than 0".into());
        }
        match self.tui.theme.to_lowercase().trim() {
            "default"
            | "high-contrast"
            | "low-light"
            | "colorblind-safe"
            | "ocean"
            | "sunset" => {}
            _ => {
                return Err(
                    "tui.theme must be one of default, high-contrast, low-light, colorblind-safe, ocean, sunset"
                        .into(),
                )
            }
        }

        // Event retention
        if self.event_retention.enabled {
            if self.event_retention.max_age.is_zero() && self.event_retention.max_count == 0 {
                return Err(
                    "event_retention: at least one of max_age or max_count must be set when enabled"
                        .into(),
                );
            }
            if self.event_retention.cleanup_interval < Duration::from_secs(60) {
                return Err("event_retention.cleanup_interval must be at least 1 minute".into());
            }
            if self.event_retention.batch_size < 1 {
                return Err("event_retention.batch_size must be at least 1".into());
            }
        }

        // Accounts
        for (i, account) in self.accounts.iter().enumerate() {
            if account.provider.is_empty() {
                return Err(format!("accounts[{i}].provider is required"));
            }
            if account.profile_name.is_empty() {
                return Err(format!("accounts[{i}].profile_name is required"));
            }
            if account.credential_ref.is_empty() {
                return Err(format!("accounts[{i}].credential_ref is required"));
            }
            match account.provider.as_str() {
                "anthropic" | "openai" | "google" | "custom" => {}
                _ => {
                    return Err(format!(
                        "accounts[{i}].provider must be one of anthropic, openai, google, custom"
                    ))
                }
            }
        }

        // Profiles
        let mut profile_names: HashMap<&str, bool> = HashMap::new();
        for (i, profile) in self.profiles.iter().enumerate() {
            if profile.name.trim().is_empty() {
                return Err(format!("profiles[{i}].name is required"));
            }
            if profile_names.contains_key(profile.name.as_str()) {
                return Err(format!("profiles[{i}].name must be unique"));
            }
            profile_names.insert(&profile.name, true);
            if !is_valid_harness(&profile.harness) {
                return Err(format!(
                    "profiles[{i}].harness must be one of pi, opencode, codex, claude"
                ));
            }
            if profile.command_template.is_empty() {
                return Err(format!("profiles[{i}].command_template is required"));
            }
            if profile.max_concurrency < 0 {
                return Err(format!("profiles[{i}].max_concurrency must be >= 0"));
            }
            if !profile.prompt_mode.is_empty() && !is_valid_prompt_mode(&profile.prompt_mode) {
                return Err(format!(
                    "profiles[{i}].prompt_mode must be env, stdin, or path"
                ));
            }
        }

        // Pools
        let mut pool_names: HashMap<&str, bool> = HashMap::new();
        for (i, pool) in self.pools.iter().enumerate() {
            if pool.name.trim().is_empty() {
                return Err(format!("pools[{i}].name is required"));
            }
            if pool_names.contains_key(pool.name.as_str()) {
                return Err(format!("pools[{i}].name must be unique"));
            }
            pool_names.insert(&pool.name, true);
            for profile_name in &pool.profiles {
                if profile_name.is_empty() {
                    return Err(format!("pools[{i}].profiles must not be empty"));
                }
                if !profile_names.contains_key(profile_name.as_str()) {
                    return Err(format!(
                        "pools[{i}].profiles references unknown profile {profile_name:?}"
                    ));
                }
            }
        }

        // Default pool
        if !self.default_pool.is_empty() && !pool_names.contains_key(self.default_pool.as_str()) {
            return Err(format!(
                "default_pool references unknown pool {:?}",
                self.default_pool
            ));
        }

        // Loop defaults
        // Interval of zero is allowed (means no sleep).

        Ok(())
    }

    /// Expands `~` to home directory in all path-related config fields.
    pub fn expand_paths(&mut self) {
        self.global.data_dir = expand_tilde(&self.global.data_dir);
        self.global.config_dir = expand_tilde(&self.global.config_dir);
        self.database.path = expand_tilde(&self.database.path);
        self.logging.file = expand_tilde(&self.logging.file);
        self.node_defaults.ssh_key_path = expand_tilde(&self.node_defaults.ssh_key_path);
        self.event_retention.archive_dir = expand_tilde(&self.event_retention.archive_dir);
        self.loop_defaults.prompt = expand_tilde(&self.loop_defaults.prompt);
        for p in &mut self.profiles {
            p.auth_home = expand_tilde(&p.auth_home);
        }
    }
}

// ---------------------------------------------------------------------------
// Section configs
// ---------------------------------------------------------------------------

/// Global settings.
#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub data_dir: String,
    pub config_dir: String,
    pub auto_register_local_node: bool,
}

/// Database configuration section.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub path: String,
    pub max_connections: i32,
    pub busy_timeout_ms: i32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: String::new(),
            max_connections: 10,
            busy_timeout_ms: 5000,
        }
    }
}

/// Logging configuration section.
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub file: String,
    pub enable_caller: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            format: "console".into(),
            file: String::new(),
            enable_caller: false,
        }
    }
}

/// Account configuration for provider credentials.
#[derive(Debug, Clone)]
pub struct AccountConfig {
    pub provider: String,
    pub profile_name: String,
    pub credential_ref: String,
    pub is_active: bool,
}

/// Profile configuration defining harness+auth profiles.
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    pub name: String,
    pub harness: String,
    pub auth_kind: String,
    pub auth_home: String,
    pub prompt_mode: String,
    pub command_template: String,
    pub model: String,
    pub extra_args: Vec<String>,
    pub env: HashMap<String, String>,
    pub max_concurrency: i32,
}

/// Pool configuration defining profile pools.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub name: String,
    pub strategy: String,
    pub profiles: Vec<String>,
    pub weights: HashMap<String, i32>,
    pub is_default: bool,
}

/// Node default settings.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub ssh_backend: String,
    pub ssh_timeout: Duration,
    pub ssh_key_path: String,
    pub health_check_interval: Duration,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            ssh_backend: "auto".into(),
            ssh_timeout: Duration::from_secs(30),
            ssh_key_path: String::new(),
            health_check_interval: Duration::from_secs(60),
        }
    }
}

/// Workspace default settings.
#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    pub tmux_prefix: String,
    pub default_agent_type: String,
    pub auto_import_existing: bool,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            tmux_prefix: "forge".into(),
            default_agent_type: "opencode".into(),
            auto_import_existing: false,
        }
    }
}

/// Workspace-specific overrides.
#[derive(Debug, Clone)]
pub struct WorkspaceOverrideConfig {
    pub workspace_id: String,
    pub name: String,
    pub repo_path: String,
    pub approval_policy: String,
    pub approval_rules: Vec<ApprovalRule>,
}

/// An approval rule for custom approval policies.
#[derive(Debug, Clone)]
pub struct ApprovalRule {
    pub request_type: String,
    pub action: String,
}

/// Agent default settings.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub default_type: String,
    pub state_polling_interval: Duration,
    pub idle_timeout: Duration,
    pub transcript_buffer_size: i32,
    pub approval_policy: String,
    pub approval_rules: Vec<ApprovalRule>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            default_type: "opencode".into(),
            state_polling_interval: Duration::from_secs(2),
            idle_timeout: Duration::from_secs(10),
            transcript_buffer_size: 10000,
            approval_policy: "strict".into(),
            approval_rules: Vec::new(),
        }
    }
}

/// Scheduler settings.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub dispatch_interval: Duration,
    pub max_retries: i32,
    pub retry_backoff: Duration,
    pub default_cooldown_duration: Duration,
    pub auto_rotate_on_rate_limit: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            dispatch_interval: Duration::from_secs(1),
            max_retries: 3,
            retry_backoff: Duration::from_secs(5),
            default_cooldown_duration: Duration::from_secs(300),
            auto_rotate_on_rate_limit: true,
        }
    }
}

/// Loop default settings.
#[derive(Debug, Clone)]
pub struct LoopDefaultsConfig {
    pub interval: Duration,
    pub prompt: String,
    pub prompt_msg: String,
}

impl Default for LoopDefaultsConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            prompt: String::new(),
            prompt_msg: String::new(),
        }
    }
}

/// TUI settings.
#[derive(Debug, Clone)]
pub struct TuiConfig {
    pub refresh_interval: Duration,
    pub theme: String,
    pub show_timestamps: bool,
    pub compact_mode: bool,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            refresh_interval: Duration::from_millis(500),
            theme: "default".into(),
            show_timestamps: true,
            compact_mode: false,
        }
    }
}

/// Mail subsystem settings.
#[derive(Debug, Clone, Default)]
pub struct MailConfig {
    pub relay: MailRelayConfig,
}

/// Mail relay configuration.
#[derive(Debug, Clone)]
pub struct MailRelayConfig {
    pub enabled: bool,
    pub peers: Vec<String>,
    pub dial_timeout: Duration,
    pub reconnect_interval: Duration,
}

impl Default for MailRelayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            peers: Vec::new(),
            dial_timeout: Duration::from_secs(2),
            reconnect_interval: Duration::from_secs(2),
        }
    }
}

/// Event retention policy settings.
#[derive(Debug, Clone)]
pub struct EventRetentionConfig {
    pub enabled: bool,
    pub max_age: Duration,
    pub max_count: i32,
    pub cleanup_interval: Duration,
    pub archive_before_delete: bool,
    pub archive_dir: String,
    pub batch_size: i32,
}

impl Default for EventRetentionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_age: Duration::from_secs(30 * 24 * 3600), // 30 days
            max_count: 0,
            cleanup_interval: Duration::from_secs(3600), // 1 hour
            archive_before_delete: false,
            archive_dir: String::new(),
            batch_size: 1000,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_valid_agent_type(t: &str) -> bool {
    matches!(
        t,
        "opencode" | "claude-code" | "codex" | "gemini" | "generic"
    )
}

fn is_valid_harness(h: &str) -> bool {
    matches!(h, "pi" | "opencode" | "codex" | "claude")
}

fn is_valid_prompt_mode(m: &str) -> bool {
    matches!(m, "env" | "stdin" | "path")
}

fn validate_approval_policy(
    path: &str,
    policy: &str,
    rules: &[ApprovalRule],
) -> Result<(), String> {
    if policy.is_empty() && rules.is_empty() {
        // Both empty is fine at this level — the caller checks context.
        return Ok(());
    }
    match policy {
        "" | "strict" | "permissive" | "custom" => {}
        other => {
            return Err(format!(
                "{path}.approval_policy must be strict, permissive, or custom (got {other:?})"
            ))
        }
    }
    if policy == "custom" && rules.is_empty() {
        return Err(format!(
            "{path}: custom approval_policy requires at least one approval_rule"
        ));
    }
    for (i, rule) in rules.iter().enumerate() {
        if rule.request_type.is_empty() {
            return Err(format!(
                "{path}.approval_rules[{i}].request_type is required"
            ));
        }
        match rule.action.as_str() {
            "approve" | "deny" | "prompt" => {}
            other => {
                return Err(format!(
                    "{path}.approval_rules[{i}].action must be approve, deny, or prompt (got {other:?})"
                ))
            }
        }
    }
    Ok(())
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(path: &str) -> String {
    if path.is_empty() {
        return path.to_string();
    }
    if path == "~" {
        return home_dir().display().to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home_dir().join(rest).display().to_string();
    }
    path.to_string()
}

/// Search for a configuration file in the standard locations.
/// Returns `None` if no config file is found.
pub fn find_config_file() -> Option<PathBuf> {
    let candidates = config_search_paths();
    for dir in candidates {
        let candidate = dir.join("config.yaml");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Returns the list of directories to search for config files.
fn config_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // XDG_CONFIG_HOME
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        paths.push(Path::new(&xdg).join("forge"));
        paths.push(Path::new(&xdg).join("swarm")); // legacy fallback
    }

    let home = home_dir();
    if home.as_os_str() != "" {
        paths.push(home.join(".config/forge"));
        paths.push(home.join(".config/swarm")); // legacy fallback
    }

    // Current directory
    paths.push(PathBuf::from("."));

    paths
}

/// Get the user's home directory, falling back to `/` on failure.
fn home_dir() -> PathBuf {
    #[allow(deprecated)]
    std::env::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg = Config::default();
        // Backward-compat: existing downstream checks.
        assert_eq!(cfg.logging.level, "info");
        assert_eq!(cfg.logging.format, "console");
        assert_eq!(cfg.database.max_connections, 10);
        assert_eq!(cfg.database.busy_timeout_ms, 5000);
    }

    #[test]
    fn config_default_validates() {
        let cfg = Config::default();
        assert!(cfg.validate().is_ok(), "default config must validate");
    }

    #[test]
    fn validate_rejects_empty_data_dir() {
        let mut cfg = Config::default();
        cfg.global.data_dir = "  ".into();
        let err = match cfg.validate() {
            Ok(()) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("data_dir"), "err={err}");
    }

    #[test]
    fn validate_rejects_bad_log_level() {
        let mut cfg = Config::default();
        cfg.logging.level = "bogus".into();
        let err = match cfg.validate() {
            Ok(()) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("logging.level"), "err={err}");
    }

    #[test]
    fn validate_rejects_bad_log_format() {
        let mut cfg = Config::default();
        cfg.logging.format = "xml".into();
        let err = match cfg.validate() {
            Ok(()) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("logging.format"), "err={err}");
    }

    #[test]
    fn database_path_derived() {
        let cfg = Config::default();
        let db_path = cfg.database_path();
        assert!(db_path.ends_with("forge.db"), "got {db_path}");
        assert!(db_path.contains("forge"), "got {db_path}");
    }

    #[test]
    fn archive_path_derived() {
        let cfg = Config::default();
        let archive = cfg.archive_path();
        assert!(archive.ends_with("archives"), "got {archive}");
    }

    #[test]
    fn expand_tilde_works() {
        assert_eq!(expand_tilde(""), "");
        assert!(!expand_tilde("~").contains('~'));
        let expanded = expand_tilde("~/foo/bar");
        assert!(!expanded.starts_with('~'));
        assert!(expanded.ends_with("foo/bar"));
        assert_eq!(expand_tilde("/absolute/path"), "/absolute/path");
    }

    #[test]
    fn validate_scheduler_bounds() {
        let mut cfg = Config::default();
        cfg.scheduler.dispatch_interval = Duration::from_millis(10);
        let err = match cfg.validate() {
            Ok(()) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("dispatch_interval"), "err={err}");
    }

    #[test]
    fn validate_agent_polling_bounds() {
        let mut cfg = Config::default();
        cfg.agent_defaults.state_polling_interval = Duration::from_millis(10);
        let err = match cfg.validate() {
            Ok(()) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("state_polling_interval"), "err={err}");
    }

    #[test]
    fn validate_event_retention_requires_limit() {
        let mut cfg = Config::default();
        cfg.event_retention.max_age = Duration::ZERO;
        cfg.event_retention.max_count = 0;
        let err = match cfg.validate() {
            Ok(()) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("max_age"), "err={err}");
    }

    #[test]
    fn validate_tui_theme() {
        let mut cfg = Config::default();
        cfg.tui.theme = "neon".into();
        let err = match cfg.validate() {
            Ok(()) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("tui.theme"), "err={err}");
    }

    #[test]
    fn validate_profiles_unique() {
        let mut cfg = Config::default();
        let p = ProfileConfig {
            name: "dup".into(),
            harness: "opencode".into(),
            auth_kind: String::new(),
            auth_home: String::new(),
            prompt_mode: String::new(),
            command_template: "cmd".into(),
            model: String::new(),
            extra_args: Vec::new(),
            env: HashMap::new(),
            max_concurrency: 0,
        };
        cfg.profiles.push(p.clone());
        cfg.profiles.push(p);
        let err = match cfg.validate() {
            Ok(()) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("unique"), "err={err}");
    }

    #[test]
    fn validate_pools_reference_profiles() {
        let mut cfg = Config::default();
        cfg.pools.push(PoolConfig {
            name: "test-pool".into(),
            strategy: String::new(),
            profiles: vec!["nonexistent".into()],
            weights: HashMap::new(),
            is_default: false,
        });
        let err = match cfg.validate() {
            Ok(()) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("unknown profile"), "err={err}");
    }

    #[test]
    fn expand_paths_mutates() {
        let mut cfg = Config::default();
        cfg.global.data_dir = "~/forge-data".into();
        cfg.expand_paths();
        assert!(!cfg.global.data_dir.starts_with('~'));
    }
}
