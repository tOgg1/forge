//! Configuration types for the Forge system.
//!
//! Mirrors Go `internal/config/` — root configuration struct and nested
//! section types. Full deserialization will be added as implementation
//! progresses.

/// Root configuration for the Forge system.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
}

/// Database configuration section.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "forge.db".to_string(),
        }
    }
}

/// Logging configuration section.
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.database.path, "forge.db");
        assert_eq!(cfg.logging.level, "info");
    }
}
