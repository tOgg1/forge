use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;

use crate::spawn_loop::SpawnOptions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct StopConfig {
    pub quant: Option<QuantStopConfig>,
    pub qual: Option<QualStopConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuantStopConfig {
    pub cmd: String,
    pub every_n: i32,
    pub when: String,
    pub decision: String,
    pub exit_codes: Vec<i32>,
    pub exit_invert: bool,
    pub stdout_mode: String,
    pub stderr_mode: String,
    pub stdout_regex: String,
    pub stderr_regex: String,
    pub timeout_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QualStopConfig {
    pub every_n: i32,
    pub prompt: String,
    pub is_prompt_path: bool,
    pub on_invalid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueItem {
    Pause {
        duration_seconds: i64,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopCreateSpec {
    pub name: String,
    pub repo: String,
    pub pool: String,
    pub profile: String,
    pub prompt: String,
    pub prompt_msg: String,
    pub interval_seconds: i64,
    pub max_runtime_seconds: i64,
    pub max_iterations: i32,
    pub tags: Vec<String>,
    pub stop_config: StopConfig,
}

pub trait UpBackend {
    fn list_loop_names(&self) -> Result<Vec<String>, String>;
    fn create_loop(&mut self, spec: &LoopCreateSpec) -> Result<LoopRecord, String>;
    fn enqueue_item(&mut self, loop_id: &str, item: QueueItem) -> Result<(), String>;
    fn start_loop(
        &mut self,
        loop_id: &str,
        spawn_owner: &str,
        spawn_options: &SpawnOptions,
        warning_writer: &mut dyn Write,
    ) -> Result<(), String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryUpBackend {
    existing_names: Vec<String>,
    next_id: u64,
    pub created_specs: Vec<LoopCreateSpec>,
    pub created_records: Vec<LoopRecord>,
    pub queued: Vec<(String, QueueItem)>,
    pub starts: Vec<(String, String)>,
}

impl InMemoryUpBackend {
    pub fn with_existing_names(names: Vec<String>) -> Self {
        Self {
            existing_names: names,
            ..Default::default()
        }
    }
}

impl UpBackend for InMemoryUpBackend {
    fn list_loop_names(&self) -> Result<Vec<String>, String> {
        let mut all = self.existing_names.clone();
        for record in &self.created_records {
            all.push(record.name.clone());
        }
        Ok(all)
    }

    fn create_loop(&mut self, spec: &LoopCreateSpec) -> Result<LoopRecord, String> {
        self.next_id += 1;
        let repo_path = if spec.repo.trim().is_empty() {
            std::env::current_dir()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            spec.repo.clone()
        };
        let mut stored_spec = spec.clone();
        stored_spec.prompt = crate::prompt_resolution::resolve_prompt_name_or_path(
            Path::new(&repo_path),
            &spec.prompt,
        );
        let record = LoopRecord {
            id: format!("loop-{:03}", self.next_id),
            short_id: format!("s{:03}", self.next_id),
            name: spec.name.clone(),
        };
        self.created_specs.push(stored_spec);
        self.created_records.push(record.clone());
        Ok(record)
    }

    fn enqueue_item(&mut self, loop_id: &str, item: QueueItem) -> Result<(), String> {
        self.queued.push((loop_id.to_string(), item));
        Ok(())
    }

    fn start_loop(
        &mut self,
        loop_id: &str,
        spawn_owner: &str,
        _spawn_options: &SpawnOptions,
        _warning_writer: &mut dyn Write,
    ) -> Result<(), String> {
        self.starts
            .push((loop_id.to_string(), spawn_owner.to_string()));
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SqliteUpBackend {
    db_path: PathBuf,
}

impl SqliteUpBackend {
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

    fn resolve_repo_for_create(&self, value: &str) -> Result<String, String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            let cwd = std::env::current_dir().map_err(|err| format!("resolve cwd: {err}"))?;
            return Ok(cwd.to_string_lossy().into_owned());
        }
        normalize_repo_filter(trimmed)
    }
}

impl UpBackend for SqliteUpBackend {
    fn list_loop_names(&self) -> Result<Vec<String>, String> {
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
        Ok(loops.into_iter().map(|entry| entry.name).collect())
    }

    fn create_loop(&mut self, spec: &LoopCreateSpec) -> Result<LoopRecord, String> {
        let db = self.open_db()?;
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let pool_repo = forge_db::pool_repository::PoolRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let repo_path = self.resolve_repo_for_create(&spec.repo)?;
        let pool_id = if spec.pool.trim().is_empty() {
            String::new()
        } else {
            resolve_pool_ref(&pool_repo, &spec.pool)?
        };
        let profile_id = if spec.profile.trim().is_empty() {
            String::new()
        } else {
            resolve_profile_ref(&profile_repo, &spec.profile)?
        };

        let stop_metadata = if spec.stop_config.quant.is_some() || spec.stop_config.qual.is_some() {
            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                "stop_config".to_string(),
                serde_json::to_value(&spec.stop_config)
                    .map_err(|err| format!("serialize stop config: {err}"))?,
            );
            Some(metadata)
        } else {
            None
        };
        let resolved_prompt = crate::prompt_resolution::resolve_prompt_name_or_path(
            Path::new(&repo_path),
            &spec.prompt,
        );

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: spec.name.clone(),
            repo_path: repo_path.clone(),
            base_prompt_path: resolved_prompt,
            base_prompt_msg: spec.prompt_msg.clone(),
            interval_seconds: spec.interval_seconds,
            max_iterations: i64::from(spec.max_iterations),
            max_runtime_seconds: spec.max_runtime_seconds,
            pool_id,
            profile_id,
            state: forge_db::loop_repository::LoopState::Stopped,
            tags: spec.tags.clone(),
            metadata: stop_metadata,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .map_err(|err| format!("create loop: {err}"))?;

        Ok(LoopRecord {
            id: loop_entry.id,
            short_id: loop_entry.short_id,
            name: loop_entry.name,
        })
    }

    fn enqueue_item(&mut self, loop_id: &str, item: QueueItem) -> Result<(), String> {
        let db = self.open_db()?;
        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);

        let (item_type, payload) = match item {
            QueueItem::Pause {
                duration_seconds,
                reason,
            } => (
                "pause".to_string(),
                json!({ "duration_seconds": duration_seconds, "reason": reason }).to_string(),
            ),
        };

        let mut queue_item = forge_db::loop_queue_repository::LoopQueueItem {
            item_type,
            payload,
            ..Default::default()
        };
        queue_repo
            .enqueue(loop_id, std::slice::from_mut(&mut queue_item))
            .map_err(|err| format!("enqueue queue item: {err}"))
    }

    fn start_loop(
        &mut self,
        loop_id: &str,
        spawn_owner: &str,
        spawn_options: &SpawnOptions,
        warning_writer: &mut dyn Write,
    ) -> Result<(), String> {
        let spawn_result = crate::spawn_loop::start_loop_runner(
            loop_id,
            spawn_owner,
            spawn_options,
            warning_writer,
        )?;
        let db = self.open_db()?;
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);

        let mut loop_entry = match loop_repo.get(loop_id) {
            Ok(entry) => entry,
            Err(err) if err.to_string().contains("not found") => {
                return Err(format!("loop {loop_id} not found"));
            }
            Err(err) => return Err(err.to_string()),
        };

        loop_entry.state = forge_db::loop_repository::LoopState::Running;
        let mut metadata = loop_entry.metadata.take().unwrap_or_default();
        metadata.insert("runner_owner".to_string(), json!(spawn_result.owner));
        metadata.insert(
            "runner_instance_id".to_string(),
            json!(spawn_result.instance_id),
        );
        if let Some(pid) = spawn_result.pid {
            metadata.insert("pid".to_string(), json!(pid));
        }
        loop_entry.metadata = Some(metadata);

        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| format!("start loop {loop_id}: {err}"))
    }
}

fn resolve_database_path() -> PathBuf {
    crate::runtime_paths::resolve_database_path()
}

fn normalize_repo_filter(value: &str) -> Result<String, String> {
    let path = std::path::Path::new(value);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|err| format!("failed to resolve current directory: {err}"))?
            .join(path)
    };
    Ok(abs.to_string_lossy().into_owned())
}

fn resolve_pool_ref(
    repo: &forge_db::pool_repository::PoolRepository<'_>,
    value: &str,
) -> Result<String, String> {
    if let Ok(pool) = repo.get_by_name(value) {
        return Ok(pool.id);
    }
    if let Ok(pool) = repo.get(value) {
        return Ok(pool.id);
    }
    Err(format!("pool {value:?} not found"))
}

fn resolve_profile_ref(
    repo: &forge_db::profile_repository::ProfileRepository<'_>,
    value: &str,
) -> Result<String, String> {
    if let Ok(profile) = repo.get_by_name(value) {
        return Ok(profile.id);
    }
    if let Ok(profile) = repo.get(value) {
        return Ok(profile.id);
    }
    Err(format!("profile {value:?} not found"))
}

pub fn run_for_test(args: &[&str], backend: &mut dyn UpBackend) -> CommandOutput {
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
    backend: &mut dyn UpBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout, stderr) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
    count: usize,
    name: String,
    name_prefix: String,
    pool: String,
    profile: String,
    prompt: String,
    prompt_msg: String,
    interval_seconds: i64,
    initial_wait_seconds: i64,
    max_runtime_seconds: i64,
    max_iterations: i32,
    tags: Vec<String>,
    spawn_owner: String,
    config_path: String,
    stop_config: StopConfig,
}

#[derive(Debug, Serialize)]
struct UpResultEntry {
    name: String,
    short_id: String,
}

fn execute(
    args: &[String],
    backend: &mut dyn UpBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    let spawn_options = SpawnOptions {
        config_path: parsed.config_path.clone(),
        suppress_warning: parsed.quiet || parsed.json || parsed.jsonl,
        ..Default::default()
    };

    let existing_names_list = backend.list_loop_names()?;
    let mut existing_names: BTreeSet<String> = existing_names_list.into_iter().collect();

    let mut created: Vec<LoopRecord> = Vec::new();

    for index in 0..parsed.count {
        let name = if !parsed.name.is_empty() {
            parsed.name.clone()
        } else if !parsed.name_prefix.is_empty() {
            format!("{}-{}", parsed.name_prefix, index + 1)
        } else {
            generate_loop_name(&existing_names)
        };

        if existing_names.contains(&name) {
            return Err(format!("loop name \"{name}\" already exists"));
        }
        existing_names.insert(name.clone());

        let spec = LoopCreateSpec {
            name,
            repo: String::new(),
            pool: parsed.pool.clone(),
            profile: parsed.profile.clone(),
            prompt: parsed.prompt.clone(),
            prompt_msg: parsed.prompt_msg.clone(),
            interval_seconds: parsed.interval_seconds,
            max_runtime_seconds: parsed.max_runtime_seconds,
            max_iterations: parsed.max_iterations,
            tags: parsed.tags.clone(),
            stop_config: parsed.stop_config.clone(),
        };

        let record = backend.create_loop(&spec)?;
        if parsed.initial_wait_seconds > 0 {
            backend.enqueue_item(
                &record.id,
                QueueItem::Pause {
                    duration_seconds: parsed.initial_wait_seconds,
                    reason: "initial wait".to_string(),
                },
            )?;
        }
        backend.start_loop(&record.id, &parsed.spawn_owner, &spawn_options, stderr)?;
        created.push(record);
    }

    if parsed.json || parsed.jsonl {
        let entries: Vec<UpResultEntry> = created
            .iter()
            .map(|record| UpResultEntry {
                name: record.name.clone(),
                short_id: record.short_id.clone(),
            })
            .collect();
        write_serialized(stdout, &entries, parsed.jsonl)?;
        return Ok(());
    }

    if parsed.quiet {
        return Ok(());
    }

    for record in &created {
        writeln!(
            stdout,
            "Loop \"{}\" started ({})",
            record.name, record.short_id
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "up") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
    let mut count = 1usize;
    let mut name = String::new();
    let mut name_prefix = String::new();
    let mut pool = String::new();
    let mut profile = String::new();
    let mut prompt = String::new();
    let mut prompt_msg = String::new();
    let mut interval_raw = String::new();
    let mut initial_wait_raw = String::new();
    let mut max_runtime_raw = String::new();
    let mut max_iterations = 0i32;
    let mut tags_raw = String::new();
    let mut spawn_owner = "auto".to_string();
    let mut spawn_owner_explicit = false;
    let mut config_path = String::new();

    let mut quant_cmd = String::new();
    let mut quant_every = 1i32;
    let mut quant_when = "before".to_string();
    let mut quant_decision = "stop".to_string();
    let mut quant_exit_codes_raw = String::new();
    let mut quant_exit_invert = false;
    let mut quant_stdout_mode = "any".to_string();
    let mut quant_stderr_mode = "any".to_string();
    let mut quant_stdout_regex = String::new();
    let mut quant_stderr_regex = String::new();
    let mut quant_timeout_raw = String::new();

    let mut qual_every = 0i32;
    let mut qual_prompt = String::new();
    let mut qual_prompt_msg = String::new();
    let mut qual_on_invalid = "continue".to_string();

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => return Err(HELP_TEXT.to_string()),
            "--json" => {
                json = true;
                index += 1;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
            }
            "--quiet" => {
                quiet = true;
                index += 1;
            }
            "--count" | "-n" => {
                let value = take_value(args, index, token)?;
                let parsed = parse_i32(token, &value)?;
                if parsed < 1 {
                    return Err("--count must be at least 1".to_string());
                }
                count = parsed as usize;
                index += 2;
            }
            "--name" => {
                name = take_value(args, index, "--name")?;
                index += 2;
            }
            "--name-prefix" => {
                name_prefix = take_value(args, index, "--name-prefix")?;
                index += 2;
            }
            "--pool" => {
                pool = take_value(args, index, "--pool")?;
                index += 2;
            }
            "--profile" => {
                profile = take_value(args, index, "--profile")?;
                index += 2;
            }
            "--prompt" => {
                prompt = take_value(args, index, "--prompt")?;
                index += 2;
            }
            "--prompt-msg" => {
                prompt_msg = take_value(args, index, "--prompt-msg")?;
                index += 2;
            }
            "--interval" => {
                interval_raw = take_value(args, index, "--interval")?;
                index += 2;
            }
            "--initial-wait" => {
                initial_wait_raw = take_value(args, index, "--initial-wait")?;
                index += 2;
            }
            "--max-runtime" | "-r" => {
                max_runtime_raw = take_value(args, index, token)?;
                index += 2;
            }
            "--max-iterations" | "-i" => {
                max_iterations = parse_i32(token, &take_value(args, index, token)?)?;
                index += 2;
            }
            "--tags" => {
                tags_raw = take_value(args, index, "--tags")?;
                index += 2;
            }
            "--spawn-owner" => {
                spawn_owner = take_value(args, index, "--spawn-owner")?;
                spawn_owner_explicit = true;
                index += 2;
            }
            "--config" => {
                config_path = take_value(args, index, "--config")?;
                index += 2;
            }
            "--quantitative-stop-cmd" => {
                quant_cmd = take_value(args, index, "--quantitative-stop-cmd")?;
                index += 2;
            }
            "--quantitative-stop-every" => {
                quant_every = parse_i32(
                    "--quantitative-stop-every",
                    &take_value(args, index, "--quantitative-stop-every")?,
                )?;
                index += 2;
            }
            "--quantitative-stop-when" => {
                quant_when = take_value(args, index, "--quantitative-stop-when")?;
                index += 2;
            }
            "--quantitative-stop-decision" => {
                quant_decision = take_value(args, index, "--quantitative-stop-decision")?;
                index += 2;
            }
            "--quantitative-stop-exit-codes" => {
                quant_exit_codes_raw = take_value(args, index, "--quantitative-stop-exit-codes")?;
                index += 2;
            }
            "--quantitative-stop-exit-invert" => {
                quant_exit_invert = true;
                index += 1;
            }
            "--quantitative-stop-stdout" => {
                quant_stdout_mode = take_value(args, index, "--quantitative-stop-stdout")?;
                index += 2;
            }
            "--quantitative-stop-stderr" => {
                quant_stderr_mode = take_value(args, index, "--quantitative-stop-stderr")?;
                index += 2;
            }
            "--quantitative-stop-stdout-regex" => {
                quant_stdout_regex = take_value(args, index, "--quantitative-stop-stdout-regex")?;
                index += 2;
            }
            "--quantitative-stop-stderr-regex" => {
                quant_stderr_regex = take_value(args, index, "--quantitative-stop-stderr-regex")?;
                index += 2;
            }
            "--quantitative-stop-timeout" => {
                quant_timeout_raw = take_value(args, index, "--quantitative-stop-timeout")?;
                index += 2;
            }
            "--qualitative-stop-every" => {
                qual_every = parse_i32(
                    "--qualitative-stop-every",
                    &take_value(args, index, "--qualitative-stop-every")?,
                )?;
                index += 2;
            }
            "--qualitative-stop-prompt" => {
                qual_prompt = take_value(args, index, "--qualitative-stop-prompt")?;
                index += 2;
            }
            "--qualitative-stop-prompt-msg" => {
                qual_prompt_msg = take_value(args, index, "--qualitative-stop-prompt-msg")?;
                index += 2;
            }
            "--qualitative-stop-on-invalid" => {
                qual_on_invalid = take_value(args, index, "--qualitative-stop-on-invalid")?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for up: '{flag}'"));
            }
            value => {
                return Err(format!(
                    "error: up accepts no positional arguments, got '{value}'"
                ));
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }
    if !name.is_empty() && count > 1 {
        return Err("--name requires --count=1".to_string());
    }
    if !pool.is_empty() && !profile.is_empty() {
        return Err("use either --pool or --profile, not both".to_string());
    }
    if max_iterations < 0 {
        return Err("max iterations must be >= 0".to_string());
    }
    if !matches!(spawn_owner.as_str(), "local" | "daemon" | "auto") {
        return Err(format!(
            "invalid --spawn-owner \"{spawn_owner}\" (valid: local|daemon|auto)"
        ));
    }
    // Go parity: implicit auto (default, not explicitly provided) resolves to local.
    if !spawn_owner_explicit && spawn_owner == "auto" {
        spawn_owner = "local".to_string();
    }

    let interval_seconds = parse_duration_seconds(&interval_raw, 0, "interval")?;
    let initial_wait_seconds = parse_duration_seconds(&initial_wait_raw, 0, "initial wait")?;
    let max_runtime_seconds = parse_duration_seconds(&max_runtime_raw, 0, "max runtime")?;

    let mut stop_config = StopConfig::default();

    if !quant_cmd.trim().is_empty() {
        if quant_every <= 0 {
            return Err("quantitative stop every must be > 0".to_string());
        }

        let quant_when_normalized = normalize_choice(
            &quant_when,
            &["before", "after", "both"],
            "quantitative stop when",
        )?;
        let quant_decision_normalized = normalize_choice(
            &quant_decision,
            &["stop", "continue"],
            "quantitative stop decision",
        )?;
        let quant_stdout_mode_normalized = normalize_choice(
            &quant_stdout_mode,
            &["any", "empty", "nonempty"],
            "quantitative stop stdout mode",
        )?;
        let quant_stderr_mode_normalized = normalize_choice(
            &quant_stderr_mode,
            &["any", "empty", "nonempty"],
            "quantitative stop stderr mode",
        )?;

        let mut exit_codes = parse_csv_i32(&quant_exit_codes_raw)?;
        let timeout_seconds =
            parse_duration_seconds(&quant_timeout_raw, 0, "quantitative stop timeout")?;
        let no_criteria = exit_codes.is_empty()
            && quant_stdout_mode_normalized == "any"
            && quant_stderr_mode_normalized == "any"
            && quant_stdout_regex.trim().is_empty()
            && quant_stderr_regex.trim().is_empty();
        if no_criteria {
            exit_codes.push(0);
        }

        stop_config.quant = Some(QuantStopConfig {
            cmd: quant_cmd,
            every_n: quant_every,
            when: quant_when_normalized,
            decision: quant_decision_normalized,
            exit_codes,
            exit_invert: quant_exit_invert,
            stdout_mode: quant_stdout_mode_normalized,
            stderr_mode: quant_stderr_mode_normalized,
            stdout_regex: quant_stdout_regex,
            stderr_regex: quant_stderr_regex,
            timeout_seconds,
        });
    }

    if qual_every > 0 || !qual_prompt.trim().is_empty() || !qual_prompt_msg.trim().is_empty() {
        if qual_every <= 0 {
            return Err("qualitative stop every must be > 0".to_string());
        }
        if !qual_prompt.trim().is_empty() && !qual_prompt_msg.trim().is_empty() {
            return Err(
                "use either --qualitative-stop-prompt or --qualitative-stop-prompt-msg, not both"
                    .to_string(),
            );
        }

        let prompt_value = if !qual_prompt_msg.trim().is_empty() {
            qual_prompt_msg.trim().to_string()
        } else if !qual_prompt.trim().is_empty() {
            qual_prompt.trim().to_string()
        } else {
            return Err(
                "qualitative stop requires --qualitative-stop-prompt or --qualitative-stop-prompt-msg"
                    .to_string(),
            );
        };

        let on_invalid = normalize_choice(
            &qual_on_invalid,
            &["stop", "continue"],
            "qualitative stop on invalid",
        )?;

        stop_config.qual = Some(QualStopConfig {
            every_n: qual_every,
            prompt: prompt_value,
            is_prompt_path: !qual_prompt.trim().is_empty(),
            on_invalid,
        });
    }

    Ok(ParsedArgs {
        json,
        jsonl,
        quiet,
        count,
        name,
        name_prefix,
        pool,
        profile,
        prompt,
        prompt_msg,
        interval_seconds,
        initial_wait_seconds,
        max_runtime_seconds,
        max_iterations,
        tags: parse_tags(&tags_raw),
        spawn_owner,
        config_path,
        stop_config,
    })
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

fn parse_i32(flag: &str, value: &str) -> Result<i32, String> {
    value
        .parse::<i32>()
        .map_err(|_| format!("error: invalid value for {flag}: '{value}'"))
}

fn parse_duration_seconds(
    raw: &str,
    default_seconds: i64,
    field_name: &str,
) -> Result<i64, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(default_seconds);
    }

    if trimmed.starts_with('-') {
        return Err(format!("{field_name} must be >= 0"));
    }

    let split = trimmed
        .char_indices()
        .find(|(_, ch)| !ch.is_ascii_digit())
        .map_or((trimmed, "s"), |(pos, _)| {
            let (number, unit) = trimmed.split_at(pos);
            (number, unit)
        });

    let number = split
        .0
        .parse::<i64>()
        .map_err(|_| format!("invalid {field_name} duration: {trimmed}"))?;

    let multiplier = match split.1 {
        "" | "s" => 1,
        "m" => 60,
        "h" => 3_600,
        _ => return Err(format!("invalid {field_name} duration: {trimmed}")),
    };

    number
        .checked_mul(multiplier)
        .ok_or_else(|| format!("invalid {field_name} duration: {trimmed}"))
}

fn parse_csv_i32(raw: &str) -> Result<Vec<i32>, String> {
    let mut values = Vec::new();
    for chunk in raw.split(',') {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        values.push(
            trimmed
                .parse::<i32>()
                .map_err(|_| format!("invalid integer value: {trimmed}"))?,
        );
    }
    Ok(values)
}

fn normalize_choice(raw: &str, allowed: &[&str], label: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if allowed.iter().any(|entry| *entry == normalized) {
        return Ok(normalized);
    }
    Err(format!("{label} must be one of {}", allowed.join("|")))
}

fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn generate_loop_name(existing: &BTreeSet<String>) -> String {
    for index in 1.. {
        let candidate = format!("loop-{index}");
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    "loop-fallback".to_string()
}

fn write_serialized(
    stdout: &mut dyn Write,
    payload: &impl Serialize,
    jsonl: bool,
) -> Result<(), String> {
    if jsonl {
        serde_json::to_writer(&mut *stdout, payload).map_err(|err| err.to_string())?;
    } else {
        serde_json::to_writer_pretty(&mut *stdout, payload).map_err(|err| err.to_string())?;
    }
    writeln!(stdout).map_err(|err| err.to_string())?;
    Ok(())
}

const HELP_TEXT: &str = "\
Start loop(s) for a repo

Usage:
  forge up [flags]

Flags:
  -n, --count int                          number of loops to start (default 1)
      --name string                        loop name (single loop, requires --count=1)
      --name-prefix string                 loop name prefix
      --pool string                        pool name or ID
      --profile string                     profile name or ID
      --prompt string                      base prompt path or prompt name
      --prompt-msg string                  base prompt content for each iteration
      --config string                      config file path passed to spawned loop runner
      --interval string                    sleep interval (e.g., 30s, 2m)
      --initial-wait string                wait before first iteration (e.g., 30s, 2m)
  -r, --max-runtime string                 max runtime before stopping (e.g., 30m, 2h)
  -i, --max-iterations int                 max iterations before stopping (0 = no limit)
      --tags string                        comma-separated tags
      --spawn-owner string                 loop runner owner (local|daemon|auto)
      --quantitative-stop-cmd string       quantitative stop: command to execute
      --quantitative-stop-every int        quantitative stop: evaluate every N iterations
      --quantitative-stop-when string      quantitative stop: when to evaluate (before|after|both)
      --quantitative-stop-decision string  quantitative stop: decision on match (stop|continue)
      --quantitative-stop-exit-codes string quantitative stop: match exit codes (comma-separated)
      --quantitative-stop-exit-invert      quantitative stop: invert exit code match
      --quantitative-stop-stdout string    quantitative stop: stdout mode (any|empty|nonempty)
      --quantitative-stop-stderr string    quantitative stop: stderr mode (any|empty|nonempty)
      --quantitative-stop-stdout-regex string quantitative stop: stdout regex
      --quantitative-stop-stderr-regex string quantitative stop: stderr regex
      --quantitative-stop-timeout string   quantitative stop: command timeout (e.g. 10s)
      --qualitative-stop-every int         qualitative stop: run every N main iterations
      --qualitative-stop-prompt string     qualitative stop: prompt path or name
      --qualitative-stop-prompt-msg string qualitative stop: inline prompt content
      --qualitative-stop-on-invalid string qualitative stop: on invalid judge output (stop|continue)";

#[cfg(test)]
mod tests {
    use super::{
        parse_args, run_for_test, InMemoryUpBackend, QualStopConfig, QuantStopConfig, QueueItem,
        SqliteUpBackend,
    };
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_count_must_be_at_least_1() {
        let args = vec!["up".to_string(), "--count".to_string(), "0".to_string()];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "--count must be at least 1");
    }

    #[test]
    fn parse_name_requires_count_1() {
        let args = vec![
            "up".to_string(),
            "--name".to_string(),
            "my-loop".to_string(),
            "--count".to_string(),
            "2".to_string(),
        ];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "--name requires --count=1");
    }

    #[test]
    fn parse_rejects_pool_and_profile() {
        let args = vec![
            "up".to_string(),
            "--pool".to_string(),
            "default".to_string(),
            "--profile".to_string(),
            "codex".to_string(),
        ];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "use either --pool or --profile, not both");
    }

    #[test]
    fn parse_rejects_unknown_flag() {
        let args = vec!["up".to_string(), "--bogus".to_string()];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "error: unknown argument for up: '--bogus'");
    }

    #[test]
    fn parse_rejects_positional_args() {
        let args = vec!["up".to_string(), "extra".to_string()];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            "error: up accepts no positional arguments, got 'extra'"
        );
    }

    #[test]
    fn up_creates_single_loop_with_name() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(&["up", "--name", "oracle-loop", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
        assert_eq!(backend.created_specs.len(), 1);
        assert_eq!(backend.created_specs[0].name, "oracle-loop");
        assert_eq!(backend.starts.len(), 1);
    }

    #[test]
    fn up_creates_multiple_with_prefix() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(
            &["up", "--count", "3", "--name-prefix", "batch"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(out.stdout, "Loop \"batch-1\" started (s001)\nLoop \"batch-2\" started (s002)\nLoop \"batch-3\" started (s003)\n");
        assert_eq!(backend.created_specs.len(), 3);
        assert_eq!(backend.created_specs[0].name, "batch-1");
        assert_eq!(backend.created_specs[1].name, "batch-2");
        assert_eq!(backend.created_specs[2].name, "batch-3");
    }

    #[test]
    fn up_generates_names_when_not_specified() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(&["up", "--count", "2", "--quiet"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.created_specs[0].name, "loop-1");
        assert_eq!(backend.created_specs[1].name, "loop-2");
    }

    #[test]
    fn up_rejects_duplicate_name() {
        let mut backend = InMemoryUpBackend::with_existing_names(vec!["oracle-loop".to_string()]);
        let out = run_for_test(&["up", "--name", "oracle-loop"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "loop name \"oracle-loop\" already exists\n");
    }

    #[test]
    fn up_enqueues_initial_wait() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(
            &[
                "up",
                "--name",
                "test-loop",
                "--initial-wait",
                "60s",
                "--quiet",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.queued.len(), 1);
        assert_eq!(
            backend.queued[0].1,
            QueueItem::Pause {
                duration_seconds: 60,
                reason: "initial wait".to_string(),
            }
        );
    }

    #[test]
    fn up_json_output() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(&["up", "--name", "oracle-loop", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(
            out.stdout,
            "[\n  {\n    \"name\": \"oracle-loop\",\n    \"short_id\": \"s001\"\n  }\n]\n"
        );
    }

    #[test]
    fn up_jsonl_output() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(&["up", "--name", "oracle-loop", "--jsonl"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(
            out.stdout,
            "[{\"name\":\"oracle-loop\",\"short_id\":\"s001\"}]\n"
        );
    }

    #[test]
    fn up_quiet_suppresses_output() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(&["up", "--quiet"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
        assert_eq!(backend.created_specs.len(), 1);
    }

    #[test]
    fn up_human_output() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(&["up", "--name", "oracle-loop"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "Loop \"oracle-loop\" started (s001)\n");
    }

    #[test]
    fn up_passes_profile_to_spec() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(
            &["up", "--name", "test", "--profile", "codex", "--quiet"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.created_specs[0].profile, "codex");
    }

    #[test]
    fn up_passes_pool_to_spec() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(
            &["up", "--name", "test", "--pool", "default", "--quiet"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.created_specs[0].pool, "default");
    }

    #[test]
    fn up_passes_tags_to_spec() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(
            &[
                "up",
                "--name",
                "test",
                "--tags",
                "team-a, team-b",
                "--quiet",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(
            backend.created_specs[0].tags,
            vec!["team-a".to_string(), "team-b".to_string()]
        );
    }

    #[test]
    fn up_passes_spawn_owner() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(
            &["up", "--name", "test", "--spawn-owner", "daemon", "--quiet"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.starts[0].1, "daemon");
    }

    #[test]
    fn up_rejects_invalid_spawn_owner() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(&["up", "--spawn-owner", "weird"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(
            out.stderr,
            "invalid --spawn-owner \"weird\" (valid: local|daemon|auto)\n"
        );
    }

    #[test]
    fn up_passes_interval() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(
            &["up", "--name", "test", "--interval", "2m", "--quiet"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.created_specs[0].interval_seconds, 120);
    }

    #[test]
    fn up_passes_max_runtime() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(
            &["up", "--name", "test", "-r", "1h", "--quiet"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.created_specs[0].max_runtime_seconds, 3600);
    }

    #[test]
    fn up_passes_max_iterations() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(
            &["up", "--name", "test", "-i", "10", "--quiet"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.created_specs[0].max_iterations, 10);
    }

    #[test]
    fn parse_quant_stop_defaults_exit_0_when_no_criteria() {
        let args = vec![
            "up".to_string(),
            "--quantitative-stop-cmd".to_string(),
            "echo ok".to_string(),
        ];
        let parsed = match parse_args(&args) {
            Ok(parsed) => parsed,
            Err(err) => panic!("expected parse ok: {err}"),
        };
        let quant = match parsed.stop_config.quant {
            Some(quant) => quant,
            None => panic!("expected quantitative stop config"),
        };
        assert_eq!(quant.exit_codes, vec![0]);
        assert_eq!(quant.cmd, "echo ok");
    }

    #[test]
    fn parse_quant_stop_with_all_options() {
        let args: Vec<String> = vec![
            "up",
            "--quantitative-stop-cmd",
            "check.sh",
            "--quantitative-stop-every",
            "2",
            "--quantitative-stop-when",
            "after",
            "--quantitative-stop-decision",
            "continue",
            "--quantitative-stop-exit-codes",
            "0,1",
            "--quantitative-stop-exit-invert",
            "--quantitative-stop-stdout",
            "nonempty",
            "--quantitative-stop-stderr",
            "empty",
            "--quantitative-stop-stdout-regex",
            "ok.*",
            "--quantitative-stop-stderr-regex",
            "fail.*",
            "--quantitative-stop-timeout",
            "10s",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let parsed = match parse_args(&args) {
            Ok(parsed) => parsed,
            Err(err) => panic!("expected parse ok: {err}"),
        };
        let quant = match parsed.stop_config.quant {
            Some(quant) => quant,
            None => panic!("expected quantitative stop config"),
        };
        assert_eq!(
            quant,
            QuantStopConfig {
                cmd: "check.sh".to_string(),
                every_n: 2,
                when: "after".to_string(),
                decision: "continue".to_string(),
                exit_codes: vec![0, 1],
                exit_invert: true,
                stdout_mode: "nonempty".to_string(),
                stderr_mode: "empty".to_string(),
                stdout_regex: "ok.*".to_string(),
                stderr_regex: "fail.*".to_string(),
                timeout_seconds: 10,
            }
        );
    }

    #[test]
    fn parse_qual_stop_with_prompt_msg() {
        let args: Vec<String> = vec![
            "up",
            "--qualitative-stop-every",
            "3",
            "--qualitative-stop-prompt-msg",
            "judge this",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let parsed = match parse_args(&args) {
            Ok(parsed) => parsed,
            Err(err) => panic!("expected parse ok: {err}"),
        };
        let qual = match parsed.stop_config.qual {
            Some(qual) => qual,
            None => panic!("expected qualitative stop config"),
        };
        assert_eq!(
            qual,
            QualStopConfig {
                every_n: 3,
                prompt: "judge this".to_string(),
                is_prompt_path: false,
                on_invalid: "continue".to_string(),
            }
        );
    }

    #[test]
    fn parse_qual_stop_with_prompt_path() {
        let args: Vec<String> = vec![
            "up",
            "--qualitative-stop-every",
            "5",
            "--qualitative-stop-prompt",
            "judge.md",
            "--qualitative-stop-on-invalid",
            "stop",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let parsed = match parse_args(&args) {
            Ok(parsed) => parsed,
            Err(err) => panic!("expected parse ok: {err}"),
        };
        let qual = match parsed.stop_config.qual {
            Some(qual) => qual,
            None => panic!("expected qualitative stop config"),
        };
        assert_eq!(
            qual,
            QualStopConfig {
                every_n: 5,
                prompt: "judge.md".to_string(),
                is_prompt_path: true,
                on_invalid: "stop".to_string(),
            }
        );
    }

    #[test]
    fn parse_qual_stop_rejects_both_prompt_types() {
        let args: Vec<String> = vec![
            "up",
            "--qualitative-stop-every",
            "1",
            "--qualitative-stop-prompt",
            "file.md",
            "--qualitative-stop-prompt-msg",
            "inline",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            "use either --qualitative-stop-prompt or --qualitative-stop-prompt-msg, not both"
        );
    }

    #[test]
    fn up_help_returns_help_text() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(&["up", "--help"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.starts_with("Start loop(s) for a repo"));
    }

    #[test]
    fn up_json_and_jsonl_mutually_exclusive() {
        let args = vec![
            "up".to_string(),
            "--json".to_string(),
            "--jsonl".to_string(),
        ];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "error: --json and --jsonl cannot be used together");
    }

    #[test]
    fn up_default_spawn_owner_resolves_to_local() {
        // Go parity: implicit auto (not explicitly provided) resolves to local.
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(&["up", "--quiet"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.starts[0].1, "local");
    }

    #[test]
    fn up_explicit_auto_stays_auto() {
        let mut backend = InMemoryUpBackend::default();
        let out = run_for_test(&["up", "--quiet", "--spawn-owner", "auto"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.starts[0].1, "auto");
    }

    #[test]
    fn up_sqlite_backend_creates_loop_and_sets_running_metadata() {
        let _cwd_guard = current_dir_guard();
        let db_path = temp_db_path("sqlite-create-start");
        let db = init_db(&db_path);

        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);
        let mut profile = forge_db::profile_repository::Profile {
            name: "codex".to_string(),
            command_template: "codex exec".to_string(),
            harness: "codex".to_string(),
            ..Default::default()
        };
        profile_repo
            .create(&mut profile)
            .unwrap_or_else(|err| panic!("create profile: {err}"));

        let mut backend = SqliteUpBackend::new(db_path.clone());
        let out = run_for_test(
            &[
                "up",
                "--name",
                "sqlite-loop",
                "--profile",
                "codex",
                "--quiet",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("reopen db: {err}"));
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let created = loop_repo
            .get_by_name("sqlite-loop")
            .unwrap_or_else(|err| panic!("load loop: {err}"));

        assert_eq!(created.state, forge_db::loop_repository::LoopState::Running);
        assert_eq!(created.profile_id, profile.id);
        assert_eq!(created.repo_path, cwd_string());

        let metadata = created
            .metadata
            .unwrap_or_else(|| panic!("missing loop metadata"));
        // Go parity: implicit auto (not explicitly provided) resolves to local.
        assert_eq!(metadata.get("runner_owner"), Some(&json!("local")));
        assert!(
            metadata
                .get("runner_instance_id")
                .and_then(|value| value.as_str())
                .is_some_and(|value| !value.trim().is_empty()),
            "missing runner_instance_id metadata"
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn up_sqlite_backend_resolves_registered_prompt_name_before_path_fallback() {
        let db_path = temp_db_path("sqlite-prompt-name");
        let _db = init_db(&db_path);
        let repo_path = temp_repo_path("prompt-name");
        let prompts_dir = repo_path.join(".forge").join("prompts");
        std::fs::create_dir_all(&prompts_dir)
            .unwrap_or_else(|err| panic!("create prompts dir: {err}"));
        std::fs::write(prompts_dir.join("po-design.md"), "# prompt")
            .unwrap_or_else(|err| panic!("write prompt file: {err}"));

        with_current_dir(&repo_path, || {
            let mut backend = SqliteUpBackend::new(db_path.clone());
            let out = run_for_test(
                &[
                    "up",
                    "--name",
                    "prompt-loop",
                    "--prompt",
                    "po-design",
                    "--quiet",
                ],
                &mut backend,
            );
            assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

            let db = forge_db::Db::open(forge_db::Config::new(&db_path))
                .unwrap_or_else(|err| panic!("reopen db: {err}"));
            let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
            let created = loop_repo
                .get_by_name("prompt-loop")
                .unwrap_or_else(|err| panic!("load loop: {err}"));
            assert_eq!(created.base_prompt_path, ".forge/prompts/po-design.md");
        });

        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_dir_all(repo_path);
    }

    #[test]
    fn up_sqlite_backend_enqueues_initial_wait_pause_item() {
        let db_path = temp_db_path("sqlite-initial-wait");
        let _db = init_db(&db_path);

        let mut backend = SqliteUpBackend::new(db_path.clone());
        let out = run_for_test(
            &[
                "up",
                "--name",
                "sqlite-loop",
                "--initial-wait",
                "45s",
                "--quiet",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("reopen db: {err}"));
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let created = loop_repo
            .get_by_name("sqlite-loop")
            .unwrap_or_else(|err| panic!("load loop: {err}"));
        let queued = queue_repo
            .list(&created.id)
            .unwrap_or_else(|err| panic!("list queue: {err}"));
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].item_type, "pause");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&queued[0].payload)
                .unwrap_or_else(|err| panic!("parse payload: {err}")),
            json!({
                "duration_seconds": 45,
                "reason": "initial wait"
            })
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn up_sqlite_backend_resolves_pool_and_profile_by_name() {
        let db_path = temp_db_path("sqlite-pool-profile");
        let db = init_db(&db_path);

        let pool_repo = forge_db::pool_repository::PoolRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let mut pool = forge_db::pool_repository::Pool {
            name: "default".to_string(),
            strategy: "round_robin".to_string(),
            ..Default::default()
        };
        pool_repo
            .create(&mut pool)
            .unwrap_or_else(|err| panic!("create pool: {err}"));

        let mut profile = forge_db::profile_repository::Profile {
            name: "codex".to_string(),
            command_template: "codex exec".to_string(),
            harness: "codex".to_string(),
            ..Default::default()
        };
        profile_repo
            .create(&mut profile)
            .unwrap_or_else(|err| panic!("create profile: {err}"));

        let mut backend = SqliteUpBackend::new(db_path.clone());
        let out = run_for_test(
            &["up", "--name", "pool-loop", "--pool", "default", "--quiet"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

        let out = run_for_test(
            &[
                "up",
                "--name",
                "profile-loop",
                "--profile",
                "codex",
                "--quiet",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("reopen db: {err}"));
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);

        let with_pool = loop_repo
            .get_by_name("pool-loop")
            .unwrap_or_else(|err| panic!("load pool-loop: {err}"));
        assert_eq!(with_pool.pool_id, pool.id);

        let with_profile = loop_repo
            .get_by_name("profile-loop")
            .unwrap_or_else(|err| panic!("load profile-loop: {err}"));
        assert_eq!(with_profile.profile_id, profile.id);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn up_sqlite_backend_daemon_owner_sets_metadata() {
        let db_path = temp_db_path("sqlite-daemon-owner");
        let _db = init_db(&db_path);

        let mut backend = SqliteUpBackend::new(db_path.clone());
        let out = run_for_test(
            &[
                "up",
                "--name",
                "daemon-loop",
                "--spawn-owner",
                "daemon",
                "--quiet",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("reopen db: {err}"));
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let created = loop_repo
            .get_by_name("daemon-loop")
            .unwrap_or_else(|err| panic!("load loop: {err}"));
        let metadata = created
            .metadata
            .unwrap_or_else(|| panic!("missing loop metadata"));

        assert_eq!(metadata.get("runner_owner"), Some(&json!("daemon")));
        assert!(
            metadata
                .get("runner_instance_id")
                .and_then(|v| v.as_str())
                .is_some_and(|v| !v.trim().is_empty()),
            "daemon owner should set runner_instance_id"
        );
        // daemon-spawned loops do not set a local pid
        assert!(
            !metadata.contains_key("pid"),
            "daemon owner should not set pid metadata"
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn up_sqlite_backend_local_owner_sets_pid_metadata() {
        let db_path = temp_db_path("sqlite-local-owner");
        let _db = init_db(&db_path);

        let mut backend = SqliteUpBackend::new(db_path.clone());
        let out = run_for_test(
            &[
                "up",
                "--name",
                "local-loop",
                "--spawn-owner",
                "local",
                "--quiet",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("reopen db: {err}"));
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let created = loop_repo
            .get_by_name("local-loop")
            .unwrap_or_else(|err| panic!("load loop: {err}"));
        let metadata = created
            .metadata
            .unwrap_or_else(|| panic!("missing loop metadata"));

        assert_eq!(metadata.get("runner_owner"), Some(&json!("local")));
        assert!(
            metadata
                .get("runner_instance_id")
                .and_then(|v| v.as_str())
                .is_some_and(|v| !v.trim().is_empty()),
            "local owner should set runner_instance_id"
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn up_sqlite_backend_persists_stop_config_metadata() {
        let db_path = temp_db_path("sqlite-stop-config");
        let _db = init_db(&db_path);

        let mut backend = SqliteUpBackend::new(db_path.clone());
        let out = run_for_test(
            &[
                "up",
                "--name",
                "stop-loop",
                "--quantitative-stop-cmd",
                "echo ok",
                "--quantitative-stop-every",
                "2",
                "--qualitative-stop-every",
                "3",
                "--qualitative-stop-prompt-msg",
                "judge this",
                "--quiet",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("reopen db: {err}"));
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let created = loop_repo
            .get_by_name("stop-loop")
            .unwrap_or_else(|err| panic!("load loop: {err}"));
        let metadata = created
            .metadata
            .unwrap_or_else(|| panic!("missing loop metadata"));
        let stop_config = metadata
            .get("stop_config")
            .unwrap_or_else(|| panic!("missing stop_config metadata"));
        assert_eq!(stop_config["quant"]["cmd"], json!("echo ok"));
        assert_eq!(stop_config["quant"]["every_n"], json!(2));
        assert_eq!(stop_config["qual"]["every_n"], json!(3));
        assert_eq!(stop_config["qual"]["prompt"], json!("judge this"));

        let _ = std::fs::remove_file(db_path);
    }

    fn temp_db_path(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-cli-up-{tag}-{nanos}-{}-{suffix}.sqlite",
            std::process::id(),
        ))
    }

    fn temp_repo_path(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-cli-up-repo-{tag}-{nanos}-{}-{suffix}",
            std::process::id(),
        ))
    }

    fn with_current_dir<F>(dir: &std::path::Path, f: F)
    where
        F: FnOnce(),
    {
        let _cwd_guard = current_dir_guard();
        let previous = std::env::current_dir().unwrap_or_else(|err| panic!("resolve cwd: {err}"));
        std::env::set_current_dir(dir).unwrap_or_else(|err| panic!("set cwd: {err}"));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        std::env::set_current_dir(previous).unwrap_or_else(|err| panic!("restore cwd: {err}"));
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }

    fn current_dir_guard() -> MutexGuard<'static, ()> {
        static CURRENT_DIR_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        match CURRENT_DIR_LOCK.get_or_init(|| Mutex::new(())).lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn init_db(db_path: &PathBuf) -> forge_db::Db {
        let mut db = forge_db::Db::open(forge_db::Config::new(db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));
        db
    }

    fn cwd_string() -> String {
        std::env::current_dir()
            .unwrap_or_else(|err| panic!("resolve cwd: {err}"))
            .to_string_lossy()
            .into_owned()
    }
}
