use std::path::PathBuf;

const DB_PATH_ENV_KEYS: [&str; 2] = ["FORGE_DATABASE_PATH", "FORGE_DB_PATH"];
const DATA_DIR_ENV_KEYS: [&str; 3] = [
    "FORGE_DATA_DIR",
    "FORGE_GLOBAL_DATA_DIR",
    "SWARM_GLOBAL_DATA_DIR",
];

pub fn resolve_database_path() -> PathBuf {
    for key in DB_PATH_ENV_KEYS {
        if let Some(path) = non_empty_env_path(key) {
            return path;
        }
    }
    resolve_data_dir().join("forge.db")
}

pub fn resolve_data_dir() -> PathBuf {
    for key in DATA_DIR_ENV_KEYS {
        if let Some(path) = non_empty_env_path(key) {
            return path;
        }
    }
    if let Some(home) = non_empty_env_path("HOME") {
        return home.join(".local").join("share").join("forge");
    }
    PathBuf::from(".forge-data")
}

fn non_empty_env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).and_then(|raw| {
        if raw.is_empty() {
            None
        } else {
            Some(PathBuf::from(raw))
        }
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::{resolve_data_dir, resolve_database_path};

    #[test]
    fn database_path_prefers_explicit_database_env() {
        let _lock = env_lock();
        let _g_data = EnvGuard::set("FORGE_GLOBAL_DATA_DIR", "/tmp/forge-global-data");
        let _g_db = EnvGuard::set("FORGE_DATABASE_PATH", "/tmp/forge-explicit.db");

        assert_eq!(
            resolve_database_path(),
            PathBuf::from("/tmp/forge-explicit.db")
        );
    }

    #[test]
    fn database_path_uses_global_data_dir_alias_when_db_env_is_unset() {
        let _lock = env_lock();
        let _g_db = EnvGuard::unset("FORGE_DATABASE_PATH");
        let _g_legacy_db = EnvGuard::unset("FORGE_DB_PATH");
        let _g_data = EnvGuard::set("FORGE_GLOBAL_DATA_DIR", "/tmp/forge-global-data");

        assert_eq!(
            resolve_database_path(),
            PathBuf::from("/tmp/forge-global-data/forge.db")
        );
    }

    #[test]
    fn data_dir_uses_swarm_global_alias_when_primary_data_dir_is_unset() {
        let _lock = env_lock();
        let _g_data = EnvGuard::unset("FORGE_DATA_DIR");
        let _g_global = EnvGuard::unset("FORGE_GLOBAL_DATA_DIR");
        let _g_swarm = EnvGuard::set("SWARM_GLOBAL_DATA_DIR", "/tmp/swarm-global-data");

        assert_eq!(resolve_data_dir(), PathBuf::from("/tmp/swarm-global-data"));
    }

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = LOCK.get_or_init(|| Mutex::new(()));
        match lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    struct EnvGuard {
        key: String,
        previous: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self {
                key: key.to_string(),
                previous,
            }
        }

        fn unset(key: &str) -> Self {
            let previous = std::env::var_os(key);
            std::env::remove_var(key);
            Self {
                key: key.to_string(),
                previous,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous.take() {
                std::env::set_var(&self.key, value);
            } else {
                std::env::remove_var(&self.key);
            }
        }
    }
}
