use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::path::PathBuf;

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
    pub name: String,
    pub repo: String,
    pub pool: String,
    pub profile: String,
    pub created_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopSelector {
    pub repo: String,
    pub pool: String,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueItem {
    StopGraceful,
    KillNow,
    Pause {
        duration_seconds: i64,
        reason: String,
    },
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

pub trait ScaleBackend {
    fn select_loops(&self, selector: &LoopSelector) -> Result<Vec<LoopRecord>, String>;
    fn enqueue_item(&mut self, loop_id: &str, item: QueueItem) -> Result<(), String>;
    fn create_loop(&mut self, spec: &LoopCreateSpec) -> Result<LoopRecord, String>;
    fn start_loop(
        &mut self,
        loop_id: &str,
        spawn_owner: &str,
        spawn_options: &SpawnOptions,
        warning_writer: &mut dyn Write,
    ) -> Result<(), String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryScaleBackend {
    loops: Vec<LoopRecord>,
    next_id: u64,
    next_created_seq: u64,
    pub queue_by_loop: HashMap<String, Vec<QueueItem>>,
    pub created_specs: Vec<LoopCreateSpec>,
    pub starts: Vec<(String, String)>,
}

impl InMemoryScaleBackend {
    pub fn with_loops(mut loops: Vec<LoopRecord>) -> Self {
        loops.sort_by_key(|entry| entry.created_seq);
        let next_created_seq = loops.last().map(|entry| entry.created_seq).unwrap_or(0);
        Self {
            next_id: loops.len() as u64,
            next_created_seq,
            loops,
            queue_by_loop: HashMap::new(),
            created_specs: Vec::new(),
            starts: Vec::new(),
        }
    }

    pub fn loops(&self) -> &[LoopRecord] {
        &self.loops
    }
}

impl ScaleBackend for InMemoryScaleBackend {
    fn select_loops(&self, selector: &LoopSelector) -> Result<Vec<LoopRecord>, String> {
        Ok(self
            .loops
            .iter()
            .filter(|entry| {
                (selector.repo.is_empty() || entry.repo == selector.repo)
                    && (selector.pool.is_empty() || entry.pool == selector.pool)
                    && (selector.profile.is_empty() || entry.profile == selector.profile)
            })
            .cloned()
            .collect())
    }

    fn enqueue_item(&mut self, loop_id: &str, item: QueueItem) -> Result<(), String> {
        if !self.loops.iter().any(|entry| entry.id == loop_id) {
            return Err(format!("loop {loop_id} not found"));
        }
        self.queue_by_loop
            .entry(loop_id.to_string())
            .or_default()
            .push(item);
        Ok(())
    }

    fn create_loop(&mut self, spec: &LoopCreateSpec) -> Result<LoopRecord, String> {
        if self.loops.iter().any(|entry| entry.name == spec.name) {
            return Err(format!("loop name \"{}\" already exists", spec.name));
        }

        self.next_id += 1;
        self.next_created_seq += 1;

        let entry = LoopRecord {
            id: format!("loop-{:03}", self.next_id),
            name: spec.name.clone(),
            repo: spec.repo.clone(),
            pool: spec.pool.clone(),
            profile: spec.profile.clone(),
            created_seq: self.next_created_seq,
        };
        self.created_specs.push(spec.clone());
        self.loops.push(entry.clone());
        Ok(entry)
    }

    fn start_loop(
        &mut self,
        loop_id: &str,
        spawn_owner: &str,
        _spawn_options: &SpawnOptions,
        _warning_writer: &mut dyn Write,
    ) -> Result<(), String> {
        if !self.loops.iter().any(|entry| entry.id == loop_id) {
            return Err(format!("loop {loop_id} not found"));
        }
        self.starts
            .push((loop_id.to_string(), spawn_owner.to_string()));
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SqliteScaleBackend {
    db_path: PathBuf,
}

impl SqliteScaleBackend {
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

impl ScaleBackend for SqliteScaleBackend {
    fn select_loops(&self, selector: &LoopSelector) -> Result<Vec<LoopRecord>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = self.open_db()?;
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let pool_repo = forge_db::pool_repository::PoolRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let loops = match loop_repo.list() {
            Ok(loops) => loops,
            Err(err) if err.to_string().contains("no such table: loops") => return Ok(Vec::new()),
            Err(err) => return Err(err.to_string()),
        };

        let repo_filter = if selector.repo.is_empty() {
            String::new()
        } else {
            normalize_repo_filter(&selector.repo)?
        };
        let pool_filter = if selector.pool.is_empty() {
            String::new()
        } else {
            resolve_pool_ref(&pool_repo, &selector.pool)?
        };
        let profile_filter = if selector.profile.is_empty() {
            String::new()
        } else {
            resolve_profile_ref(&profile_repo, &selector.profile)?
        };

        let mut out = Vec::new();
        for entry in loops {
            if !repo_filter.is_empty() && entry.repo_path != repo_filter {
                continue;
            }
            if !pool_filter.is_empty() && entry.pool_id != pool_filter {
                continue;
            }
            if !profile_filter.is_empty() && entry.profile_id != profile_filter {
                continue;
            }

            let pool_name = if entry.pool_id.is_empty() {
                String::new()
            } else {
                pool_repo
                    .get(&entry.pool_id)
                    .map(|pool| pool.name)
                    .unwrap_or(entry.pool_id.clone())
            };
            let profile_name = if entry.profile_id.is_empty() {
                String::new()
            } else {
                profile_repo
                    .get(&entry.profile_id)
                    .map(|profile| profile.name)
                    .unwrap_or(entry.profile_id.clone())
            };

            let created_seq = (out.len() + 1) as u64;
            out.push(LoopRecord {
                id: entry.id,
                name: entry.name,
                repo: entry.repo_path,
                pool: pool_name,
                profile: profile_name,
                created_seq,
            });
        }
        Ok(out)
    }

    fn enqueue_item(&mut self, loop_id: &str, item: QueueItem) -> Result<(), String> {
        let db = self.open_db()?;
        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);

        let (item_type, payload) = match item {
            QueueItem::StopGraceful => ("stop_graceful".to_string(), "{}".to_string()),
            QueueItem::KillNow => ("kill_now".to_string(), "{}".to_string()),
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
            let mut metadata = HashMap::new();
            metadata.insert(
                "stop_config".to_string(),
                serde_json::to_value(&spec.stop_config)
                    .map_err(|err| format!("serialize stop config: {err}"))?,
            );
            Some(metadata)
        } else {
            None
        };

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: spec.name.clone(),
            repo_path: repo_path.clone(),
            base_prompt_path: spec.prompt.clone(),
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
            name: loop_entry.name,
            repo: repo_path,
            pool: spec.pool.clone(),
            profile: spec.profile.clone(),
            created_seq: 0,
        })
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
    count: usize,
    selector: LoopSelector,
    prompt: String,
    prompt_msg: String,
    interval_seconds: i64,
    initial_wait_seconds: i64,
    max_runtime_seconds: i64,
    max_iterations: i32,
    tags: Vec<String>,
    name_prefix: String,
    kill: bool,
    spawn_owner: String,
    config_path: String,
    stop_config: StopConfig,
}

#[derive(Debug, Serialize)]
struct ScaleResult {
    target: usize,
    current: usize,
}

pub fn run_for_test(args: &[&str], backend: &mut dyn ScaleBackend) -> CommandOutput {
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
    backend: &mut dyn ScaleBackend,
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

fn execute(
    args: &[String],
    backend: &mut dyn ScaleBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    let spawn_options = SpawnOptions {
        config_path: parsed.config_path.clone(),
        suppress_warning: parsed.quiet || parsed.json || parsed.jsonl,
        ..Default::default()
    };
    let mut loops = backend.select_loops(&parsed.selector)?;
    loops.sort_by_key(|entry| entry.created_seq);

    let current = loops.len();

    if current > parsed.count {
        for loop_entry in loops.iter().skip(parsed.count) {
            let item = if parsed.kill {
                QueueItem::KillNow
            } else {
                QueueItem::StopGraceful
            };
            backend.enqueue_item(&loop_entry.id, item)?;
        }
    } else if current < parsed.count {
        let to_create = parsed.count - current;
        let mut existing_names: BTreeSet<String> =
            loops.iter().map(|entry| entry.name.clone()).collect();

        for index in 0..to_create {
            let name = if parsed.name_prefix.is_empty() {
                generate_loop_name(&existing_names)
            } else {
                format!("{}-{}", parsed.name_prefix, index + 1)
            };

            if existing_names.contains(&name) {
                return Err(format!("loop name \"{name}\" already exists"));
            }
            existing_names.insert(name.clone());

            let spec = LoopCreateSpec {
                name,
                repo: parsed.selector.repo.clone(),
                pool: parsed.selector.pool.clone(),
                profile: parsed.selector.profile.clone(),
                prompt: parsed.prompt.clone(),
                prompt_msg: parsed.prompt_msg.clone(),
                interval_seconds: parsed.interval_seconds,
                max_runtime_seconds: parsed.max_runtime_seconds,
                max_iterations: parsed.max_iterations,
                tags: parsed.tags.clone(),
                stop_config: parsed.stop_config.clone(),
            };

            let created = backend.create_loop(&spec)?;
            if parsed.initial_wait_seconds > 0 {
                backend.enqueue_item(
                    &created.id,
                    QueueItem::Pause {
                        duration_seconds: parsed.initial_wait_seconds,
                        reason: "initial wait".to_string(),
                    },
                )?;
            }
            backend.start_loop(&created.id, &parsed.spawn_owner, &spawn_options, stderr)?;
        }
    }

    if parsed.json || parsed.jsonl {
        let payload = ScaleResult {
            target: parsed.count,
            current,
        };
        write_serialized(stdout, &payload, parsed.jsonl)?;
        return Ok(());
    }

    if parsed.quiet {
        return Ok(());
    }

    writeln!(stdout, "Scaled loops to {}", parsed.count).map_err(|err| err.to_string())?;
    Ok(())
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|arg| arg == "scale") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;

    let mut count = 1usize;
    let mut selector = LoopSelector::default();
    let mut prompt = String::new();
    let mut prompt_msg = String::new();
    let mut interval_raw = String::new();
    let mut initial_wait_raw = String::new();
    let mut max_runtime_raw = String::new();
    let mut max_iterations = 0i32;
    let mut tags_raw = String::new();
    let mut name_prefix = String::new();
    let mut kill = false;
    let mut spawn_owner = "auto".to_string();
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
                if parsed < 0 {
                    return Err("--count must be >= 0".to_string());
                }
                count = parsed as usize;
                index += 2;
            }
            "--pool" => {
                selector.pool = take_value(args, index, "--pool")?;
                index += 2;
            }
            "--profile" => {
                selector.profile = take_value(args, index, "--profile")?;
                index += 2;
            }
            "--repo" => {
                selector.repo = take_value(args, index, "--repo")?;
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
            "--name-prefix" => {
                name_prefix = take_value(args, index, "--name-prefix")?;
                index += 2;
            }
            "--kill" => {
                kill = true;
                index += 1;
            }
            "--spawn-owner" => {
                spawn_owner = take_value(args, index, "--spawn-owner")?;
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
                return Err(format!("error: unknown argument for scale: '{flag}'"));
            }
            value => {
                return Err(format!(
                    "error: scale accepts no positional arguments, got '{value}'"
                ));
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }
    if !selector.pool.is_empty() && !selector.profile.is_empty() {
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
        selector,
        prompt,
        prompt_msg,
        interval_seconds,
        initial_wait_seconds,
        max_runtime_seconds,
        max_iterations,
        tags: parse_tags(&tags_raw),
        name_prefix,
        kill,
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
        .map_or((trimmed, "s"), |(index, _)| {
            let (number, unit) = trimmed.split_at(index);
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

fn normalize_repo_filter(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("repo filter cannot be empty".to_string());
    }
    let path = std::path::Path::new(trimmed);
    if path.is_absolute() {
        return Ok(trimmed.to_string());
    }
    let cwd = std::env::current_dir().map_err(|err| format!("resolve cwd: {err}"))?;
    Ok(cwd.join(trimmed).to_string_lossy().into_owned())
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

const HELP_TEXT: &str = "\
Scale loops to a target count

Usage:
  forge scale [flags]

Flags:
  -n, --count int            target loop count
      --pool string          pool name or ID
      --profile string       profile name or ID
      --prompt string        base prompt path or name
      --prompt-msg string    base prompt content
      --config string        config file path passed to spawned loop runner
      --initial-wait string  wait before first iteration for new loops
      --kill                 kill extra loops instead of stopping
      --spawn-owner string   loop runner owner (local|daemon|auto)";

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        parse_args, run_for_test, InMemoryScaleBackend, QueueItem, ScaleBackend, SqliteScaleBackend,
    };

    #[test]
    fn parse_rejects_pool_and_profile_combo() {
        let args = vec![
            "scale".to_string(),
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
    fn parse_rejects_negative_count() {
        let args = vec!["scale".to_string(), "--count".to_string(), "-1".to_string()];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "--count must be >= 0");
    }

    #[test]
    fn parse_defaults_quant_exit_codes_when_cmd_has_no_criteria() {
        let args = vec![
            "scale".to_string(),
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
    }

    #[test]
    fn parse_rejects_invalid_quantitative_settings() {
        let bad_every = vec![
            "scale".to_string(),
            "--quantitative-stop-cmd".to_string(),
            "echo ok".to_string(),
            "--quantitative-stop-every".to_string(),
            "0".to_string(),
        ];
        let err = match parse_args(&bad_every) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "quantitative stop every must be > 0");

        let bad_when = vec![
            "scale".to_string(),
            "--quantitative-stop-cmd".to_string(),
            "echo ok".to_string(),
            "--quantitative-stop-when".to_string(),
            "later".to_string(),
        ];
        let err = match parse_args(&bad_when) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            "quantitative stop when must be one of before|after|both"
        );

        let bad_stdout = vec![
            "scale".to_string(),
            "--quantitative-stop-cmd".to_string(),
            "echo ok".to_string(),
            "--quantitative-stop-stdout".to_string(),
            "weird".to_string(),
        ];
        let err = match parse_args(&bad_stdout) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            "quantitative stop stdout mode must be one of any|empty|nonempty"
        );

        let bad_exit_codes = vec![
            "scale".to_string(),
            "--quantitative-stop-cmd".to_string(),
            "echo ok".to_string(),
            "--quantitative-stop-exit-codes".to_string(),
            "0,abc".to_string(),
        ];
        let err = match parse_args(&bad_exit_codes) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "invalid integer value: abc");

        let bad_timeout = vec![
            "scale".to_string(),
            "--quantitative-stop-cmd".to_string(),
            "echo ok".to_string(),
            "--quantitative-stop-timeout".to_string(),
            "-5s".to_string(),
        ];
        let err = match parse_args(&bad_timeout) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "quantitative stop timeout must be >= 0");
    }

    #[test]
    fn parse_rejects_invalid_qualitative_settings() {
        let bad_every = vec![
            "scale".to_string(),
            "--qualitative-stop-prompt".to_string(),
            "judge.md".to_string(),
        ];
        let err = match parse_args(&bad_every) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "qualitative stop every must be > 0");

        let bad_pair = vec![
            "scale".to_string(),
            "--qualitative-stop-every".to_string(),
            "1".to_string(),
            "--qualitative-stop-prompt".to_string(),
            "judge.md".to_string(),
            "--qualitative-stop-prompt-msg".to_string(),
            "inline".to_string(),
        ];
        let err = match parse_args(&bad_pair) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            "use either --qualitative-stop-prompt or --qualitative-stop-prompt-msg, not both"
        );

        let missing_prompt = vec![
            "scale".to_string(),
            "--qualitative-stop-every".to_string(),
            "2".to_string(),
        ];
        let err = match parse_args(&missing_prompt) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            "qualitative stop requires --qualitative-stop-prompt or --qualitative-stop-prompt-msg"
        );

        let bad_on_invalid = vec![
            "scale".to_string(),
            "--qualitative-stop-every".to_string(),
            "1".to_string(),
            "--qualitative-stop-prompt".to_string(),
            "judge.md".to_string(),
            "--qualitative-stop-on-invalid".to_string(),
            "ignore".to_string(),
        ];
        let err = match parse_args(&bad_on_invalid) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            "qualitative stop on invalid must be one of stop|continue"
        );
    }

    #[test]
    fn parse_qualitative_prompt_variants_set_path_flag() {
        let prompt_path_args = vec![
            "scale".to_string(),
            "--qualitative-stop-every".to_string(),
            "2".to_string(),
            "--qualitative-stop-prompt".to_string(),
            "judge.md".to_string(),
        ];
        let parsed = match parse_args(&prompt_path_args) {
            Ok(parsed) => parsed,
            Err(err) => panic!("expected parse ok: {err}"),
        };
        let qual = match parsed.stop_config.qual {
            Some(qual) => qual,
            None => panic!("expected qualitative stop config"),
        };
        assert_eq!(qual.prompt, "judge.md");
        assert!(qual.is_prompt_path);
        assert_eq!(qual.on_invalid, "continue");

        let inline_args = vec![
            "scale".to_string(),
            "--qualitative-stop-every".to_string(),
            "3".to_string(),
            "--qualitative-stop-prompt-msg".to_string(),
            "judge inline".to_string(),
            "--qualitative-stop-on-invalid".to_string(),
            "stop".to_string(),
        ];
        let parsed = match parse_args(&inline_args) {
            Ok(parsed) => parsed,
            Err(err) => panic!("expected parse ok: {err}"),
        };
        let qual = match parsed.stop_config.qual {
            Some(qual) => qual,
            None => panic!("expected qualitative stop config"),
        };
        assert_eq!(qual.prompt, "judge inline");
        assert!(!qual.is_prompt_path);
        assert_eq!(qual.on_invalid, "stop");
    }

    #[test]
    fn scale_down_enqueues_stop_by_default() {
        let loops = vec![
            loop_record("loop-001", "alpha", 1),
            loop_record("loop-002", "beta", 2),
            loop_record("loop-003", "gamma", 3),
        ];
        let mut backend = InMemoryScaleBackend::with_loops(loops);

        let out = run_for_test(&["scale", "--count", "1", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stderr, "");
        assert_eq!(out.stdout, "{\n  \"target\": 1,\n  \"current\": 3\n}\n");

        let queued_beta = backend
            .queue_by_loop
            .get("loop-002")
            .cloned()
            .unwrap_or_default();
        let queued_gamma = backend
            .queue_by_loop
            .get("loop-003")
            .cloned()
            .unwrap_or_default();
        assert_eq!(queued_beta, vec![QueueItem::StopGraceful]);
        assert_eq!(queued_gamma, vec![QueueItem::StopGraceful]);
    }

    #[test]
    fn scale_up_creates_loops_and_initial_wait() {
        let loops = vec![loop_record("loop-001", "existing", 1)];
        let mut backend = InMemoryScaleBackend::with_loops(loops);

        let out = run_for_test(
            &[
                "scale",
                "--count",
                "3",
                "--name-prefix",
                "oracle",
                "--initial-wait",
                "90s",
                "--spawn-owner",
                "local",
            ],
            &mut backend,
        );

        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stderr, "");
        assert_eq!(out.stdout, "Scaled loops to 3\n");

        assert_eq!(backend.created_specs.len(), 2);
        assert_eq!(backend.created_specs[0].name, "oracle-1");
        assert_eq!(backend.created_specs[1].name, "oracle-2");

        assert_eq!(backend.starts.len(), 2);
        assert_eq!(backend.starts[0].1, "local");
        assert_eq!(backend.starts[1].1, "local");

        for (loop_id, _) in &backend.starts {
            let queued = backend
                .queue_by_loop
                .get(loop_id)
                .cloned()
                .unwrap_or_default();
            assert_eq!(
                queued,
                vec![QueueItem::Pause {
                    duration_seconds: 90,
                    reason: "initial wait".to_string()
                }]
            );
        }
    }

    #[test]
    fn scale_rejects_invalid_spawn_owner() {
        let mut backend = InMemoryScaleBackend::default();
        let out = run_for_test(
            &["scale", "--count", "1", "--spawn-owner", "weird"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "invalid --spawn-owner \"weird\" (valid: local|daemon|auto)\n"
        );
    }

    #[test]
    fn scale_backend_trait_is_object_safe() {
        let mut backend = InMemoryScaleBackend::default();
        let as_trait: &mut dyn ScaleBackend = &mut backend;
        let out = run_for_test(&["scale", "--count", "0", "--quiet"], as_trait);
        assert_eq!(out.exit_code, 0);
    }

    #[test]
    fn scale_sqlite_down_enqueues_stop_items() {
        let db_path = temp_db_path("sqlite-down");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let mut loop_one = forge_db::loop_repository::Loop {
            name: "alpha".to_string(),
            repo_path: "/tmp/alpha".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_one)
            .unwrap_or_else(|err| panic!("create loop one: {err}"));
        let mut loop_two = forge_db::loop_repository::Loop {
            name: "beta".to_string(),
            repo_path: "/tmp/beta".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_two)
            .unwrap_or_else(|err| panic!("create loop two: {err}"));

        let mut backend = SqliteScaleBackend::new(db_path.clone());
        let out = run_for_test(&["scale", "--count", "1", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert_eq!(out.stdout, "{\n  \"target\": 1,\n  \"current\": 2\n}\n");

        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let one_items = queue_repo
            .list(&loop_one.id)
            .unwrap_or_else(|err| panic!("list loop one queue: {err}"));
        let two_items = queue_repo
            .list(&loop_two.id)
            .unwrap_or_else(|err| panic!("list loop two queue: {err}"));

        assert!(one_items.is_empty());
        assert_eq!(two_items.len(), 1);
        assert_eq!(two_items[0].item_type, "stop_graceful");

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn scale_sqlite_up_creates_loop_wait_and_start_metadata() {
        let db_path = temp_db_path("sqlite-up");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let mut existing = forge_db::loop_repository::Loop {
            name: "existing".to_string(),
            repo_path: "/tmp/existing".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            ..Default::default()
        };
        loop_repo
            .create(&mut existing)
            .unwrap_or_else(|err| panic!("create existing loop: {err}"));

        let mut backend = SqliteScaleBackend::new(db_path.clone());
        let out = run_for_test(
            &[
                "scale",
                "--count",
                "2",
                "--name-prefix",
                "scaled",
                "--initial-wait",
                "30s",
                "--spawn-owner",
                "daemon",
                "--json",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert_eq!(out.stdout, "{\n  \"target\": 2,\n  \"current\": 1\n}\n");

        let loops = loop_repo
            .list()
            .unwrap_or_else(|err| panic!("list loops: {err}"));
        assert_eq!(loops.len(), 2);
        let created = loops
            .iter()
            .find(|entry| entry.id != existing.id)
            .unwrap_or_else(|| panic!("expected created loop"));
        assert_eq!(created.name, "scaled-1");
        assert_eq!(created.state, forge_db::loop_repository::LoopState::Running);

        let metadata = created
            .metadata
            .clone()
            .unwrap_or_else(|| panic!("expected metadata with runner fields"));
        assert_eq!(
            metadata
                .get("runner_owner")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
            "daemon"
        );
        assert!(metadata
            .get("runner_instance_id")
            .and_then(|value| value.as_str())
            .is_some());

        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let queued = queue_repo
            .list(&created.id)
            .unwrap_or_else(|err| panic!("list created queue: {err}"));
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].item_type, "pause");
        let payload: serde_json::Value = serde_json::from_str(&queued[0].payload)
            .unwrap_or_else(|err| panic!("parse pause payload: {err}"));
        assert_eq!(payload["duration_seconds"], 30);
        assert_eq!(payload["reason"], "initial wait");

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
            "forge-cli-scale-{tag}-{nanos}-{}-{suffix}.sqlite",
            std::process::id()
        ))
    }

    fn loop_record(id: &str, name: &str, created_seq: u64) -> super::LoopRecord {
        super::LoopRecord {
            id: id.to_string(),
            name: name.to_string(),
            repo: "/repo".to_string(),
            pool: "".to_string(),
            profile: "".to_string(),
            created_seq,
        }
    }
}
