use std::path::PathBuf;

use super::{resolve_loop_ref, LoopRecord, QueueBackend, QueueItem};

#[derive(Debug, Clone)]
pub struct SqliteQueueBackend {
    db_path: PathBuf,
}

impl SqliteQueueBackend {
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

    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = self.open_db()?;
        let repo = forge_db::loop_repository::LoopRepository::new(&db);

        let loops = match repo.list() {
            Ok(loops) => loops,
            Err(err) if err.to_string().contains("no such table: loops") => return Ok(Vec::new()),
            Err(err) => return Err(err.to_string()),
        };

        Ok(loops
            .into_iter()
            .map(|entry| LoopRecord {
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
}

impl QueueBackend for SqliteQueueBackend {
    fn resolve_loop(&self, loop_ref: &str) -> Result<LoopRecord, String> {
        let loops = self.list_loops()?;
        resolve_loop_ref(&loops, loop_ref)
    }

    fn list_queue(&self, loop_id: &str) -> Result<Vec<QueueItem>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = self.open_db()?;
        let repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let items = match repo.list(loop_id) {
            Ok(items) => items,
            Err(err) if err.to_string().contains("no such table: loop_queue_items") => {
                return Ok(Vec::new())
            }
            Err(err) => return Err(err.to_string()),
        };

        Ok(items
            .into_iter()
            .map(|item| QueueItem {
                id: item.id,
                item_type: item.item_type,
                status: item.status,
                position: item.position,
                created_at: item.created_at,
            })
            .collect())
    }

    fn clear_pending(&mut self, loop_id: &str) -> Result<usize, String> {
        if !self.db_path.exists() {
            return Ok(0);
        }

        let db = self.open_db()?;
        let repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        repo.clear(loop_id).map_err(|err| err.to_string())
    }

    fn remove_item(&mut self, loop_id: &str, item_id: &str) -> Result<(), String> {
        // Match Go behavior: verify item belongs to loop first.
        let items = self.list_queue(loop_id)?;
        if !items.iter().any(|item| item.id == item_id) {
            return Err("queue item not found in loop".to_string());
        }

        let db = self.open_db()?;
        let repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        repo.remove(item_id).map_err(|err| err.to_string())
    }

    fn move_item(&mut self, loop_id: &str, item_id: &str, to: &str) -> Result<(), String> {
        let items = self.list_queue(loop_id)?;
        let mut pending: Vec<String> = items
            .iter()
            .filter(|item| item.status == "pending")
            .map(|item| item.id.clone())
            .collect();
        if pending.is_empty() {
            return Err("no pending items".to_string());
        }

        let Some(index) = pending.iter().position(|id| id == item_id) else {
            return Err("queue item not found".to_string());
        };

        let moving = pending.remove(index);
        match to.to_ascii_lowercase().as_str() {
            "front" => pending.insert(0, moving),
            "back" => pending.push(moving),
            other => return Err(format!("unknown move target \"{other}\"")),
        }

        let db = self.open_db()?;
        let repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        repo.reorder(loop_id, &pending)
            .map_err(|err| err.to_string())
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
