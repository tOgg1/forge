//! Loop run repository — CRUD for the `loop_runs` table with full Go parity.

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Loop run status matching the Go `LoopRunStatus` enum.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LoopRunStatus {
    #[default]
    Running,
    Success,
    Error,
    Killed,
}

impl LoopRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Success => "success",
            Self::Error => "error",
            Self::Killed => "killed",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DbError> {
        match s {
            "running" => Ok(Self::Running),
            "success" => Ok(Self::Success),
            "error" => Ok(Self::Error),
            "killed" => Ok(Self::Killed),
            other => Err(DbError::Validation(format!(
                "invalid loop run status: {other}"
            ))),
        }
    }
}

/// A single loop iteration record. Mirrors the Go `models.LoopRun` struct.
#[derive(Debug, Clone, Default)]
pub struct LoopRun {
    pub id: String,
    pub loop_id: String,
    pub profile_id: String,
    pub status: LoopRunStatus,
    pub prompt_source: String,
    pub prompt_path: String,
    pub prompt_override: bool,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub output_tail: String,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

// ---------------------------------------------------------------------------
// Helpers (local to this module)
// ---------------------------------------------------------------------------

fn nullable_string(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn now_rfc3339() -> String {
    let now = std::time::SystemTime::now();
    let duration = match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => std::time::Duration::from_secs(0),
    };
    let secs = duration.as_secs();

    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_civil(days as i64);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_civil(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ---------------------------------------------------------------------------
// LoopRunRepository
// ---------------------------------------------------------------------------

pub struct LoopRunRepository<'a> {
    db: &'a Db,
}

impl<'a> LoopRunRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Create adds a new loop run.
    pub fn create(&self, run: &mut LoopRun) -> Result<(), DbError> {
        if run.id.is_empty() {
            run.id = Uuid::new_v4().to_string();
        }
        if run.started_at.is_empty() {
            run.started_at = now_rfc3339();
        }

        let metadata_json: Option<String> = match &run.metadata {
            Some(m) => Some(serde_json::to_string(m).map_err(|e| {
                DbError::Validation(format!("failed to marshal run metadata: {e}"))
            })?),
            None => None,
        };

        let prompt_override: i32 = if run.prompt_override { 1 } else { 0 };

        self.db.conn().execute(
            "INSERT INTO loop_runs (
                id, loop_id, profile_id, status,
                prompt_source, prompt_path, prompt_override,
                started_at, finished_at, exit_code, output_tail, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                run.id,
                run.loop_id,
                nullable_string(&run.profile_id),
                run.status.as_str(),
                nullable_string(&run.prompt_source),
                nullable_string(&run.prompt_path),
                prompt_override,
                run.started_at,
                run.finished_at,
                run.exit_code,
                nullable_string(&run.output_tail),
                metadata_json,
            ],
        )?;

        Ok(())
    }

    /// Get retrieves a loop run by ID.
    pub fn get(&self, id: &str) -> Result<LoopRun, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, loop_id, profile_id, status,
                    prompt_source, prompt_path, prompt_override,
                    started_at, finished_at, exit_code, output_tail, metadata_json
                FROM loop_runs WHERE id = ?1",
                params![id],
                scan_loop_run,
            )
            .optional()?;

        result.ok_or(DbError::LoopRunNotFound)
    }

    /// ListByLoop retrieves runs for a loop, ordered by started_at DESC.
    pub fn list_by_loop(&self, loop_id: &str) -> Result<Vec<LoopRun>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, loop_id, profile_id, status,
                prompt_source, prompt_path, prompt_override,
                started_at, finished_at, exit_code, output_tail, metadata_json
            FROM loop_runs
            WHERE loop_id = ?1
            ORDER BY started_at DESC",
        )?;

        let rows = stmt.query_map(params![loop_id], scan_loop_run)?;

        let mut runs = Vec::new();
        for row in rows {
            runs.push(row?);
        }
        Ok(runs)
    }

    /// CountRunningByProfile returns the number of running loop runs for a profile.
    pub fn count_running_by_profile(&self, profile_id: &str) -> Result<i64, DbError> {
        let count: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM loop_runs
            WHERE profile_id = ?1 AND status = ?2",
            params![profile_id, LoopRunStatus::Running.as_str()],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// CountByLoop returns the number of runs for a loop.
    pub fn count_by_loop(&self, loop_id: &str) -> Result<i64, DbError> {
        let count: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM loop_runs WHERE loop_id = ?1",
            params![loop_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Finish updates a loop run with completion details.
    pub fn finish(&self, run: &mut LoopRun) -> Result<(), DbError> {
        run.finished_at = Some(now_rfc3339());

        let rows_affected = self.db.conn().execute(
            "UPDATE loop_runs
            SET status = ?1, finished_at = ?2, exit_code = ?3, output_tail = ?4
            WHERE id = ?5",
            params![
                run.status.as_str(),
                run.finished_at,
                run.exit_code,
                nullable_string(&run.output_tail),
                run.id,
            ],
        )?;

        if rows_affected == 0 {
            return Err(DbError::LoopRunNotFound);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Row scanner (mirrors Go scanLoopRun)
// ---------------------------------------------------------------------------

fn scan_loop_run(row: &rusqlite::Row) -> rusqlite::Result<LoopRun> {
    let id: String = row.get(0)?;
    let loop_id: String = row.get(1)?;
    let profile_id: Option<String> = row.get(2)?;
    let status_str: String = row.get(3)?;
    let prompt_source: Option<String> = row.get(4)?;
    let prompt_path: Option<String> = row.get(5)?;
    let prompt_override: i32 = row.get(6)?;
    let started_at: String = row.get(7)?;
    let finished_at: Option<String> = row.get(8)?;
    let exit_code: Option<i32> = row.get(9)?;
    let output_tail: Option<String> = row.get(10)?;
    let metadata_json: Option<String> = row.get(11)?;

    let status = LoopRunStatus::parse(&status_str).unwrap_or_default();

    let metadata: Option<HashMap<String, serde_json::Value>> = match metadata_json {
        Some(ref s) if !s.is_empty() => serde_json::from_str(s).ok(),
        _ => None,
    };

    Ok(LoopRun {
        id,
        loop_id,
        profile_id: profile_id.unwrap_or_default(),
        status,
        prompt_source: prompt_source.unwrap_or_default(),
        prompt_path: prompt_path.unwrap_or_default(),
        prompt_override: prompt_override == 1,
        started_at,
        finished_at: finished_at.filter(|s| !s.is_empty()),
        exit_code,
        output_tail: output_tail.unwrap_or_default(),
        metadata,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loop_repository::{Loop, LoopRepository, LoopState};
    use crate::profile_repository::{Profile, ProfileRepository};
    use crate::Config;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-db-loop-run-repo-{tag}-{nanos}-{}-{suffix}.sqlite",
            std::process::id(),
        ))
    }

    fn open_migrated(tag: &str) -> (Db, PathBuf) {
        let path = temp_db_path(tag);
        let _ = std::fs::remove_file(&path);
        let mut db = match Db::open(Config::new(&path)) {
            Ok(db) => db,
            Err(e) => panic!("open db: {e}"),
        };
        match db.migrate_up() {
            Ok(_) => {}
            Err(e) => panic!("migrate: {e}"),
        }
        (db, path)
    }

    fn create_test_loop(db: &Db) -> Loop {
        let repo = LoopRepository::new(db);
        let mut l = Loop {
            name: format!("test-loop-{}", Uuid::new_v4()),
            repo_path: "/repo".to_string(),
            state: LoopState::Stopped,
            ..Loop::default()
        };
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create loop: {e}"),
        }
        l
    }

    fn create_test_profile(db: &Db) -> Profile {
        let repo = ProfileRepository::new(db);
        let mut p = Profile {
            name: format!("pi-runner-{}", Uuid::new_v4()),
            harness: "pi".to_string(),
            command_template: "pi -p \"{prompt}\"".to_string(),
            max_concurrency: 1,
            prompt_mode: "path".to_string(),
            ..Profile::default()
        };
        match repo.create(&mut p) {
            Ok(()) => {}
            Err(e) => panic!("create profile: {e}"),
        }
        p
    }

    // -- Create + Finish (mirrors Go TestLoopRunRepository_CreateFinish) -----

    #[test]
    fn create_and_finish() {
        let (db, path) = open_migrated("create-finish");
        let test_loop = create_test_loop(&db);
        let profile = create_test_profile(&db);
        let repo = LoopRunRepository::new(&db);

        let mut run = LoopRun {
            loop_id: test_loop.id.clone(),
            profile_id: profile.id.clone(),
            prompt_source: "base".to_string(),
            status: LoopRunStatus::Running,
            ..LoopRun::default()
        };
        match repo.create(&mut run) {
            Ok(()) => {}
            Err(e) => panic!("create run: {e}"),
        }
        assert!(!run.id.is_empty(), "id should be generated");
        assert!(!run.started_at.is_empty(), "started_at should be set");

        // Finish the run
        run.status = LoopRunStatus::Success;
        run.exit_code = Some(0);
        run.output_tail = "ok".to_string();
        match repo.finish(&mut run) {
            Ok(()) => {}
            Err(e) => panic!("finish: {e}"),
        }

        let stored = match repo.get(&run.id) {
            Ok(r) => r,
            Err(e) => panic!("get: {e}"),
        };
        assert_eq!(stored.status, LoopRunStatus::Success);
        assert_eq!(stored.exit_code, Some(0));
        assert_eq!(stored.output_tail, "ok");
        assert!(stored.finished_at.is_some(), "finished_at should be set");

        let _ = std::fs::remove_file(path);
    }

    // -- Get not found -------------------------------------------------------

    #[test]
    fn get_not_found() {
        let (db, path) = open_migrated("get-404");
        let repo = LoopRunRepository::new(&db);
        let err = repo.get("nonexistent");
        assert!(
            matches!(err, Err(DbError::LoopRunNotFound)),
            "expected LoopRunNotFound, got {err:?}"
        );
        let _ = std::fs::remove_file(path);
    }

    // -- Finish not found ----------------------------------------------------

    #[test]
    fn finish_not_found() {
        let (db, path) = open_migrated("finish-404");
        let repo = LoopRunRepository::new(&db);
        let mut run = LoopRun {
            id: "nonexistent".to_string(),
            status: LoopRunStatus::Error,
            ..LoopRun::default()
        };
        let err = repo.finish(&mut run);
        assert!(
            matches!(err, Err(DbError::LoopRunNotFound)),
            "expected LoopRunNotFound, got {err:?}"
        );
        let _ = std::fs::remove_file(path);
    }

    // -- CountByLoop (mirrors Go TestLoopRunRepository_CountByLoop) ----------

    #[test]
    fn count_by_loop() {
        let (db, path) = open_migrated("count-by-loop");
        let repo = LoopRunRepository::new(&db);
        let loop_a = create_test_loop(&db);
        let loop_b = create_test_loop(&db);

        // 3 runs for loop_a
        for _ in 0..3 {
            let mut run = LoopRun {
                loop_id: loop_a.id.clone(),
                prompt_source: "base".to_string(),
                status: LoopRunStatus::Running,
                ..LoopRun::default()
            };
            match repo.create(&mut run) {
                Ok(()) => {}
                Err(e) => panic!("create run: {e}"),
            }
        }

        // 1 run for loop_b
        let mut run = LoopRun {
            loop_id: loop_b.id.clone(),
            prompt_source: "base".to_string(),
            status: LoopRunStatus::Running,
            ..LoopRun::default()
        };
        match repo.create(&mut run) {
            Ok(()) => {}
            Err(e) => panic!("create run: {e}"),
        }

        let count_a = match repo.count_by_loop(&loop_a.id) {
            Ok(c) => c,
            Err(e) => panic!("count: {e}"),
        };
        assert_eq!(count_a, 3);

        let count_b = match repo.count_by_loop(&loop_b.id) {
            Ok(c) => c,
            Err(e) => panic!("count: {e}"),
        };
        assert_eq!(count_b, 1);

        let _ = std::fs::remove_file(path);
    }

    // -- CountRunningByProfile -----------------------------------------------

    #[test]
    fn count_running_by_profile() {
        let (db, path) = open_migrated("count-running");
        let repo = LoopRunRepository::new(&db);
        let test_loop = create_test_loop(&db);
        let profile = create_test_profile(&db);

        // 2 running runs
        for _ in 0..2 {
            let mut run = LoopRun {
                loop_id: test_loop.id.clone(),
                profile_id: profile.id.clone(),
                status: LoopRunStatus::Running,
                ..LoopRun::default()
            };
            match repo.create(&mut run) {
                Ok(()) => {}
                Err(e) => panic!("create run: {e}"),
            }
        }

        // 1 finished run
        let mut finished = LoopRun {
            loop_id: test_loop.id.clone(),
            profile_id: profile.id.clone(),
            status: LoopRunStatus::Success,
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: Some("2026-01-01T00:01:00Z".to_string()),
            ..LoopRun::default()
        };
        match repo.create(&mut finished) {
            Ok(()) => {}
            Err(e) => panic!("create finished run: {e}"),
        }

        let count = match repo.count_running_by_profile(&profile.id) {
            Ok(c) => c,
            Err(e) => panic!("count: {e}"),
        };
        assert_eq!(count, 2, "only running runs should be counted");

        let _ = std::fs::remove_file(path);
    }

    // -- ListByLoop ----------------------------------------------------------

    #[test]
    fn list_by_loop_returns_desc_order() {
        let (db, path) = open_migrated("list-by-loop");
        let repo = LoopRunRepository::new(&db);
        let test_loop = create_test_loop(&db);

        // Create runs with distinct started_at timestamps
        for i in 0..3 {
            let mut run = LoopRun {
                loop_id: test_loop.id.clone(),
                prompt_source: format!("source-{i}"),
                status: LoopRunStatus::Running,
                started_at: format!("2026-01-0{}T00:00:00Z", i + 1),
                ..LoopRun::default()
            };
            match repo.create(&mut run) {
                Ok(()) => {}
                Err(e) => panic!("create run: {e}"),
            }
        }

        let runs = match repo.list_by_loop(&test_loop.id) {
            Ok(r) => r,
            Err(e) => panic!("list: {e}"),
        };
        assert_eq!(runs.len(), 3);
        // DESC order: most recent first
        assert_eq!(runs[0].prompt_source, "source-2");
        assert_eq!(runs[1].prompt_source, "source-1");
        assert_eq!(runs[2].prompt_source, "source-0");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn list_by_loop_empty() {
        let (db, path) = open_migrated("list-empty");
        let repo = LoopRunRepository::new(&db);
        let test_loop = create_test_loop(&db);

        let runs = match repo.list_by_loop(&test_loop.id) {
            Ok(r) => r,
            Err(e) => panic!("list: {e}"),
        };
        assert!(runs.is_empty());

        let _ = std::fs::remove_file(path);
    }

    // -- Metadata roundtrip --------------------------------------------------

    #[test]
    fn metadata_roundtrip() {
        let (db, path) = open_migrated("metadata");
        let repo = LoopRunRepository::new(&db);
        let test_loop = create_test_loop(&db);

        let mut meta = HashMap::new();
        meta.insert("pid".to_string(), serde_json::json!(42));
        meta.insert("host".to_string(), serde_json::json!("worker-1"));

        let mut run = LoopRun {
            loop_id: test_loop.id.clone(),
            status: LoopRunStatus::Running,
            metadata: Some(meta),
            ..LoopRun::default()
        };
        match repo.create(&mut run) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let stored = match repo.get(&run.id) {
            Ok(r) => r,
            Err(e) => panic!("get: {e}"),
        };
        let m = match stored.metadata.as_ref() {
            Some(m) => m,
            None => panic!("metadata should be Some"),
        };
        assert_eq!(m.get("pid"), Some(&serde_json::json!(42)));
        assert_eq!(m.get("host"), Some(&serde_json::json!("worker-1")));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn null_metadata_roundtrip() {
        let (db, path) = open_migrated("null-metadata");
        let repo = LoopRunRepository::new(&db);
        let test_loop = create_test_loop(&db);

        let mut run = LoopRun {
            loop_id: test_loop.id.clone(),
            status: LoopRunStatus::Running,
            ..LoopRun::default()
        };
        match repo.create(&mut run) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let stored = match repo.get(&run.id) {
            Ok(r) => r,
            Err(e) => panic!("get: {e}"),
        };
        assert!(stored.metadata.is_none());

        let _ = std::fs::remove_file(path);
    }

    // -- Prompt override roundtrip -------------------------------------------

    #[test]
    fn prompt_override_roundtrip() {
        let (db, path) = open_migrated("prompt-override");
        let repo = LoopRunRepository::new(&db);
        let test_loop = create_test_loop(&db);

        let mut run = LoopRun {
            loop_id: test_loop.id.clone(),
            status: LoopRunStatus::Running,
            prompt_override: true,
            prompt_source: "override".to_string(),
            prompt_path: "/tmp/prompt.md".to_string(),
            ..LoopRun::default()
        };
        match repo.create(&mut run) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let stored = match repo.get(&run.id) {
            Ok(r) => r,
            Err(e) => panic!("get: {e}"),
        };
        assert!(stored.prompt_override);
        assert_eq!(stored.prompt_source, "override");
        assert_eq!(stored.prompt_path, "/tmp/prompt.md");

        let _ = std::fs::remove_file(path);
    }

    // -- Status parse --------------------------------------------------------

    #[test]
    fn status_parse_all_valid() {
        assert_eq!(
            LoopRunStatus::parse("running").ok(),
            Some(LoopRunStatus::Running)
        );
        assert_eq!(
            LoopRunStatus::parse("success").ok(),
            Some(LoopRunStatus::Success)
        );
        assert_eq!(
            LoopRunStatus::parse("error").ok(),
            Some(LoopRunStatus::Error)
        );
        assert_eq!(
            LoopRunStatus::parse("killed").ok(),
            Some(LoopRunStatus::Killed)
        );
    }

    #[test]
    fn status_parse_invalid() {
        let err = LoopRunStatus::parse("bogus");
        assert!(matches!(err, Err(DbError::Validation(_))));
    }

    // -- All status variants roundtrip through DB ----------------------------

    #[test]
    fn status_roundtrip_all_variants() {
        let (db, path) = open_migrated("status-variants");
        let repo = LoopRunRepository::new(&db);
        let test_loop = create_test_loop(&db);

        for status in [
            LoopRunStatus::Running,
            LoopRunStatus::Success,
            LoopRunStatus::Error,
            LoopRunStatus::Killed,
        ] {
            let mut run = LoopRun {
                loop_id: test_loop.id.clone(),
                status: status.clone(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                ..LoopRun::default()
            };
            match repo.create(&mut run) {
                Ok(()) => {}
                Err(e) => panic!("create: {e}"),
            }

            let stored = match repo.get(&run.id) {
                Ok(r) => r,
                Err(e) => panic!("get: {e}"),
            };
            assert_eq!(stored.status, status);
        }

        let _ = std::fs::remove_file(path);
    }

    // -- Optional fields stored as null --------------------------------------

    #[test]
    fn optional_fields_stored_as_null() {
        let (db, path) = open_migrated("nullable");
        let repo = LoopRunRepository::new(&db);
        let test_loop = create_test_loop(&db);

        let mut run = LoopRun {
            loop_id: test_loop.id.clone(),
            status: LoopRunStatus::Running,
            ..LoopRun::default()
        };
        match repo.create(&mut run) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let stored = match repo.get(&run.id) {
            Ok(r) => r,
            Err(e) => panic!("get: {e}"),
        };
        assert!(stored.profile_id.is_empty());
        assert!(stored.prompt_source.is_empty());
        assert!(stored.prompt_path.is_empty());
        assert!(!stored.prompt_override);
        assert!(stored.finished_at.is_none());
        assert!(stored.exit_code.is_none());
        assert!(stored.output_tail.is_empty());
        assert!(stored.metadata.is_none());

        let _ = std::fs::remove_file(path);
    }

    // -- Create with given ID ------------------------------------------------

    #[test]
    fn create_preserves_given_id() {
        let (db, path) = open_migrated("given-id");
        let repo = LoopRunRepository::new(&db);
        let test_loop = create_test_loop(&db);

        let mut run = LoopRun {
            id: "my-custom-run-id".to_string(),
            loop_id: test_loop.id.clone(),
            status: LoopRunStatus::Running,
            started_at: "2026-01-01T00:00:00Z".to_string(),
            ..LoopRun::default()
        };
        match repo.create(&mut run) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }
        assert_eq!(run.id, "my-custom-run-id");

        let stored = match repo.get("my-custom-run-id") {
            Ok(r) => r,
            Err(e) => panic!("get: {e}"),
        };
        assert_eq!(stored.id, "my-custom-run-id");

        let _ = std::fs::remove_file(path);
    }
}
