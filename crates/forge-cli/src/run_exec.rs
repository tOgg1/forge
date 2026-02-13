use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use chrono::{DateTime, Utc};
use forge_loop::harness_wrapper::{
    build_execution_plan, HarnessKind, ProfileSpec, PromptMode as HarnessPromptMode,
};
use forge_loop::ledger_writer::{
    append_ledger_entry, ensure_ledger_file, LoopLedgerRecord, LoopRunRecord, ProfileRecord,
};
use forge_loop::log_io::{LoopLogger, DEFAULT_OUTPUT_TAIL_LINES};
use forge_loop::prompt_composition::{
    append_operator_messages, resolve_base_prompt, resolve_override_prompt, LoopPromptConfig,
    OperatorMessage, PromptOverridePayload,
};
use forge_loop::queue_interactions::{should_inject_qualitative_stop, QueueInteractionPlan};
use forge_loop::stop_rules;
use serde::Deserialize;
use serde_json::Value;

const DEFAULT_WAIT_SECONDS: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IterationControl {
    Stop,
    Sleep(Duration),
}

#[derive(Debug, Clone, Default, Deserialize)]
struct StoredStopConfig {
    #[serde(default)]
    quant: Option<StoredQuantStopConfig>,
    #[serde(default)]
    qual: Option<StoredQualStopConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct StoredQuantStopConfig {
    #[serde(default)]
    cmd: String,
    #[serde(default)]
    every_n: i32,
    #[serde(default)]
    when: String,
    #[serde(default)]
    decision: String,
    #[serde(default)]
    exit_codes: Vec<i32>,
    #[serde(default)]
    exit_invert: bool,
    #[serde(default)]
    stdout_mode: String,
    #[serde(default)]
    stderr_mode: String,
    #[serde(default)]
    stdout_regex: String,
    #[serde(default)]
    stderr_regex: String,
    #[serde(default)]
    timeout_seconds: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct StoredQualStopConfig {
    #[serde(default)]
    every_n: i32,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    is_prompt_path: bool,
    #[serde(default)]
    on_invalid: String,
}

pub fn run_single_iteration(db_path: &Path, loop_id: &str) -> Result<(), String> {
    let db = forge_db::Db::open(forge_db::Config::new(db_path))
        .map_err(|err| format!("open database {}: {err}", db_path.display()))?;
    let _ = run_iteration(&db, loop_id, true)?;
    Ok(())
}

pub fn run_loop_until_stop(db_path: &Path, loop_id: &str) -> Result<(), String> {
    let db = forge_db::Db::open(forge_db::Config::new(db_path))
        .map_err(|err| format!("open database {}: {err}", db_path.display()))?;
    loop {
        match run_iteration(&db, loop_id, false)? {
            IterationControl::Stop => return Ok(()),
            IterationControl::Sleep(duration) => {
                if duration > Duration::ZERO {
                    std::thread::sleep(duration);
                }
            }
        }
    }
}

fn run_iteration(
    db: &forge_db::Db,
    loop_id: &str,
    single_run: bool,
) -> Result<IterationControl, String> {
    let loop_repo = forge_db::loop_repository::LoopRepository::new(db);
    let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(db);
    let run_repo = forge_db::loop_run_repository::LoopRunRepository::new(db);
    let mut loop_entry = loop_repo
        .get(loop_id)
        .map_err(|err| format!("load loop {loop_id}: {err}"))?;
    ensure_loop_paths(&mut loop_entry, &loop_repo)?;

    let mut logger = LoopLogger::new(Path::new(&loop_entry.log_path))
        .map_err(|err| format!("open loop log {}: {err}", loop_entry.log_path))?;
    let _ = logger.write_line("loop started");

    let now = Utc::now();
    let mut metadata = loop_entry.metadata.take().unwrap_or_default();
    metadata.insert(
        "pid".to_string(),
        Value::from(i64::from(std::process::id())),
    );
    if loop_entry.max_runtime_seconds > 0 && metadata_datetime(&metadata, "started_at").is_none() {
        metadata.insert("started_at".to_string(), Value::String(now.to_rfc3339()));
    }
    if metadata_i64(&metadata, "iteration_count").is_none() {
        metadata.insert("iteration_count".to_string(), Value::from(0));
    }
    let stop_config = parse_stop_config(&metadata)?;
    let iteration_index = metadata_i64(&metadata, "iteration_count")
        .unwrap_or(0)
        .saturating_add(1) as i32;

    if let Some(reason) = limit_reason(&loop_entry, &metadata, now) {
        let _ = logger.write_line(&reason);
        loop_entry.state = forge_db::loop_repository::LoopState::Stopped;
        loop_entry.last_error = reason;
        loop_entry.metadata = Some(metadata);
        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| format!("update stopped loop {}: {err}", loop_entry.id))?;
        return Ok(IterationControl::Stop);
    }

    loop_entry.state = forge_db::loop_repository::LoopState::Running;
    loop_entry.metadata = Some(metadata);
    loop_repo
        .update(&mut loop_entry)
        .map_err(|err| format!("set loop {} running: {err}", loop_entry.id))?;

    let queue_items = queue_repo
        .list(&loop_entry.id)
        .map_err(|err| format!("list queue {}: {err}", loop_entry.id))?;
    let plan = build_queue_plan(&queue_items)?;

    if !plan.stop_ids.is_empty() {
        mark_queue_completed(&queue_repo, &plan.stop_ids)?;
        let _ = logger.write_line("graceful stop requested");
        loop_entry.state = forge_db::loop_repository::LoopState::Stopped;
        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| format!("persist stop state {}: {err}", loop_entry.id))?;
        return Ok(IterationControl::Stop);
    }
    if !plan.kill_ids.is_empty() {
        mark_queue_completed(&queue_repo, &plan.kill_ids)?;
        let _ = logger.write_line("kill requested");
        loop_entry.state = forge_db::loop_repository::LoopState::Stopped;
        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| format!("persist kill state {}: {err}", loop_entry.id))?;
        return Ok(IterationControl::Stop);
    }

    if let Some(duration) = plan.pause_duration {
        if plan.pause_before_run {
            let _ = logger.write_line(&format!("pause for {:?}", duration));
            loop_entry.state = forge_db::loop_repository::LoopState::Sleeping;
            loop_repo
                .update(&mut loop_entry)
                .map_err(|err| format!("persist pause state {}: {err}", loop_entry.id))?;
            if duration > Duration::ZERO {
                std::thread::sleep(duration);
            }
            mark_queue_completed(&queue_repo, &plan.pause_ids)?;
        }
    }

    if let Some(stop_reason) = stop_config
        .as_ref()
        .and_then(|cfg| cfg.quant.as_ref())
        .and_then(|cfg| {
            evaluate_quantitative_stop(
                &loop_entry.repo_path,
                cfg,
                iteration_index,
                false,
                &mut logger,
            )
        })
    {
        loop_entry.state = forge_db::loop_repository::LoopState::Stopped;
        loop_entry.last_error = stop_reason;
        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| format!("persist quantitative stop {}: {err}", loop_entry.id))?;
        return Ok(IterationControl::Stop);
    }

    let (profile, wait_until) = match select_profile(db, &loop_entry, now) {
        Ok(result) => result,
        Err(err) => {
            let _ = logger.write_line(&err);
            loop_entry.state = forge_db::loop_repository::LoopState::Error;
            loop_entry.last_error = err.clone();
            loop_repo.update(&mut loop_entry).map_err(|update_err| {
                format!("persist selection error {}: {update_err}", loop_entry.id)
            })?;
            return Err(err);
        }
    };
    if let Some(wait_until) = wait_until {
        let wait_reason = format!(
            "waiting for profile availability until {}",
            wait_until.to_rfc3339()
        );
        let _ = logger.write_line(&wait_reason);
        let mut metadata = loop_entry.metadata.take().unwrap_or_default();
        metadata.insert(
            "wait_until".to_string(),
            Value::String(wait_until.to_rfc3339()),
        );
        loop_entry.metadata = Some(metadata);
        loop_entry.state = forge_db::loop_repository::LoopState::Waiting;
        loop_entry.last_error = wait_reason;
        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| format!("persist waiting state {}: {err}", loop_entry.id))?;
        if single_run {
            return Ok(IterationControl::Stop);
        }
        let sleep_for = wait_until
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap_or_else(|_| Duration::from_secs(DEFAULT_WAIT_SECONDS as u64));
        return Ok(IterationControl::Sleep(sleep_for));
    }
    let profile = profile.ok_or_else(|| "profile unavailable".to_string())?;

    let mut metadata = loop_entry.metadata.take().unwrap_or_default();
    metadata.remove("wait_until");
    loop_entry.metadata = Some(metadata);

    let mut prompt = resolve_base_prompt(&LoopPromptConfig {
        repo_path: loop_entry.repo_path.clone(),
        base_prompt_msg: loop_entry.base_prompt_msg.clone(),
        base_prompt_path: loop_entry.base_prompt_path.clone(),
    })?;

    if let Some(payload) = &plan.override_prompt {
        prompt = resolve_override_prompt(
            &loop_entry.repo_path,
            &PromptOverridePayload {
                prompt: payload.prompt.clone(),
                is_path: payload.is_path,
            },
        )?;
    }

    let prompt_content = append_operator_messages(&prompt.content, &plan.messages);

    let mut run_record = forge_db::loop_run_repository::LoopRun {
        loop_id: loop_entry.id.clone(),
        profile_id: profile.id.clone(),
        status: forge_db::loop_run_repository::LoopRunStatus::Running,
        prompt_source: prompt.source.clone(),
        prompt_path: prompt.path.clone(),
        prompt_override: prompt.is_override,
        metadata: Some(HashMap::from([(
            "kind".to_string(),
            Value::String("main".to_string()),
        )])),
        ..Default::default()
    };
    run_repo
        .create(&mut run_record)
        .map_err(|err| format!("create loop run {}: {err}", loop_entry.id))?;

    let prepared = prepare_prompt(
        &loop_entry,
        &run_record.id,
        &profile,
        &prompt,
        &prompt_content,
        !plan.messages.is_empty(),
    )?;

    let _ = logger.write_line(&format!(
        "run {} start (profile={})",
        run_record.id, profile.name
    ));
    let exec_result = execute_profile(
        &profile,
        &loop_entry,
        &prepared.prompt_path,
        &prepared.prompt_content,
        &mut logger,
    );

    run_record.status = if exec_result.err_text.is_empty() && exec_result.exit_code == 0 {
        forge_db::loop_run_repository::LoopRunStatus::Success
    } else {
        forge_db::loop_run_repository::LoopRunStatus::Error
    };
    run_record.exit_code = Some(exec_result.exit_code);
    run_record.output_tail = exec_result.output_tail.clone();
    run_repo
        .finish(&mut run_record)
        .map_err(|err| format!("finish run {}: {err}", run_record.id))?;

    let mut metadata = loop_entry.metadata.take().unwrap_or_default();
    let next_iteration = metadata_i64(&metadata, "iteration_count").unwrap_or(0) + 1;
    metadata.insert("iteration_count".to_string(), Value::from(next_iteration));

    let mut post_run_stop_reason = stop_config
        .as_ref()
        .and_then(|cfg| cfg.quant.as_ref())
        .and_then(|cfg| {
            evaluate_quantitative_stop(
                &loop_entry.repo_path,
                cfg,
                next_iteration as i32,
                true,
                &mut logger,
            )
        });
    if post_run_stop_reason.is_none() {
        post_run_stop_reason = stop_config
            .as_ref()
            .and_then(|cfg| cfg.qual.as_ref())
            .and_then(|cfg| {
                evaluate_qualitative_stop(
                    &loop_entry.repo_path,
                    cfg,
                    next_iteration as i32,
                    single_run,
                    &plan,
                    &mut logger,
                )
            });
    }

    loop_entry.metadata = Some(metadata);
    loop_entry.last_run_at = run_record.finished_at.clone();
    loop_entry.last_exit_code = Some(i64::from(exec_result.exit_code));
    if let Some(reason) = post_run_stop_reason.as_ref() {
        loop_entry.last_error = reason.clone();
        loop_entry.state = forge_db::loop_repository::LoopState::Stopped;
    } else {
        loop_entry.last_error = exec_result.err_text.clone();
        loop_entry.state = forge_db::loop_repository::LoopState::Sleeping;
    }
    loop_repo
        .update(&mut loop_entry)
        .map_err(|err| format!("persist run result {}: {err}", loop_entry.id))?;

    mark_queue_completed(&queue_repo, &plan.consume_ids)?;

    let ledger_loop = LoopLedgerRecord {
        id: loop_entry.id.clone(),
        name: loop_entry.name.clone(),
        repo_path: loop_entry.repo_path.clone(),
        ledger_path: loop_entry.ledger_path.clone(),
    };
    let ledger_run = LoopRunRecord {
        id: run_record.id.clone(),
        status: run_record.status.as_str().to_string(),
        prompt_source: run_record.prompt_source.clone(),
        prompt_path: run_record.prompt_path.clone(),
        prompt_override: run_record.prompt_override,
        started_at: parse_rfc3339_utc(&run_record.started_at).unwrap_or(now),
        finished_at: run_record
            .finished_at
            .as_deref()
            .and_then(parse_rfc3339_utc),
        exit_code: run_record.exit_code,
    };
    let ledger_profile = ProfileRecord {
        name: profile.name.clone(),
        harness: profile.harness.clone(),
        auth_kind: profile.auth_kind.clone(),
    };
    let _ = append_ledger_entry(
        &ledger_loop,
        &ledger_run,
        &ledger_profile,
        &exec_result.output_tail,
        DEFAULT_OUTPUT_TAIL_LINES,
    );

    if let Some(duration) = plan.pause_duration {
        if !plan.pause_before_run {
            if post_run_stop_reason.is_some() {
                let _ = logger.write_line("skip pause because loop is stopping");
            } else {
                let _ = logger.write_line(&format!("pause for {:?}", duration));
                if duration > Duration::ZERO {
                    std::thread::sleep(duration);
                }
            }
            mark_queue_completed(&queue_repo, &plan.pause_ids)?;
        }
    }

    if post_run_stop_reason.is_some() {
        return Ok(IterationControl::Stop);
    }

    if single_run {
        loop_entry.state = forge_db::loop_repository::LoopState::Stopped;
        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| format!("persist single-run stop {}: {err}", loop_entry.id))?;
        return Ok(IterationControl::Stop);
    }

    let empty_meta = HashMap::new();
    if let Some(reason) = limit_reason(
        &loop_entry,
        loop_entry.metadata.as_ref().unwrap_or(&empty_meta),
        Utc::now(),
    ) {
        let _ = logger.write_line(&reason);
        loop_entry.state = forge_db::loop_repository::LoopState::Stopped;
        loop_entry.last_error = reason;
        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| format!("persist loop limit stop {}: {err}", loop_entry.id))?;
        return Ok(IterationControl::Stop);
    }

    let interval = if loop_entry.interval_seconds <= 0 {
        Duration::ZERO
    } else {
        Duration::from_secs(loop_entry.interval_seconds as u64)
    };
    Ok(IterationControl::Sleep(interval))
}

fn parse_stop_config(
    metadata: &HashMap<String, Value>,
) -> Result<Option<StoredStopConfig>, String> {
    let Some(value) = metadata.get("stop_config") else {
        return Ok(None);
    };
    let parsed: StoredStopConfig = serde_json::from_value(value.clone())
        .map_err(|err| format!("decode stop_config: {err}"))?;
    Ok(Some(parsed))
}

fn evaluate_quantitative_stop(
    repo_path: &str,
    cfg: &StoredQuantStopConfig,
    iteration_index: i32,
    after_run: bool,
    logger: &mut LoopLogger,
) -> Option<String> {
    if !stop_rules::quant_should_evaluate(&cfg.when, cfg.every_n, iteration_index, after_run) {
        return None;
    }

    let timeout = if cfg.timeout_seconds <= 0 {
        Duration::ZERO
    } else {
        Duration::from_secs(cfg.timeout_seconds as u64)
    };
    let command_result = stop_rules::run_quant_command(Path::new(repo_path), &cfg.cmd, timeout);
    let runtime_cfg = stop_rules::QuantStopConfig {
        cmd: cfg.cmd.clone(),
        every_n: cfg.every_n,
        when: cfg.when.clone(),
        decision: cfg.decision.clone(),
        exit_codes: cfg.exit_codes.clone(),
        exit_invert: cfg.exit_invert,
        stdout_mode: cfg.stdout_mode.clone(),
        stderr_mode: cfg.stderr_mode.clone(),
        stdout_regex: cfg.stdout_regex.clone(),
        stderr_regex: cfg.stderr_regex.clone(),
        timeout_seconds: cfg.timeout_seconds,
    };
    let match_result = stop_rules::quant_rule_matches(&runtime_cfg, &command_result);
    if !match_result.matched {
        let _ = logger.write_line(&format!("quant stop not matched: {}", match_result.reason));
        return None;
    }

    let decision = stop_rules::normalize_decision(&cfg.decision);
    if decision == stop_rules::STOP_DECISION_CONTINUE {
        let _ = logger.write_line("quant stop matched but decision=continue");
        return None;
    }

    let phase = if after_run { "after-run" } else { "before-run" };
    Some(format!(
        "quantitative stop matched ({phase}): {}",
        match_result.reason
    ))
}

fn evaluate_qualitative_stop(
    repo_path: &str,
    cfg: &StoredQualStopConfig,
    iteration_index: i32,
    single_run: bool,
    plan: &QueuePlan,
    logger: &mut LoopLogger,
) -> Option<String> {
    if cfg.every_n <= 0 {
        return None;
    }
    let qual_due = iteration_index > 0 && iteration_index % cfg.every_n == 0;
    let interaction_plan = QueueInteractionPlan {
        has_messages: !plan.messages.is_empty(),
        has_prompt_override: plan.override_prompt.is_some(),
        pause_requested: plan.pause_duration.is_some(),
        pause_before_run: plan.pause_before_run,
        stop_requested: !plan.stop_ids.is_empty(),
        kill_requested: !plan.kill_ids.is_empty(),
    };
    if !should_inject_qualitative_stop(qual_due, single_run, &interaction_plan) {
        return None;
    }

    let judge_output = match resolve_qualitative_output(repo_path, cfg) {
        Ok(output) => output,
        Err(err) => {
            let _ = logger.write_line(&format!("qual stop judge error: {err}"));
            err
        }
    };
    if !stop_rules::qual_should_stop(&judge_output, &cfg.on_invalid) {
        let _ = logger.write_line("qual stop not matched");
        return None;
    }
    Some("qualitative stop matched".to_string())
}

fn resolve_qualitative_output(
    repo_path: &str,
    cfg: &StoredQualStopConfig,
) -> Result<String, String> {
    if cfg.is_prompt_path {
        let prompt = resolve_override_prompt(
            repo_path,
            &PromptOverridePayload {
                prompt: cfg.prompt.clone(),
                is_path: true,
            },
        )?;
        return Ok(prompt.content);
    }
    Ok(cfg.prompt.clone())
}

#[derive(Debug, Clone)]
struct PromptOverride {
    prompt: String,
    is_path: bool,
}

#[derive(Debug, Clone, Default)]
struct QueuePlan {
    messages: Vec<OperatorMessage>,
    override_prompt: Option<PromptOverride>,
    pause_duration: Option<Duration>,
    pause_before_run: bool,
    consume_ids: Vec<String>,
    pause_ids: Vec<String>,
    stop_ids: Vec<String>,
    kill_ids: Vec<String>,
}

fn build_queue_plan(
    items: &[forge_db::loop_queue_repository::LoopQueueItem],
) -> Result<QueuePlan, String> {
    let mut plan = QueuePlan::default();

    for item in items {
        if item.status != "pending" {
            continue;
        }
        match item.item_type.as_str() {
            "message_append" => {
                let payload: Value = serde_json::from_str(&item.payload)
                    .map_err(|err| format!("decode message_append payload {}: {err}", item.id))?;
                let text = payload
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !text.is_empty() {
                    plan.messages.push(OperatorMessage {
                        timestamp_rfc3339: item.created_at.clone(),
                        text,
                    });
                    plan.consume_ids.push(item.id.clone());
                }
            }
            "steer_message" => {
                let payload: Value = serde_json::from_str(&item.payload)
                    .map_err(|err| format!("decode steer_message payload {}: {err}", item.id))?;
                let text = payload
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !text.is_empty() {
                    plan.messages.push(OperatorMessage {
                        timestamp_rfc3339: item.created_at.clone(),
                        text,
                    });
                    plan.consume_ids.push(item.id.clone());
                }
            }
            "next_prompt_override" => {
                if plan.override_prompt.is_none() {
                    let payload: Value = serde_json::from_str(&item.payload).map_err(|err| {
                        format!("decode next_prompt_override payload {}: {err}", item.id)
                    })?;
                    let prompt = payload
                        .get("prompt")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let is_path = payload
                        .get("is_path")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if !prompt.trim().is_empty() {
                        plan.override_prompt = Some(PromptOverride { prompt, is_path });
                        plan.consume_ids.push(item.id.clone());
                    }
                }
            }
            "pause" => {
                let payload: Value = serde_json::from_str(&item.payload)
                    .map_err(|err| format!("decode pause payload {}: {err}", item.id))?;
                let seconds = payload
                    .get("duration_seconds")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .max(0);
                plan.pause_duration = Some(Duration::from_secs(seconds as u64));
                plan.pause_before_run = plan.messages.is_empty() && plan.override_prompt.is_none();
                plan.pause_ids.push(item.id.clone());
                break;
            }
            "stop_graceful" => {
                plan.stop_ids.push(item.id.clone());
                break;
            }
            "kill_now" => {
                plan.kill_ids.push(item.id.clone());
                break;
            }
            other => return Err(format!("unsupported queue item type \"{other}\"")),
        }
    }

    Ok(plan)
}

fn mark_queue_completed(
    queue_repo: &forge_db::loop_queue_repository::LoopQueueRepository<'_>,
    ids: &[String],
) -> Result<(), String> {
    for id in ids {
        queue_repo
            .update_status(id, "completed", "")
            .map_err(|err| format!("complete queue item {id}: {err}"))?;
    }
    Ok(())
}

fn ensure_loop_paths(
    loop_entry: &mut forge_db::loop_repository::Loop,
    loop_repo: &forge_db::loop_repository::LoopRepository<'_>,
) -> Result<(), String> {
    let mut changed = false;

    if loop_entry.log_path.trim().is_empty() {
        loop_entry.log_path =
            default_log_path(&resolve_data_dir(), &loop_entry.name, &loop_entry.id);
        changed = true;
    }
    if loop_entry.ledger_path.trim().is_empty() {
        loop_entry.ledger_path =
            default_ledger_path(&loop_entry.repo_path, &loop_entry.name, &loop_entry.id);
        changed = true;
    }

    if let Some(parent) = Path::new(&loop_entry.log_path).parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create log dir {}: {err}", parent.display()))?;
    }
    if let Some(parent) = Path::new(&loop_entry.ledger_path).parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create ledger dir {}: {err}", parent.display()))?;
    }

    if changed {
        loop_repo
            .update(loop_entry)
            .map_err(|err| format!("persist loop paths {}: {err}", loop_entry.id))?;
    }

    ensure_ledger_file(&LoopLedgerRecord {
        id: loop_entry.id.clone(),
        name: loop_entry.name.clone(),
        repo_path: loop_entry.repo_path.clone(),
        ledger_path: loop_entry.ledger_path.clone(),
    })
}

fn select_profile(
    db: &forge_db::Db,
    loop_entry: &forge_db::loop_repository::Loop,
    now: DateTime<Utc>,
) -> Result<
    (
        Option<forge_db::profile_repository::Profile>,
        Option<DateTime<Utc>>,
    ),
    String,
> {
    let profile_repo = forge_db::profile_repository::ProfileRepository::new(db);
    let pool_repo = forge_db::pool_repository::PoolRepository::new(db);
    let run_repo = forge_db::loop_run_repository::LoopRunRepository::new(db);

    if !loop_entry.profile_id.trim().is_empty() {
        let profile = profile_repo
            .get(&loop_entry.profile_id)
            .map_err(|err| format!("load pinned profile {}: {err}", loop_entry.profile_id))?;
        let (available, _, _) = profile_available(&run_repo, &profile, now)?;
        if !available {
            return Err(format!("pinned profile {} unavailable", profile.name));
        }
        return Ok((Some(profile), None));
    }

    let mut pool = if !loop_entry.pool_id.trim().is_empty() {
        pool_repo
            .get(&loop_entry.pool_id)
            .map_err(|err| format!("load pool {}: {err}", loop_entry.pool_id))?
    } else {
        pool_repo
            .get_default()
            .map_err(|_| "pool unavailable".to_string())?
    };
    let members = pool_repo
        .list_members(&pool.id)
        .map_err(|err| format!("list pool members {}: {err}", pool.id))?;
    if members.is_empty() {
        return Err("pool unavailable".to_string());
    }

    let start_index = pool_last_index(&pool);
    let mut earliest_wait: Option<DateTime<Utc>> = None;
    for offset in 0..members.len() {
        let idx = ((start_index + 1 + offset as i32).rem_euclid(members.len() as i32)) as usize;
        let member = &members[idx];
        let profile = match profile_repo.get(&member.profile_id) {
            Ok(profile) => profile,
            Err(_) => continue,
        };
        let (available, next_wait, _) = match profile_available(&run_repo, &profile, now) {
            Ok(result) => result,
            Err(_) => continue,
        };
        if available {
            set_pool_last_index(&mut pool, idx as i32);
            let _ = pool_repo.update(&mut pool);
            return Ok((Some(profile), None));
        }
        if let Some(wait_until) = next_wait {
            earliest_wait = Some(match earliest_wait {
                Some(existing) if existing <= wait_until => existing,
                _ => wait_until,
            });
        }
    }

    let wait_until =
        earliest_wait.unwrap_or_else(|| now + chrono::Duration::seconds(DEFAULT_WAIT_SECONDS));
    Ok((None, Some(wait_until)))
}

fn profile_available(
    run_repo: &forge_db::loop_run_repository::LoopRunRepository<'_>,
    profile: &forge_db::profile_repository::Profile,
    now: DateTime<Utc>,
) -> Result<ProfileAvailability, String> {
    if let Some(cooldown) = profile
        .cooldown_until
        .as_deref()
        .and_then(parse_rfc3339_utc)
    {
        if cooldown > now {
            return Ok((false, Some(cooldown), None));
        }
    }

    if profile.max_concurrency > 0 {
        let running = run_repo
            .count_running_by_profile(&profile.id)
            .map_err(|err| format!("count running for profile {}: {err}", profile.id))?;
        if running >= profile.max_concurrency {
            return Ok((false, None, None));
        }
    }

    Ok((true, None, None))
}

type ProfileAvailability = (bool, Option<DateTime<Utc>>, Option<String>);

fn pool_last_index(pool: &forge_db::pool_repository::Pool) -> i32 {
    let Some(metadata) = &pool.metadata else {
        return -1;
    };
    let Some(value) = metadata.get("last_index") else {
        return -1;
    };
    match value {
        Value::Number(num) => num.as_i64().unwrap_or(-1) as i32,
        Value::String(text) => text.parse::<i32>().unwrap_or(-1),
        _ => -1,
    }
}

fn set_pool_last_index(pool: &mut forge_db::pool_repository::Pool, idx: i32) {
    let metadata = pool.metadata.get_or_insert_with(HashMap::new);
    metadata.insert("last_index".to_string(), Value::from(idx));
}

#[derive(Debug, Clone)]
struct PreparedPrompt {
    prompt_path: String,
    prompt_content: String,
}

fn prepare_prompt(
    loop_entry: &forge_db::loop_repository::Loop,
    run_id: &str,
    profile: &forge_db::profile_repository::Profile,
    prompt: &forge_loop::prompt_composition::PromptSpec,
    prompt_content: &str,
    has_messages: bool,
) -> Result<PreparedPrompt, String> {
    let mode = to_prompt_mode(&profile.prompt_mode);
    let mut prompt_path = prompt.path.clone();
    let needs_render = !prompt.from_file || has_messages;

    if matches!(mode, HarnessPromptMode::Path) && (prompt_path.trim().is_empty() || needs_render) {
        prompt_path =
            write_prompt_file(&resolve_data_dir(), &loop_entry.id, run_id, prompt_content)?;
    }

    Ok(PreparedPrompt {
        prompt_path,
        prompt_content: prompt_content.to_string(),
    })
}

#[derive(Debug, Clone)]
struct ExecutionResult {
    exit_code: i32,
    output_tail: String,
    err_text: String,
}

fn execute_profile(
    profile: &forge_db::profile_repository::Profile,
    loop_entry: &forge_db::loop_repository::Loop,
    prompt_path: &str,
    prompt_content: &str,
    logger: &mut LoopLogger,
) -> ExecutionResult {
    let mut profile_env = profile.env.clone();
    profile_env.insert("FORGE_LOOP_ID".to_string(), loop_entry.id.clone());
    profile_env.insert("FORGE_LOOP_NAME".to_string(), loop_entry.name.clone());
    if !profile_env.contains_key("FMAIL_AGENT")
        || profile_env
            .get("FMAIL_AGENT")
            .is_some_and(|value| value.trim().is_empty())
    {
        profile_env.insert("FMAIL_AGENT".to_string(), loop_entry.name.clone());
    }
    if !profile_env.contains_key("SV_REPO")
        || profile_env
            .get("SV_REPO")
            .is_some_and(|value| value.trim().is_empty())
    {
        profile_env.insert("SV_REPO".to_string(), loop_entry.repo_path.clone());
    }
    if !profile_env.contains_key("SV_ACTOR")
        || profile_env
            .get("SV_ACTOR")
            .is_some_and(|value| value.trim().is_empty())
    {
        let actor = profile_env
            .get("FMAIL_AGENT")
            .cloned()
            .unwrap_or_else(|| loop_entry.name.clone());
        profile_env.insert("SV_ACTOR".to_string(), actor);
    }

    let spec = ProfileSpec {
        harness: to_harness_kind(&profile.harness),
        prompt_mode: Some(to_prompt_mode(&profile.prompt_mode)),
        command_template: profile.command_template.clone(),
        extra_args: profile.extra_args.clone(),
        auth_home: profile.auth_home.clone(),
        env: profile_env.into_iter().collect(),
    };

    let base_env: Vec<String> = std::env::vars()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    let plan = match build_execution_plan(&spec, prompt_path, prompt_content, &base_env) {
        Ok(plan) => plan,
        Err(err) => {
            return ExecutionResult {
                exit_code: -1,
                output_tail: String::new(),
                err_text: err,
            };
        }
    };

    let mut command = Command::new("bash");
    command.arg("-lc").arg(&plan.command);
    command.current_dir(&loop_entry.repo_path);
    command.env_clear();
    for env_pair in &plan.env {
        if let Some((key, value)) = env_pair.split_once('=') {
            command.env(key, value);
        }
    }
    if plan.stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return ExecutionResult {
                exit_code: -1,
                output_tail: String::new(),
                err_text: err.to_string(),
            };
        }
    };

    if let Some(stdin_payload) = &plan.stdin {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(stdin_payload.as_bytes());
        }
    }

    let mut combined = String::new();
    let mut stream_errors = Vec::new();

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            return ExecutionResult {
                exit_code: -1,
                output_tail: String::new(),
                err_text: "child stdout pipe unavailable".to_string(),
            };
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            return ExecutionResult {
                exit_code: -1,
                output_tail: String::new(),
                err_text: "child stderr pipe unavailable".to_string(),
            };
        }
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let stdout_reader = std::thread::spawn({
        let tx = tx.clone();
        move || read_stream_chunks(stdout, tx)
    });
    let stderr_reader = std::thread::spawn({
        let tx = tx.clone();
        move || read_stream_chunks(stderr, tx)
    });
    drop(tx);

    while let Ok(chunk) = rx.recv() {
        if chunk.is_empty() {
            continue;
        }
        let _ = logger.write_all(&chunk);
        combined.push_str(&String::from_utf8_lossy(&chunk));
    }
    let _ = logger.flush();

    if let Ok(Some(err)) = stdout_reader.join() {
        stream_errors.push(format!("stdout read error: {err}"));
    }
    if let Ok(Some(err)) = stderr_reader.join() {
        stream_errors.push(format!("stderr read error: {err}"));
    }

    let status = match child.wait() {
        Ok(status) => status,
        Err(err) => {
            return ExecutionResult {
                exit_code: -1,
                output_tail: tail_lines(&combined, DEFAULT_OUTPUT_TAIL_LINES),
                err_text: format!("wait failed: {err}"),
            };
        }
    };

    let exit_code = status.code().unwrap_or(-1);
    let mut err_text = if status.success() {
        String::new()
    } else {
        format!("exit status {exit_code}")
    };
    if !stream_errors.is_empty() {
        let stream_msg = stream_errors.join("; ");
        if err_text.is_empty() {
            err_text = stream_msg;
        } else {
            err_text.push_str("; ");
            err_text.push_str(&stream_msg);
        }
    }
    ExecutionResult {
        exit_code,
        output_tail: tail_lines(&combined, DEFAULT_OUTPUT_TAIL_LINES),
        err_text,
    }
}

fn read_stream_chunks<R: Read>(mut reader: R, tx: Sender<Vec<u8>>) -> Option<String> {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return None,
            Ok(n) => {
                if tx.send(buf[..n].to_vec()).is_err() {
                    return None;
                }
            }
            Err(err) => return Some(err.to_string()),
        }
    }
}

fn write_prompt_file(
    data_dir: &str,
    loop_id: &str,
    run_id: &str,
    content: &str,
) -> Result<String, String> {
    let path = Path::new(data_dir)
        .join("prompts")
        .join(loop_id)
        .join(format!("{run_id}.md"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create prompt dir {}: {err}", parent.display()))?;
    }
    fs::write(&path, content)
        .map_err(|err| format!("write prompt file {}: {err}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

fn limit_reason(
    loop_entry: &forge_db::loop_repository::Loop,
    metadata: &HashMap<String, Value>,
    now: DateTime<Utc>,
) -> Option<String> {
    let iteration_count = metadata_i64(metadata, "iteration_count").unwrap_or(0);
    if loop_entry.max_iterations > 0 && iteration_count >= loop_entry.max_iterations {
        return Some(format!(
            "max iterations reached ({})",
            loop_entry.max_iterations
        ));
    }
    if loop_entry.max_runtime_seconds > 0 {
        let started_at = metadata_datetime(metadata, "started_at")?;
        let elapsed = now.signed_duration_since(started_at);
        if elapsed.num_seconds() >= loop_entry.max_runtime_seconds {
            return Some(format!(
                "max runtime reached ({}s)",
                loop_entry.max_runtime_seconds
            ));
        }
    }
    None
}

fn metadata_i64(metadata: &HashMap<String, Value>, key: &str) -> Option<i64> {
    let value = metadata.get(key)?;
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse::<i64>().ok(),
        _ => None,
    }
}

fn metadata_datetime(metadata: &HashMap<String, Value>, key: &str) -> Option<DateTime<Utc>> {
    let value = metadata.get(key)?;
    match value {
        Value::String(text) => parse_rfc3339_utc(text),
        _ => None,
    }
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

fn tail_lines(content: &str, max_lines: usize) -> String {
    if max_lines == 0 {
        return content.to_string();
    }
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= max_lines {
        return content.to_string();
    }
    lines[lines.len() - max_lines..].join("\n")
}

fn to_harness_kind(value: &str) -> HarnessKind {
    match value {
        "pi" => HarnessKind::Pi,
        "claude" => HarnessKind::Claude,
        "codex" => HarnessKind::Codex,
        "opencode" => HarnessKind::OpenCode,
        "droid" => HarnessKind::Droid,
        other => HarnessKind::Other(other.to_string()),
    }
}

fn to_prompt_mode(value: &str) -> HarnessPromptMode {
    match value {
        "stdin" => HarnessPromptMode::Stdin,
        "path" => HarnessPromptMode::Path,
        _ => HarnessPromptMode::Env,
    }
}

fn resolve_data_dir() -> String {
    crate::runtime_paths::resolve_data_dir()
        .to_string_lossy()
        .into_owned()
}

fn default_log_path(data_dir: &str, name: &str, id: &str) -> String {
    let slug = loop_slug(name);
    let file_stem = if slug.is_empty() { id } else { slug.as_str() };
    format!("{data_dir}/logs/loops/{file_stem}.log")
}

fn default_ledger_path(repo_path: &str, name: &str, id: &str) -> String {
    let slug = loop_slug(name);
    let file_stem = if slug.is_empty() { id } else { slug.as_str() };
    format!("{repo_path}/.forge/ledgers/{file_stem}.md")
}

fn loop_slug(name: &str) -> String {
    let lowered = name.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let mut prev_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            prev_dash = false;
            continue;
        }
        if (ch == ' ' || ch == '-' || ch == '_') && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}
