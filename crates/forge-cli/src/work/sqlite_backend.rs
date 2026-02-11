use std::path::PathBuf;

use super::{LoopWorkState, ResolvedLoop, SetCurrentRequest, WorkBackend};

#[derive(Debug, Clone)]
pub struct SqliteWorkBackend {
    db_path: PathBuf,
}

impl SqliteWorkBackend {
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

    fn list_loop_records(&self) -> Result<Vec<crate::queue::LoopRecord>, String> {
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

    fn loop_iteration(&self, loop_id: &str) -> Result<i32, String> {
        if !self.db_path.exists() {
            return Ok(0);
        }

        let db = self.open_db()?;
        let repo = forge_db::loop_run_repository::LoopRunRepository::new(&db);
        let count = match repo.count_by_loop(loop_id) {
            Ok(count) => count,
            Err(err) if err.to_string().contains("no such table: loop_runs") => 0,
            Err(err) => return Err(err.to_string()),
        };

        Ok(i32::try_from(count).unwrap_or(i32::MAX))
    }

    fn map_state(state: forge_db::loop_work_state_repository::LoopWorkState) -> LoopWorkState {
        LoopWorkState {
            id: state.id,
            loop_id: state.loop_id,
            agent_id: state.agent_id,
            task_id: state.task_id,
            status: state.status,
            detail: state.detail,
            loop_iteration: i32::try_from(state.loop_iteration).unwrap_or(i32::MAX),
            is_current: state.is_current,
            created_at: state.created_at,
            updated_at: state.updated_at,
        }
    }
}

impl WorkBackend for SqliteWorkBackend {
    fn resolve_loop(&self, reference: &str) -> Result<ResolvedLoop, String> {
        let loops = self.list_loop_records()?;
        let loop_entry = crate::queue::resolve_loop_ref(&loops, reference)?;
        let iteration = self.loop_iteration(&loop_entry.id)?;
        Ok(ResolvedLoop {
            id: loop_entry.id,
            name: loop_entry.name,
            iteration,
            short_id: loop_entry.short_id,
        })
    }

    fn set_current(&mut self, request: SetCurrentRequest) -> Result<LoopWorkState, String> {
        if !self.db_path.exists() {
            return Err("database not found".to_string());
        }

        let mut db = self.open_db()?;
        let mut repo = forge_db::loop_work_state_repository::LoopWorkStateRepository::new(&mut db);

        let mut state = forge_db::loop_work_state_repository::LoopWorkState {
            id: String::new(),
            loop_id: request.loop_id,
            agent_id: request.agent_id,
            task_id: request.task_id,
            status: request.status,
            detail: request.detail,
            loop_iteration: i64::from(request.loop_iteration),
            is_current: true,
            created_at: String::new(),
            updated_at: String::new(),
        };

        repo.set_current(&mut state)
            .map_err(|err| err.to_string())?;
        Ok(Self::map_state(state))
    }

    fn clear_current(&mut self, loop_id: &str) -> Result<(), String> {
        if !self.db_path.exists() {
            return Ok(());
        }

        let mut db = self.open_db()?;
        let mut repo = forge_db::loop_work_state_repository::LoopWorkStateRepository::new(&mut db);
        repo.clear_current(loop_id).map_err(|err| err.to_string())
    }

    fn current(&self, loop_id: &str) -> Result<Option<LoopWorkState>, String> {
        if !self.db_path.exists() {
            return Ok(None);
        }

        let mut db = self.open_db()?;
        let mut repo = forge_db::loop_work_state_repository::LoopWorkStateRepository::new(&mut db);

        match repo.get_current(loop_id) {
            Ok(state) => Ok(Some(Self::map_state(state))),
            Err(forge_db::DbError::LoopWorkStateNotFound) => Ok(None),
            Err(err) if err.to_string().contains("no such table: loop_work_state") => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    fn list(&self, loop_id: &str, limit: usize) -> Result<Vec<LoopWorkState>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let mut db = self.open_db()?;
        let mut repo = forge_db::loop_work_state_repository::LoopWorkStateRepository::new(&mut db);
        let items = match repo.list_by_loop(loop_id, limit as i64) {
            Ok(items) => items,
            Err(err) if err.to_string().contains("no such table: loop_work_state") => {
                return Ok(Vec::new())
            }
            Err(err) => return Err(err.to_string()),
        };
        Ok(items.into_iter().map(Self::map_state).collect())
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
