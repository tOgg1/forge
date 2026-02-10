use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tabwriter::TabWriter;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReservation {
    pub id: i64,
    pub agent: String,
    pub path_pattern: String,
    pub exclusive: bool,
    pub reason: String,
    pub created_ts: String,
    pub expires_ts: String,
    pub released_ts: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileReservationGrant {
    pub id: i64,
    pub path_pattern: String,
    pub exclusive: bool,
    pub reason: String,
    pub expires_ts: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileReservationConflict {
    pub path: String,
    pub holders: Vec<FileReservationHolder>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileReservationHolder {
    pub id: i64,
    pub agent: String,
    pub path_pattern: String,
    pub exclusive: bool,
    pub expires_ts: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockClaimResponse {
    pub granted: Vec<FileReservationGrant>,
    pub conflicts: Vec<FileReservationConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockReleaseResponse {
    pub released: i64,
    pub released_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockCheckResult {
    pub path: String,
    pub claims: Vec<FileReservation>,
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

pub trait LockBackend {
    fn resolve_project(&self) -> Result<String, String>;
    fn resolve_agent(&self, flag_value: &str) -> Result<String, String>;
    fn claim_locks(
        &self,
        project: &str,
        agent: &str,
        paths: &[String],
        ttl_seconds: i64,
        exclusive: bool,
        reason: &str,
    ) -> Result<LockClaimResponse, String>;
    fn force_release(
        &self,
        project: &str,
        agent: &str,
        lock_ids: &[i64],
        reason: &str,
    ) -> Result<(), String>;
    fn release_locks(
        &self,
        project: &str,
        agent: &str,
        paths: &[String],
        lock_ids: &[i64],
    ) -> Result<LockReleaseResponse, String>;
    fn list_reservations(
        &self,
        project: &str,
        active_only: bool,
    ) -> Result<Vec<FileReservation>, String>;
}

// ---------------------------------------------------------------------------
// In-memory backend for testing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct InMemoryLockBackend {
    pub project: String,
    pub agent: String,
    pub reservations: Vec<FileReservation>,
    pub claim_response: Option<LockClaimResponse>,
    pub release_response: Option<LockReleaseResponse>,
}

impl InMemoryLockBackend {
    pub fn with_project_and_agent(project: &str, agent: &str) -> Self {
        Self {
            project: project.to_string(),
            agent: agent.to_string(),
            ..Default::default()
        }
    }

    pub fn set_claim_response(&mut self, response: LockClaimResponse) {
        self.claim_response = Some(response);
    }

    pub fn set_release_response(&mut self, response: LockReleaseResponse) {
        self.release_response = Some(response);
    }
}

impl LockBackend for InMemoryLockBackend {
    fn resolve_project(&self) -> Result<String, String> {
        if self.project.is_empty() {
            return Err(
                "agent mail project not configured (set FORGE_AGENT_MAIL_PROJECT or run inside a git repo)"
                    .to_string(),
            );
        }
        Ok(self.project.clone())
    }

    fn resolve_agent(&self, flag_value: &str) -> Result<String, String> {
        let trimmed = flag_value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        if !self.agent.is_empty() {
            return Ok(self.agent.clone());
        }
        Err("agent name is required (use --agent or set FORGE_AGENT_MAIL_AGENT)".to_string())
    }

    fn claim_locks(
        &self,
        _project: &str,
        _agent: &str,
        _paths: &[String],
        _ttl_seconds: i64,
        _exclusive: bool,
        _reason: &str,
    ) -> Result<LockClaimResponse, String> {
        match &self.claim_response {
            Some(response) => Ok(response.clone()),
            None => Ok(LockClaimResponse {
                granted: Vec::new(),
                conflicts: Vec::new(),
            }),
        }
    }

    fn force_release(
        &self,
        _project: &str,
        _agent: &str,
        _lock_ids: &[i64],
        _reason: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    fn release_locks(
        &self,
        _project: &str,
        _agent: &str,
        _paths: &[String],
        _lock_ids: &[i64],
    ) -> Result<LockReleaseResponse, String> {
        match &self.release_response {
            Some(response) => Ok(response.clone()),
            None => Ok(LockReleaseResponse {
                released: 0,
                released_at: String::new(),
            }),
        }
    }

    fn list_reservations(
        &self,
        _project: &str,
        _active_only: bool,
    ) -> Result<Vec<FileReservation>, String> {
        Ok(self.reservations.clone())
    }
}

// ---------------------------------------------------------------------------
// Filesystem backend (production)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FilesystemLockBackend {
    store_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LockStore {
    next_id: i64,
    reservations: Vec<StoredReservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredReservation {
    project: String,
    reservation: FileReservation,
}

impl Default for FilesystemLockBackend {
    fn default() -> Self {
        Self {
            store_path: resolve_default_lock_store_path(),
        }
    }
}

impl FilesystemLockBackend {
    pub fn new(store_path: PathBuf) -> Self {
        Self { store_path }
    }

    fn now_ts() -> String {
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    fn load_store(&self) -> Result<LockStore, String> {
        let raw = match std::fs::read_to_string(&self.store_path) {
            Ok(value) => value,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LockStore::default())
            }
            Err(err) => {
                return Err(format!(
                    "failed to read lock store {}: {err}",
                    self.store_path.display()
                ))
            }
        };
        serde_json::from_str(&raw).map_err(|err| {
            format!(
                "failed to parse lock store {}: {err}",
                self.store_path.display()
            )
        })
    }

    fn save_store(&self, store: &LockStore) -> Result<(), String> {
        if let Some(parent) = self.store_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "failed to create lock store directory {}: {err}",
                    parent.display()
                )
            })?;
        }
        let body = serde_json::to_string_pretty(store)
            .map_err(|err| format!("failed to serialize lock store: {err}"))?;
        std::fs::write(&self.store_path, body).map_err(|err| {
            format!(
                "failed to write lock store {}: {err}",
                self.store_path.display()
            )
        })
    }

    fn cleanup_expired(store: &mut LockStore) {
        let now = chrono::Utc::now();
        for entry in &mut store.reservations {
            if !entry.reservation.released_ts.trim().is_empty() {
                continue;
            }
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&entry.reservation.expires_ts)
            {
                if parsed.with_timezone(&chrono::Utc) <= now {
                    entry.reservation.released_ts =
                        now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                }
            }
        }
    }
}

impl LockBackend for FilesystemLockBackend {
    fn resolve_project(&self) -> Result<String, String> {
        if let Ok(value) = std::env::var("FORGE_AGENT_MAIL_PROJECT") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        std::env::current_dir()
            .map(|path| path.display().to_string())
            .map_err(|_| {
                "agent mail project not configured (set FORGE_AGENT_MAIL_PROJECT or run inside a git repo)"
                    .to_string()
            })
    }

    fn resolve_agent(&self, flag_value: &str) -> Result<String, String> {
        let trimmed = flag_value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        if let Ok(value) = std::env::var("FORGE_AGENT_MAIL_AGENT") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        Err("agent name is required (use --agent or set FORGE_AGENT_MAIL_AGENT)".to_string())
    }

    fn claim_locks(
        &self,
        project: &str,
        agent: &str,
        paths: &[String],
        ttl_seconds: i64,
        exclusive: bool,
        reason: &str,
    ) -> Result<LockClaimResponse, String> {
        let mut store = self.load_store()?;
        Self::cleanup_expired(&mut store);

        let mut conflicts = Vec::new();
        for requested_path in paths {
            let holders: Vec<FileReservationHolder> = store
                .reservations
                .iter()
                .filter(|entry| entry.project == project)
                .filter(|entry| entry.reservation.released_ts.trim().is_empty())
                .filter(|entry| {
                    matches_path_pattern(requested_path, &entry.reservation.path_pattern)
                        && (exclusive || entry.reservation.exclusive)
                })
                .map(|entry| FileReservationHolder {
                    id: entry.reservation.id,
                    agent: entry.reservation.agent.clone(),
                    path_pattern: entry.reservation.path_pattern.clone(),
                    exclusive: entry.reservation.exclusive,
                    expires_ts: entry.reservation.expires_ts.clone(),
                })
                .collect();
            if !holders.is_empty() {
                conflicts.push(FileReservationConflict {
                    path: requested_path.clone(),
                    holders,
                });
            }
        }

        if !conflicts.is_empty() {
            self.save_store(&store)?;
            return Ok(LockClaimResponse {
                granted: Vec::new(),
                conflicts,
            });
        }

        let now = chrono::Utc::now();
        let expires = now + chrono::Duration::seconds(ttl_seconds);
        let created_ts = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let expires_ts = expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        let mut granted = Vec::with_capacity(paths.len());
        for requested_path in paths {
            store.next_id += 1;
            let id = store.next_id;
            let reservation = FileReservation {
                id,
                agent: agent.to_string(),
                path_pattern: requested_path.clone(),
                exclusive,
                reason: reason.to_string(),
                created_ts: created_ts.clone(),
                expires_ts: expires_ts.clone(),
                released_ts: String::new(),
            };
            store.reservations.push(StoredReservation {
                project: project.to_string(),
                reservation: reservation.clone(),
            });
            granted.push(FileReservationGrant {
                id,
                path_pattern: reservation.path_pattern,
                exclusive,
                reason: reservation.reason,
                expires_ts: reservation.expires_ts,
            });
        }

        self.save_store(&store)?;
        Ok(LockClaimResponse {
            granted,
            conflicts: Vec::new(),
        })
    }

    fn force_release(
        &self,
        project: &str,
        _agent: &str,
        lock_ids: &[i64],
        reason: &str,
    ) -> Result<(), String> {
        let mut store = self.load_store()?;
        let now = Self::now_ts();
        for entry in &mut store.reservations {
            if entry.project != project {
                continue;
            }
            if lock_ids.contains(&entry.reservation.id)
                && entry.reservation.released_ts.trim().is_empty()
            {
                entry.reservation.released_ts = now.clone();
                if !reason.trim().is_empty() {
                    entry.reservation.reason = reason.to_string();
                }
            }
        }
        self.save_store(&store)
    }

    fn release_locks(
        &self,
        project: &str,
        agent: &str,
        paths: &[String],
        lock_ids: &[i64],
    ) -> Result<LockReleaseResponse, String> {
        let mut store = self.load_store()?;
        let released_at = Self::now_ts();
        let mut released = 0i64;

        for entry in &mut store.reservations {
            if entry.project != project {
                continue;
            }
            if entry.reservation.agent != agent {
                continue;
            }
            if !entry.reservation.released_ts.trim().is_empty() {
                continue;
            }

            let by_id = !lock_ids.is_empty() && lock_ids.contains(&entry.reservation.id);
            let by_path = !paths.is_empty()
                && paths
                    .iter()
                    .any(|path| matches_path_pattern(path, &entry.reservation.path_pattern));
            let release_all_for_agent = lock_ids.is_empty() && paths.is_empty();
            if by_id || by_path || release_all_for_agent {
                entry.reservation.released_ts = released_at.clone();
                released += 1;
            }
        }

        self.save_store(&store)?;
        Ok(LockReleaseResponse {
            released,
            released_at,
        })
    }

    fn list_reservations(
        &self,
        project: &str,
        active_only: bool,
    ) -> Result<Vec<FileReservation>, String> {
        let mut store = self.load_store()?;
        Self::cleanup_expired(&mut store);
        self.save_store(&store)?;

        let reservations = store
            .reservations
            .into_iter()
            .filter(|entry| entry.project == project)
            .filter(|entry| {
                if !active_only {
                    return true;
                }
                entry.reservation.released_ts.trim().is_empty()
            })
            .map(|entry| entry.reservation)
            .collect();
        Ok(reservations)
    }
}

fn resolve_default_lock_store_path() -> PathBuf {
    if let Some(path) = std::env::var_os("FORGE_LOCK_STORE_PATH") {
        return PathBuf::from(path);
    }

    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".local");
        path.push("share");
        path.push("forge");
        path.push("file-locks.json");
        return path;
    }

    PathBuf::from("file-locks.json")
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

pub fn run_for_test(args: &[&str], backend: &dyn LockBackend) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_with_backend(&owned_args, backend, &mut stdout, &mut stderr);
    CommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

pub fn run_with_backend(
    args: &[String],
    backend: &dyn LockBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

// ---------------------------------------------------------------------------
// Parsed arguments
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Subcommand {
    Help,
    Claim {
        agent: String,
        paths: Vec<String>,
        ttl_seconds: i64,
        exclusive: bool,
        reason: String,
        force: bool,
    },
    Release {
        agent: String,
        paths: Vec<String>,
        lock_ids: Vec<i64>,
    },
    Status {
        agent: String,
        paths: Vec<String>,
    },
    Check {
        paths: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    subcommand: Subcommand,
    json: bool,
    jsonl: bool,
}

// ---------------------------------------------------------------------------
// Core execution
// ---------------------------------------------------------------------------

fn execute(
    args: &[String],
    backend: &dyn LockBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    match parsed.subcommand {
        Subcommand::Help => {
            write_help(stdout).map_err(|err| err.to_string())?;
            Ok(())
        }
        Subcommand::Claim {
            agent,
            paths,
            ttl_seconds,
            exclusive,
            reason,
            force,
        } => execute_claim(
            backend,
            stdout,
            parsed.json,
            parsed.jsonl,
            &agent,
            &paths,
            ttl_seconds,
            exclusive,
            &reason,
            force,
        ),
        Subcommand::Release {
            agent,
            paths,
            lock_ids,
        } => execute_release(
            backend,
            stdout,
            parsed.json,
            parsed.jsonl,
            &agent,
            &paths,
            &lock_ids,
        ),
        Subcommand::Status { agent, paths } => {
            execute_status(backend, stdout, parsed.json, parsed.jsonl, &agent, &paths)
        }
        Subcommand::Check { paths } => {
            execute_check(backend, stdout, parsed.json, parsed.jsonl, &paths)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_claim(
    backend: &dyn LockBackend,
    stdout: &mut dyn Write,
    json: bool,
    jsonl: bool,
    agent_flag: &str,
    paths: &[String],
    ttl_seconds: i64,
    exclusive: bool,
    reason: &str,
    force: bool,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("at least one --path is required".to_string());
    }
    if ttl_seconds < 60 {
        return Err("ttl must be at least 1m".to_string());
    }

    let project = backend.resolve_project()?;
    let agent_name = backend.resolve_agent(agent_flag)?;

    let mut claim_result =
        backend.claim_locks(&project, &agent_name, paths, ttl_seconds, exclusive, reason)?;

    if !claim_result.conflicts.is_empty() {
        if force {
            // Collect conflict holder IDs
            let ids: Vec<i64> = claim_result
                .conflicts
                .iter()
                .flat_map(|c| c.holders.iter().map(|h| h.id))
                .filter(|id| *id != 0)
                .collect();

            if ids.is_empty() {
                return Err("no lock IDs available for force release".to_string());
            }

            backend.force_release(&project, &agent_name, &ids, reason)?;
            claim_result = backend.claim_locks(
                &project,
                &agent_name,
                paths,
                ttl_seconds,
                exclusive,
                reason,
            )?;
        } else {
            if json || jsonl {
                return write_json_output(stdout, &claim_result, jsonl);
            }
            print_lock_conflicts(stdout, &claim_result.conflicts)?;
            return Err("lock conflicts detected".to_string());
        }
    }

    if json || jsonl {
        return write_json_output(stdout, &claim_result, jsonl);
    }

    writeln!(stdout, "Lock claimed:").map_err(|e| e.to_string())?;
    writeln!(stdout, "  Agent:   {agent_name}").map_err(|e| e.to_string())?;
    writeln!(stdout, "  Paths:   {}", paths.join(", ")).map_err(|e| e.to_string())?;
    writeln!(stdout, "  TTL:     {}", format_duration_human(ttl_seconds))
        .map_err(|e| e.to_string())?;
    if !claim_result.granted.is_empty() {
        writeln!(stdout, "  Grants:").map_err(|e| e.to_string())?;
        for grant in &claim_result.granted {
            let expires = format_time_until(&grant.expires_ts);
            writeln!(
                stdout,
                "    - {} (id {}, expires {})",
                grant.path_pattern, grant.id, expires
            )
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn execute_release(
    backend: &dyn LockBackend,
    stdout: &mut dyn Write,
    json: bool,
    jsonl: bool,
    agent_flag: &str,
    paths: &[String],
    lock_ids: &[i64],
) -> Result<(), String> {
    let project = backend.resolve_project()?;
    let agent_name = backend.resolve_agent(agent_flag)?;

    let result = backend.release_locks(&project, &agent_name, paths, lock_ids)?;

    if json || jsonl {
        return write_json_output(stdout, &result, jsonl);
    }

    writeln!(stdout, "Released {} lock(s)", result.released).map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_status(
    backend: &dyn LockBackend,
    stdout: &mut dyn Write,
    json: bool,
    jsonl: bool,
    agent_filter: &str,
    path_filters: &[String],
) -> Result<(), String> {
    let project = backend.resolve_project()?;
    let claims = backend.list_reservations(&project, true)?;
    let filtered = filter_file_reservations(&claims, agent_filter, path_filters);

    if json || jsonl {
        return write_json_output(stdout, &filtered, jsonl);
    }

    if filtered.is_empty() {
        writeln!(stdout, "No locks found").map_err(|e| e.to_string())?;
        return Ok(());
    }

    let mut tw = TabWriter::new(&mut *stdout).padding(2);
    writeln!(tw, "LOCK-ID\tAGENT\tPATH\tEXPIRES\tEXCLUSIVE").map_err(|e| e.to_string())?;
    for claim in &filtered {
        let expires = format_time_until(&claim.expires_ts);
        let exclusive_str = format_yes_no(claim.exclusive);
        writeln!(
            tw,
            "{}\t{}\t{}\t{}\t{}",
            claim.id, claim.agent, claim.path_pattern, expires, exclusive_str
        )
        .map_err(|e| e.to_string())?;
    }
    tw.flush().map_err(|e| e.to_string())?;

    Ok(())
}

fn execute_check(
    backend: &dyn LockBackend,
    stdout: &mut dyn Write,
    json: bool,
    jsonl: bool,
    paths: &[String],
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("--path is required".to_string());
    }

    let project = backend.resolve_project()?;
    let claims = backend.list_reservations(&project, true)?;
    let results = build_check_results(paths, &claims);

    if json || jsonl {
        return write_json_output(stdout, &results, jsonl);
    }

    for result in &results {
        if result.claims.is_empty() {
            writeln!(stdout, "Path is clear: {}", result.path).map_err(|e| e.to_string())?;
            continue;
        }
        writeln!(stdout, "Path is locked: {}", result.path).map_err(|e| e.to_string())?;
        for claim in &result.claims {
            let expires = format_time_until(&claim.expires_ts);
            writeln!(stdout, "  Holder: {}", claim.agent).map_err(|e| e.to_string())?;
            writeln!(stdout, "  Pattern: {}", claim.path_pattern).map_err(|e| e.to_string())?;
            writeln!(stdout, "  Expires: {expires}").map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Arg parsing
// ---------------------------------------------------------------------------

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let start = if args.first().is_some_and(|arg| arg == "lock") {
        1
    } else {
        0
    };

    // Scan for global format flags and the subcommand
    let mut json = false;
    let mut jsonl = false;
    let mut subcommand_name: Option<String> = None;
    let mut subcommand_start = start;

    let mut idx = start;
    while idx < args.len() {
        let token = args[idx].as_str();
        match token {
            "--json" => {
                json = true;
                idx += 1;
            }
            "--jsonl" => {
                jsonl = true;
                idx += 1;
            }
            "-h" | "--help" | "help" if subcommand_name.is_none() => {
                return Ok(ParsedArgs {
                    subcommand: Subcommand::Help,
                    json,
                    jsonl,
                });
            }
            _ => {
                if subcommand_name.is_none() && !token.starts_with('-') {
                    subcommand_name = Some(token.to_string());
                    subcommand_start = idx;
                }
                idx += 1;
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl are mutually exclusive".to_string());
    }

    let subcommand = match subcommand_name.as_deref() {
        None | Some("help") | Some("-h") | Some("--help") => Subcommand::Help,
        Some("claim") => parse_claim_args(&args[subcommand_start..])?,
        Some("release") => parse_release_args(&args[subcommand_start..])?,
        Some("status") => parse_status_args(&args[subcommand_start..])?,
        Some("check") => parse_check_args(&args[subcommand_start..])?,
        Some(other) => return Err(format!("unknown lock subcommand: {other}")),
    };

    Ok(ParsedArgs {
        subcommand,
        json,
        jsonl,
    })
}

fn parse_claim_args(args: &[String]) -> Result<Subcommand, String> {
    let start = if args.first().is_some_and(|a| a == "claim") {
        1
    } else {
        0
    };

    let mut agent = String::new();
    let mut paths: Vec<String> = Vec::new();
    let mut ttl_seconds: i64 = 3600; // default 1 hour
    let mut exclusive = true;
    let mut reason = String::new();
    let mut force = false;

    let mut idx = start;
    while idx < args.len() {
        let token = args[idx].as_str();
        match token {
            "--agent" | "-a" => {
                agent = take_value(args, idx, "--agent")?;
                idx += 2;
            }
            "--path" | "-p" => {
                let value = take_value(args, idx, "--path")?;
                paths.push(value);
                idx += 2;
            }
            "--ttl" => {
                let value = take_value(args, idx, "--ttl")?;
                ttl_seconds = parse_ttl_seconds(&value)?;
                idx += 2;
            }
            "--exclusive" => {
                let value = take_value(args, idx, "--exclusive")?;
                exclusive = parse_bool_flag(&value, "--exclusive")?;
                idx += 2;
            }
            "--reason" => {
                reason = take_value(args, idx, "--reason")?;
                idx += 2;
            }
            "--force" => {
                force = true;
                idx += 1;
            }
            "--json" | "--jsonl" => {
                idx += 1; // handled at top level
            }
            "-h" | "--help" | "help" => {
                return Ok(Subcommand::Help);
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag for lock claim: {flag}"));
            }
            _ => {
                return Err(format!(
                    "lock claim does not accept positional arguments: {token}"
                ));
            }
        }
    }

    Ok(Subcommand::Claim {
        agent,
        paths,
        ttl_seconds,
        exclusive,
        reason,
        force,
    })
}

fn parse_release_args(args: &[String]) -> Result<Subcommand, String> {
    let start = if args.first().is_some_and(|a| a == "release") {
        1
    } else {
        0
    };

    let mut agent = String::new();
    let mut paths: Vec<String> = Vec::new();
    let mut lock_ids: Vec<i64> = Vec::new();

    let mut idx = start;
    while idx < args.len() {
        let token = args[idx].as_str();
        match token {
            "--agent" | "-a" => {
                agent = take_value(args, idx, "--agent")?;
                idx += 2;
            }
            "--path" | "-p" => {
                let value = take_value(args, idx, "--path")?;
                paths.push(value);
                idx += 2;
            }
            "--lock-id" => {
                let value = take_value(args, idx, "--lock-id")?;
                let mut parsed = parse_lock_ids_csv(&value)?;
                lock_ids.append(&mut parsed);
                idx += 2;
            }
            "--json" | "--jsonl" => {
                idx += 1;
            }
            "-h" | "--help" | "help" => {
                return Ok(Subcommand::Help);
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag for lock release: {flag}"));
            }
            _ => {
                return Err(format!(
                    "lock release does not accept positional arguments: {token}"
                ));
            }
        }
    }

    Ok(Subcommand::Release {
        agent,
        paths,
        lock_ids,
    })
}

fn parse_status_args(args: &[String]) -> Result<Subcommand, String> {
    let start = if args.first().is_some_and(|a| a == "status") {
        1
    } else {
        0
    };

    let mut agent = String::new();
    let mut paths: Vec<String> = Vec::new();

    let mut idx = start;
    while idx < args.len() {
        let token = args[idx].as_str();
        match token {
            "--agent" | "-a" => {
                agent = take_value(args, idx, "--agent")?;
                idx += 2;
            }
            "--path" | "-p" => {
                let value = take_value(args, idx, "--path")?;
                paths.push(value);
                idx += 2;
            }
            "--json" | "--jsonl" => {
                idx += 1;
            }
            "-h" | "--help" | "help" => {
                return Ok(Subcommand::Help);
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag for lock status: {flag}"));
            }
            _ => {
                return Err(format!(
                    "lock status does not accept positional arguments: {token}"
                ));
            }
        }
    }

    Ok(Subcommand::Status { agent, paths })
}

fn parse_check_args(args: &[String]) -> Result<Subcommand, String> {
    let start = if args.first().is_some_and(|a| a == "check") {
        1
    } else {
        0
    };

    let mut paths: Vec<String> = Vec::new();

    let mut idx = start;
    while idx < args.len() {
        let token = args[idx].as_str();
        match token {
            "--path" | "-p" => {
                let value = take_value(args, idx, "--path")?;
                paths.push(value);
                idx += 2;
            }
            "--json" | "--jsonl" => {
                idx += 1;
            }
            "-h" | "--help" | "help" => {
                return Ok(Subcommand::Help);
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag for lock check: {flag}"));
            }
            _ => {
                return Err(format!(
                    "lock check does not accept positional arguments: {token}"
                ));
            }
        }
    }

    Ok(Subcommand::Check { paths })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

fn parse_ttl_seconds(raw: &str) -> Result<i64, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("ttl value cannot be empty".to_string());
    }

    if let Some(value) = trimmed.strip_suffix('h') {
        let hours: f64 = value
            .parse()
            .map_err(|_| format!("invalid ttl: {trimmed}"))?;
        return Ok((hours * 3600.0).round() as i64);
    }
    if let Some(value) = trimmed.strip_suffix('m') {
        let minutes: f64 = value
            .parse()
            .map_err(|_| format!("invalid ttl: {trimmed}"))?;
        return Ok((minutes * 60.0).round() as i64);
    }
    if let Some(value) = trimmed.strip_suffix('s') {
        let seconds: f64 = value
            .parse()
            .map_err(|_| format!("invalid ttl: {trimmed}"))?;
        return Ok(seconds.round() as i64);
    }

    // Try as raw seconds
    trimmed
        .parse::<i64>()
        .map_err(|_| format!("invalid ttl: {trimmed} (use e.g. 30m, 1h, 3600)"))
}

fn parse_bool_flag(raw: &str, flag: &str) -> Result<bool, String> {
    match raw.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(format!("invalid value for {flag}: {raw}")),
    }
}

fn parse_lock_ids_csv(raw: &str) -> Result<Vec<i64>, String> {
    let mut ids = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let id: i64 = part
            .parse()
            .map_err(|_| format!("invalid lock id {part:?}"))?;
        ids.push(id);
    }
    Ok(ids)
}

fn format_duration_human(seconds: i64) -> String {
    if seconds < 60 {
        return format!("{seconds}s");
    }
    if seconds < 3600 {
        let m = seconds / 60;
        let s = seconds % 60;
        if s == 0 {
            return format!("{m}m0s");
        }
        return format!("{m}m{s}s");
    }
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if m == 0 && s == 0 {
        return format!("{h}h0m0s");
    }
    if s == 0 {
        return format!("{h}h{m}m0s");
    }
    format!("{h}h{m}m{s}s")
}

fn format_time_until(expires_ts: &str) -> String {
    let trimmed = expires_ts.trim();
    if trimmed.is_empty() {
        return "-".to_string();
    }

    // Parse RFC3339 timestamp
    let parsed = match chrono::DateTime::parse_from_rfc3339(trimmed) {
        Ok(dt) => dt,
        Err(_) => return "-".to_string(),
    };

    let now = chrono::Utc::now();
    let remaining = parsed.signed_duration_since(now);
    let total_seconds = remaining.num_seconds();

    if total_seconds < 0 {
        return "expired".to_string();
    }
    if total_seconds < 60 {
        return "in <1m".to_string();
    }
    let total_minutes = total_seconds / 60;
    if total_minutes < 60 {
        return format!("in {total_minutes}m");
    }
    let total_hours = total_seconds / 3600;
    if total_hours < 24 {
        return format!("in {total_hours}h");
    }
    let days = total_hours / 24;
    format!("in {days}d")
}

fn format_yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn normalize_lock_path(value: &str) -> String {
    value.trim().replace('\\', "/")
}

fn matches_path_pattern(path_value: &str, pattern: &str) -> bool {
    let path_value = normalize_lock_path(path_value);
    let pattern = normalize_lock_path(pattern);
    if path_value.is_empty() || pattern.is_empty() {
        return false;
    }
    if path_value == pattern {
        return true;
    }
    // Bidirectional glob matching (path→pattern and pattern→path)
    if glob_match(&pattern, &path_value) {
        return true;
    }
    if glob_match(&path_value, &pattern) {
        return true;
    }
    false
}

/// Simple path-style glob matching (supports `*` and `?` like Go's `path.Match`).
fn glob_match(pattern: &str, name: &str) -> bool {
    let pattern_bytes = pattern.as_bytes();
    let name_bytes = name.as_bytes();
    let mut pi = 0;
    let mut ni = 0;

    while pi < pattern_bytes.len() {
        match pattern_bytes[pi] {
            b'*' => {
                pi += 1;
                // Match any sequence except '/'
                if pi >= pattern_bytes.len() {
                    // Check no '/' in remaining name
                    return !name_bytes[ni..].contains(&b'/');
                }
                for start in ni..=name_bytes.len() {
                    if start > ni && name_bytes[start - 1] == b'/' {
                        break;
                    }
                    if glob_match(
                        std::str::from_utf8(&pattern_bytes[pi..]).unwrap_or(""),
                        std::str::from_utf8(&name_bytes[start..]).unwrap_or(""),
                    ) {
                        return true;
                    }
                }
                return false;
            }
            b'?' => {
                if ni >= name_bytes.len() || name_bytes[ni] == b'/' {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
            c => {
                if ni >= name_bytes.len() || name_bytes[ni] != c {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
        }
    }
    ni >= name_bytes.len()
}

fn filter_file_reservations(
    claims: &[FileReservation],
    agent_filter: &str,
    paths: &[String],
) -> Vec<FileReservation> {
    if agent_filter.is_empty() && paths.is_empty() {
        return claims.to_vec();
    }

    claims
        .iter()
        .filter(|claim| {
            if !agent_filter.is_empty() && !claim.agent.eq_ignore_ascii_case(agent_filter) {
                return false;
            }
            if !paths.is_empty() {
                let matched = paths
                    .iter()
                    .any(|p| matches_path_pattern(p, &claim.path_pattern));
                if !matched {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}

fn build_check_results(paths: &[String], claims: &[FileReservation]) -> Vec<LockCheckResult> {
    paths
        .iter()
        .map(|path_value| {
            let matching_claims: Vec<FileReservation> = claims
                .iter()
                .filter(|claim| matches_path_pattern(path_value, &claim.path_pattern))
                .cloned()
                .collect();
            LockCheckResult {
                path: path_value.clone(),
                claims: matching_claims,
            }
        })
        .collect()
}

fn print_lock_conflicts(
    stdout: &mut dyn Write,
    conflicts: &[FileReservationConflict],
) -> Result<(), String> {
    if conflicts.is_empty() {
        return Ok(());
    }
    writeln!(stdout, "Conflicts detected:").map_err(|e| e.to_string())?;
    for conflict in conflicts {
        writeln!(stdout, "  Path: {}", conflict.path).map_err(|e| e.to_string())?;
        for holder in &conflict.holders {
            let expires = format_time_until(&holder.expires_ts);
            writeln!(
                stdout,
                "    - {} (id {}, expires {})",
                holder.agent, holder.id, expires
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn write_json_output<T: Serialize>(
    stdout: &mut dyn Write,
    value: &T,
    jsonl: bool,
) -> Result<(), String> {
    let json_value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    if jsonl {
        if let serde_json::Value::Array(items) = json_value {
            for item in items {
                let line = serde_json::to_string(&item).map_err(|e| e.to_string())?;
                writeln!(stdout, "{line}").map_err(|e| e.to_string())?;
            }
        } else {
            let line = serde_json::to_string(&json_value).map_err(|e| e.to_string())?;
            writeln!(stdout, "{line}").map_err(|e| e.to_string())?;
        }
    } else {
        let text = serde_json::to_string_pretty(&json_value).map_err(|e| e.to_string())?;
        writeln!(stdout, "{text}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

fn write_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(
        stdout,
        "Manage advisory file locks via Agent Mail for multi-agent coordination."
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Usage:")?;
    writeln!(stdout, "  forge lock <command> [options]")?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  claim    Claim file locks")?;
    writeln!(stdout, "  release  Release file locks")?;
    writeln!(stdout, "  status   Show lock status")?;
    writeln!(stdout, "  check    Check if a path is locked")?;
    writeln!(stdout)?;
    writeln!(stdout, "Claim Flags:")?;
    writeln!(stdout, "  -a, --agent string     agent name (Agent Mail)")?;
    writeln!(
        stdout,
        "  -p, --path string      file path or glob pattern (repeatable)"
    )?;
    writeln!(
        stdout,
        "      --ttl duration     lock duration (e.g., 30m) (default 1h)"
    )?;
    writeln!(
        stdout,
        "      --exclusive bool   exclusive lock (default true)"
    )?;
    writeln!(stdout, "      --reason string    reason for the lock")?;
    writeln!(
        stdout,
        "      --force            force release conflicting locks"
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Release Flags:")?;
    writeln!(stdout, "  -a, --agent string     agent name (Agent Mail)")?;
    writeln!(
        stdout,
        "  -p, --path string      file path or glob pattern (repeatable)"
    )?;
    writeln!(
        stdout,
        "      --lock-id string   lock ID to release (repeatable)"
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Status Flags:")?;
    writeln!(stdout, "  -a, --agent string     filter by agent name")?;
    writeln!(
        stdout,
        "  -p, --path string      filter by file path or glob pattern"
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Check Flags:")?;
    writeln!(
        stdout,
        "  -p, --path string      file path to check (repeatable)"
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn test_backend() -> InMemoryLockBackend {
        InMemoryLockBackend::with_project_and_agent("test-project", "test-agent")
    }

    fn fs_backend(name: &str) -> FilesystemLockBackend {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos();
        let path = std::env::temp_dir().join(format!("forge-lock-test-{name}-{nanos}.json"));
        FilesystemLockBackend::new(path)
    }

    #[test]
    fn filesystem_backend_claim_release_round_trip() {
        let backend = fs_backend("roundtrip");
        let claim = backend
            .claim_locks(
                "proj",
                "agent-a",
                &["src/main.rs".to_string()],
                300,
                true,
                "edit",
            )
            .unwrap_or_else(|err| panic!("claim should succeed: {err}"));
        assert_eq!(claim.conflicts.len(), 0);
        assert_eq!(claim.granted.len(), 1);

        let active = backend
            .list_reservations("proj", true)
            .unwrap_or_else(|err| panic!("list reservations: {err}"));
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].path_pattern, "src/main.rs");

        let released = backend
            .release_locks("proj", "agent-a", &["src/main.rs".to_string()], &[])
            .unwrap_or_else(|err| panic!("release should succeed: {err}"));
        assert_eq!(released.released, 1);

        let after = backend
            .list_reservations("proj", true)
            .unwrap_or_else(|err| panic!("list reservations: {err}"));
        assert!(after.is_empty());
    }

    #[test]
    fn filesystem_backend_reports_conflicts_for_overlapping_paths() {
        let backend = fs_backend("conflicts");
        backend
            .claim_locks("proj", "agent-a", &["src/*.rs".to_string()], 300, true, "")
            .unwrap_or_else(|err| panic!("initial claim should succeed: {err}"));

        let second = backend
            .claim_locks(
                "proj",
                "agent-b",
                &["src/main.rs".to_string()],
                300,
                true,
                "",
            )
            .unwrap_or_else(|err| panic!("second claim should return conflicts: {err}"));

        assert_eq!(second.granted.len(), 0);
        assert_eq!(second.conflicts.len(), 1);
        assert_eq!(second.conflicts[0].holders[0].agent, "agent-a");
    }

    fn assert_success(out: &CommandOutput) {
        assert_eq!(out.exit_code, 0, "expected exit 0, stderr: {}", out.stderr);
        assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
    }

    fn assert_failure(out: &CommandOutput) {
        assert_ne!(
            out.exit_code, 0,
            "expected non-zero exit, stdout: {}",
            out.stdout
        );
    }

    // --- Help ---

    #[test]
    fn lock_help_shows_subcommands() {
        let backend = test_backend();
        let out = run_for_test(&["lock"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("claim"));
        assert!(out.stdout.contains("release"));
        assert!(out.stdout.contains("status"));
        assert!(out.stdout.contains("check"));
    }

    #[test]
    fn lock_help_flag() {
        let backend = test_backend();
        let out = run_for_test(&["lock", "--help"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Manage advisory file locks"));
    }

    // --- Claim ---

    #[test]
    fn claim_requires_path() {
        let backend = test_backend();
        let out = run_for_test(&["lock", "claim", "--agent", "bot"], &backend);
        assert_failure(&out);
        assert!(out.stderr.contains("at least one --path is required"));
    }

    #[test]
    fn claim_rejects_short_ttl() {
        let backend = test_backend();
        let out = run_for_test(
            &[
                "lock",
                "claim",
                "--agent",
                "bot",
                "--path",
                "src/main.rs",
                "--ttl",
                "30s",
            ],
            &backend,
        );
        assert_failure(&out);
        assert!(out.stderr.contains("ttl must be at least 1m"));
    }

    #[test]
    fn claim_success_text_output() {
        let mut backend = test_backend();
        backend.set_claim_response(LockClaimResponse {
            granted: vec![FileReservationGrant {
                id: 42,
                path_pattern: "src/*.rs".to_string(),
                exclusive: true,
                reason: "editing".to_string(),
                expires_ts: "2099-01-01T00:00:00Z".to_string(),
            }],
            conflicts: Vec::new(),
        });

        let out = run_for_test(
            &[
                "lock", "claim", "--agent", "bot", "--path", "src/*.rs", "--reason", "editing",
            ],
            &backend,
        );
        assert_success(&out);
        assert!(out.stdout.contains("Lock claimed:"));
        assert!(out.stdout.contains("Agent:   bot"));
        assert!(out.stdout.contains("src/*.rs"));
        assert!(out.stdout.contains("id 42"));
    }

    #[test]
    fn claim_success_json_output() {
        let mut backend = test_backend();
        backend.set_claim_response(LockClaimResponse {
            granted: vec![FileReservationGrant {
                id: 42,
                path_pattern: "src/*.rs".to_string(),
                exclusive: true,
                reason: "editing".to_string(),
                expires_ts: "2099-01-01T00:00:00Z".to_string(),
            }],
            conflicts: Vec::new(),
        });

        let out = run_for_test(
            &[
                "lock", "--json", "claim", "--agent", "bot", "--path", "src/*.rs",
            ],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["granted"][0]["id"], 42);
        assert_eq!(parsed["granted"][0]["path_pattern"], "src/*.rs");
    }

    #[test]
    fn claim_conflicts_without_force() {
        let mut backend = test_backend();
        backend.set_claim_response(LockClaimResponse {
            granted: Vec::new(),
            conflicts: vec![FileReservationConflict {
                path: "src/main.rs".to_string(),
                holders: vec![FileReservationHolder {
                    id: 10,
                    agent: "other-agent".to_string(),
                    path_pattern: "src/*.rs".to_string(),
                    exclusive: true,
                    expires_ts: "2099-01-01T00:00:00Z".to_string(),
                }],
            }],
        });

        let out = run_for_test(
            &["lock", "claim", "--agent", "bot", "--path", "src/main.rs"],
            &backend,
        );
        assert_failure(&out);
        assert!(out.stdout.contains("Conflicts detected:"));
        assert!(out.stderr.contains("lock conflicts detected"));
    }

    // --- Release ---

    #[test]
    fn release_success_text() {
        let mut backend = test_backend();
        backend.set_release_response(LockReleaseResponse {
            released: 2,
            released_at: "2026-01-01T00:00:00Z".to_string(),
        });

        let out = run_for_test(
            &["lock", "release", "--agent", "bot", "--path", "src/*.rs"],
            &backend,
        );
        assert_success(&out);
        assert!(out.stdout.contains("Released 2 lock(s)"));
    }

    #[test]
    fn release_json_output() {
        let mut backend = test_backend();
        backend.set_release_response(LockReleaseResponse {
            released: 1,
            released_at: "2026-01-01T00:00:00Z".to_string(),
        });

        let out = run_for_test(
            &[
                "lock",
                "--json",
                "release",
                "--agent",
                "bot",
                "--lock-id",
                "42",
            ],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["released"], 1);
    }

    // --- Status ---

    #[test]
    fn status_no_locks() {
        let backend = test_backend();
        let out = run_for_test(&["lock", "status"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("No locks found"));
    }

    #[test]
    fn status_with_locks_table() {
        let mut backend = test_backend();
        backend.reservations = vec![FileReservation {
            id: 1,
            agent: "bot-1".to_string(),
            path_pattern: "src/*.rs".to_string(),
            exclusive: true,
            reason: "editing".to_string(),
            created_ts: "2026-01-01T00:00:00Z".to_string(),
            expires_ts: "2099-01-01T00:00:00Z".to_string(),
            released_ts: String::new(),
        }];

        let out = run_for_test(&["lock", "status"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("LOCK-ID"));
        assert!(out.stdout.contains("AGENT"));
        assert!(out.stdout.contains("PATH"));
        assert!(out.stdout.contains("EXPIRES"));
        assert!(out.stdout.contains("EXCLUSIVE"));
        assert!(out.stdout.contains("bot-1"));
        assert!(out.stdout.contains("src/*.rs"));
    }

    #[test]
    fn status_filtered_by_agent() {
        let mut backend = test_backend();
        backend.reservations = vec![
            FileReservation {
                id: 1,
                agent: "bot-1".to_string(),
                path_pattern: "src/*.rs".to_string(),
                exclusive: true,
                reason: String::new(),
                created_ts: "2026-01-01T00:00:00Z".to_string(),
                expires_ts: "2099-01-01T00:00:00Z".to_string(),
                released_ts: String::new(),
            },
            FileReservation {
                id: 2,
                agent: "bot-2".to_string(),
                path_pattern: "docs/*.md".to_string(),
                exclusive: false,
                reason: String::new(),
                created_ts: "2026-01-01T00:00:00Z".to_string(),
                expires_ts: "2099-01-01T00:00:00Z".to_string(),
                released_ts: String::new(),
            },
        ];

        let out = run_for_test(&["lock", "status", "--agent", "bot-1"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("bot-1"));
        assert!(!out.stdout.contains("bot-2"));
    }

    #[test]
    fn status_json_output() {
        let mut backend = test_backend();
        backend.reservations = vec![FileReservation {
            id: 1,
            agent: "bot-1".to_string(),
            path_pattern: "src/*.rs".to_string(),
            exclusive: true,
            reason: "editing".to_string(),
            created_ts: "2026-01-01T00:00:00Z".to_string(),
            expires_ts: "2099-01-01T00:00:00Z".to_string(),
            released_ts: String::new(),
        }];

        let out = run_for_test(&["lock", "--json", "status"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["agent"], "bot-1");
    }

    // --- Check ---

    #[test]
    fn check_requires_path() {
        let backend = test_backend();
        let out = run_for_test(&["lock", "check"], &backend);
        assert_failure(&out);
        assert!(out.stderr.contains("--path is required"));
    }

    #[test]
    fn check_clear_path() {
        let backend = test_backend();
        let out = run_for_test(&["lock", "check", "--path", "src/main.rs"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Path is clear: src/main.rs"));
    }

    #[test]
    fn check_locked_path() {
        let mut backend = test_backend();
        backend.reservations = vec![FileReservation {
            id: 1,
            agent: "bot-1".to_string(),
            path_pattern: "src/*.rs".to_string(),
            exclusive: true,
            reason: "editing".to_string(),
            created_ts: "2026-01-01T00:00:00Z".to_string(),
            expires_ts: "2099-01-01T00:00:00Z".to_string(),
            released_ts: String::new(),
        }];

        let out = run_for_test(&["lock", "check", "--path", "src/main.rs"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Path is locked: src/main.rs"));
        assert!(out.stdout.contains("Holder: bot-1"));
        assert!(out.stdout.contains("Pattern: src/*.rs"));
    }

    #[test]
    fn check_json_output() {
        let mut backend = test_backend();
        backend.reservations = vec![FileReservation {
            id: 1,
            agent: "bot-1".to_string(),
            path_pattern: "src/*.rs".to_string(),
            exclusive: true,
            reason: String::new(),
            created_ts: "2026-01-01T00:00:00Z".to_string(),
            expires_ts: "2099-01-01T00:00:00Z".to_string(),
            released_ts: String::new(),
        }];

        let out = run_for_test(
            &["lock", "--json", "check", "--path", "src/main.rs"],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["path"], "src/main.rs");
        assert!(!parsed[0]["claims"].as_array().unwrap().is_empty());
    }

    // --- parse_lock_ids_csv ---

    #[test]
    fn parse_lock_ids_csv_basic() {
        let ids = parse_lock_ids_csv("1,2,3").unwrap();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn parse_lock_ids_csv_single() {
        let ids = parse_lock_ids_csv("42").unwrap();
        assert_eq!(ids, vec![42]);
    }

    #[test]
    fn parse_lock_ids_csv_invalid() {
        let result = parse_lock_ids_csv("bad");
        assert!(result.is_err());
    }

    // --- matches_path_pattern ---

    #[test]
    fn matches_exact() {
        assert!(matches_path_pattern("src/main.go", "src/main.go"));
    }

    #[test]
    fn matches_glob() {
        assert!(matches_path_pattern("src/main.go", "src/*.go"));
    }

    #[test]
    fn no_match() {
        assert!(!matches_path_pattern("src/main.go", "docs/*.md"));
    }

    #[test]
    fn normalize_backslash() {
        assert!(matches_path_pattern("src\\main.go", "src/main.go"));
    }

    // --- format_duration_human ---

    #[test]
    fn format_duration_1h() {
        assert_eq!(format_duration_human(3600), "1h0m0s");
    }

    #[test]
    fn format_duration_30m() {
        assert_eq!(format_duration_human(1800), "30m0s");
    }

    #[test]
    fn format_duration_90s() {
        assert_eq!(format_duration_human(90), "1m30s");
    }

    // --- format_time_until ---

    #[test]
    fn format_time_until_empty() {
        assert_eq!(format_time_until(""), "-");
    }

    #[test]
    fn format_time_until_expired() {
        assert_eq!(format_time_until("2020-01-01T00:00:00Z"), "expired");
    }

    #[test]
    fn format_time_until_far_future() {
        let result = format_time_until("2099-01-01T00:00:00Z");
        assert!(
            result.starts_with("in "),
            "expected 'in ...', got: {result}"
        );
    }

    // --- Unknown subcommand ---

    #[test]
    fn unknown_subcommand() {
        let backend = test_backend();
        let out = run_for_test(&["lock", "foobar"], &backend);
        assert_failure(&out);
        assert!(out.stderr.contains("unknown lock subcommand: foobar"));
    }

    // --- Agent resolution ---

    #[test]
    fn claim_requires_agent_when_not_configured() {
        let backend = InMemoryLockBackend {
            project: "test-project".to_string(),
            agent: String::new(),
            ..Default::default()
        };
        let out = run_for_test(&["lock", "claim", "--path", "src/main.rs"], &backend);
        assert_failure(&out);
        assert!(out.stderr.contains("agent name is required"));
    }

    #[test]
    fn claim_uses_backend_agent_when_flag_empty() {
        let mut backend = test_backend();
        backend.set_claim_response(LockClaimResponse {
            granted: vec![FileReservationGrant {
                id: 1,
                path_pattern: "src/*.rs".to_string(),
                exclusive: true,
                reason: String::new(),
                expires_ts: "2099-01-01T00:00:00Z".to_string(),
            }],
            conflicts: Vec::new(),
        });

        let out = run_for_test(&["lock", "claim", "--path", "src/*.rs"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Agent:   test-agent"));
    }
}
