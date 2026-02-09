use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Config {
    pub global: GlobalConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub path: Option<PathBuf>,
    pub max_connections: i64,
    pub busy_timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

impl Config {
    pub fn default_from_env() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        let data_dir = if home.is_empty() {
            PathBuf::from(".")
        } else {
            PathBuf::from(&home)
                .join(".local")
                .join("share")
                .join("forge")
        };
        let config_dir = if home.is_empty() {
            PathBuf::from(".")
        } else {
            PathBuf::from(&home).join(".config").join("forge")
        };
        Self {
            global: GlobalConfig {
                data_dir,
                config_dir,
            },
            database: DatabaseConfig {
                path: None,
                max_connections: 10,
                busy_timeout_ms: 5000,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "console".to_string(),
            },
        }
    }

    pub fn database_path(&self) -> PathBuf {
        if let Some(path) = &self.database.path {
            return path.clone();
        }
        self.global.data_dir.join("forge.db")
    }

    pub fn ensure_directories(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.global.data_dir)?;
        std::fs::create_dir_all(&self.global.config_dir)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct PartialConfig {
    #[serde(default)]
    global: PartialGlobalConfig,
    #[serde(default)]
    database: PartialDatabaseConfig,
    #[serde(default)]
    logging: PartialLoggingConfig,
}

#[derive(Debug, Default, Deserialize)]
struct PartialGlobalConfig {
    #[serde(default)]
    data_dir: String,
    #[serde(default)]
    config_dir: String,
}

#[derive(Debug, Default, Deserialize)]
struct PartialDatabaseConfig {
    #[serde(default)]
    path: String,
    #[serde(default)]
    max_connections: i64,
    #[serde(default)]
    busy_timeout_ms: i64,
}

#[derive(Debug, Default, Deserialize)]
struct PartialLoggingConfig {
    #[serde(default)]
    level: String,
    #[serde(default)]
    format: String,
}

/// Load config with Go-like precedence:
/// defaults < (optional) config file (explicit => hard error if unreadable).
pub fn load_config(config_file: Option<&str>) -> Result<(Config, Option<PathBuf>), String> {
    let mut cfg = Config::default_from_env();

    let explicit = config_file
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    let (path_to_try, used) = if let Some(path) = explicit {
        (Some(path), true)
    } else {
        (default_config_path(), false)
    };

    if let Some(path) = path_to_try {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let parsed: PartialConfig =
                    serde_yaml::from_str(&text).map_err(|err| format!("parse config: {err}"))?;
                apply_partial(&mut cfg, parsed)?;
                return Ok((cfg, Some(path)));
            }
            Err(err) => {
                if used {
                    return Err(format!("failed to load config file: {err}"));
                }
            }
        }
    }

    Ok((cfg, None))
}

fn default_config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return Some(PathBuf::from(xdg).join("forge").join("config.yaml"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("forge")
                    .join("config.yaml"),
            );
        }
    }
    None
}

fn apply_partial(cfg: &mut Config, partial: PartialConfig) -> Result<(), String> {
    if !partial.global.data_dir.trim().is_empty() {
        cfg.global.data_dir = expand_tilde(partial.global.data_dir.trim())?;
    }
    if !partial.global.config_dir.trim().is_empty() {
        cfg.global.config_dir = expand_tilde(partial.global.config_dir.trim())?;
    }
    if !partial.database.path.trim().is_empty() {
        cfg.database.path = Some(expand_tilde(partial.database.path.trim())?);
    }
    if partial.database.max_connections > 0 {
        cfg.database.max_connections = partial.database.max_connections;
    }
    if partial.database.busy_timeout_ms > 0 {
        cfg.database.busy_timeout_ms = partial.database.busy_timeout_ms as u64;
    }
    if !partial.logging.level.trim().is_empty() {
        cfg.logging.level = partial.logging.level.trim().to_string();
    }
    if !partial.logging.format.trim().is_empty() {
        cfg.logging.format = partial.logging.format.trim().to_string();
    }
    Ok(())
}

fn expand_tilde(input: &str) -> Result<PathBuf, String> {
    if input == "~" {
        let home = std::env::var("HOME").map_err(|_| "failed to resolve HOME".to_string())?;
        return Ok(PathBuf::from(home));
    }
    if let Some(rest) = input.strip_prefix("~/") {
        let home = std::env::var("HOME").map_err(|_| "failed to resolve HOME".to_string())?;
        return Ok(PathBuf::from(home).join(rest));
    }
    Ok(Path::new(input).to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::{load_config, Config};

    #[test]
    fn default_config_contains_expected_paths() {
        let cfg = Config::default_from_env();
        assert!(!cfg.global.data_dir.as_os_str().is_empty());
        assert!(!cfg.global.config_dir.as_os_str().is_empty());
        assert_eq!(cfg.database.busy_timeout_ms, 5000);
    }

    #[test]
    fn missing_config_file_is_ok_when_not_explicit() {
        let (cfg, used) = match load_config(None) {
            Ok(value) => value,
            Err(err) => panic!("load: {err}"),
        };
        let _ = used;
        let _ = cfg.database_path();
    }
}
