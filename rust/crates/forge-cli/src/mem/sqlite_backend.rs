use std::path::PathBuf;

use super::{LoopEntry, LoopKVEntry, MemBackend};

#[derive(Debug, Clone)]
pub struct SqliteMemBackend {
    db_path: PathBuf,
}

impl SqliteMemBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
        }
    }

    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))
    }

    fn list_loops(&self) -> Result<Vec<crate::queue::LoopRecord>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = self.open_db()?;
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let loops = match loop_repo.list() {
            Ok(loops) => loops,
            Err(err) if err.to_string().contains("no such table: loops") => return Ok(Vec::new()),
            Err(err) => return Err(err.to_string()),
        };

        Ok(loops
            .into_iter()
            .map(|entry| crate::queue::LoopRecord {
                id: entry.id.clone(),
                short_id: if entry.short_id.is_empty() {
                    entry.id
                } else {
                    entry.short_id
                },
                name: entry.name,
            })
            .collect())
    }

    fn map_entry(entry: forge_db::LoopKV) -> LoopKVEntry {
        LoopKVEntry {
            created_at: entry.created_at,
            id: entry.id,
            key: entry.key,
            loop_id: entry.loop_id,
            updated_at: entry.updated_at,
            value: entry.value,
        }
    }
}

impl MemBackend for SqliteMemBackend {
    fn resolve_loop_by_ref(&self, loop_ref: &str) -> Result<LoopEntry, String> {
        let loops = self.list_loops()?;
        let loop_entry = crate::queue::resolve_loop_ref(&loops, loop_ref)?;
        Ok(LoopEntry {
            id: loop_entry.id,
            name: loop_entry.name,
        })
    }

    fn set(&mut self, loop_id: &str, key: &str, value: &str) -> Result<(), String> {
        let db = self.open_db()?;
        let repo = forge_db::LoopKVRepository::new(&db);
        repo.set(loop_id, key, value).map_err(map_repo_error)
    }

    fn get(&self, loop_id: &str, key: &str) -> Result<LoopKVEntry, String> {
        let db = self.open_db()?;
        let repo = forge_db::LoopKVRepository::new(&db);
        let entry = repo.get(loop_id, key).map_err(map_repo_error)?;
        Ok(Self::map_entry(entry))
    }

    fn list_by_loop(&self, loop_id: &str) -> Result<Vec<LoopKVEntry>, String> {
        let db = self.open_db()?;
        let repo = forge_db::LoopKVRepository::new(&db);
        repo.list_by_loop(loop_id)
            .map(|items| items.into_iter().map(Self::map_entry).collect())
            .map_err(map_repo_error)
    }

    fn delete(&mut self, loop_id: &str, key: &str) -> Result<(), String> {
        let db = self.open_db()?;
        let repo = forge_db::LoopKVRepository::new(&db);
        repo.delete(loop_id, key).map_err(map_repo_error)
    }
}

fn map_repo_error(err: forge_db::DbError) -> String {
    match err {
        forge_db::DbError::LoopKVNotFound(_) => "loop kv not found".to_string(),
        other => other.to_string(),
    }
}

fn resolve_database_path() -> PathBuf {
    if let Some(path) = std::env::var_os("FORGE_DATABASE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("FORGE_DB_PATH") {
        return PathBuf::from(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".local");
        path.push("share");
        path.push("forge");
        path.push("forge.db");
        return path;
    }
    PathBuf::from("forge.db")
}
