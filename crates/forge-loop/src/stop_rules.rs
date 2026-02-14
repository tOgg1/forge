use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use regex::Regex;
use wait_timeout::ChildExt;

pub const STOP_WHEN_BEFORE: &str = "before";
pub const STOP_WHEN_AFTER: &str = "after";
pub const STOP_WHEN_BOTH: &str = "both";

pub const STOP_DECISION_STOP: &str = "stop";
pub const STOP_DECISION_CONTINUE: &str = "continue";
pub const QUAL_SIGNAL_STOP: i32 = 0;
pub const QUAL_SIGNAL_CONTINUE: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopExprOperator {
    Eq,
    NotEq,
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopExpr {
    pub operator: StopExprOperator,
    pub rhs: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StopExprContext {
    pub tasks_open: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopToolSpec {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopToolRunResult {
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub should_stop: bool,
    pub decision_source: StopToolDecisionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopToolDecisionSource {
    Output,
    ExitStatus,
}

pub fn parse_stop_expr(expr: &str) -> Result<StopExpr, String> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err("stop expression is required".to_string());
    }

    let pattern = r"(?i)^\s*count\(\s*tasks\.open\s*\)\s*(==|!=|>=|<=|>|<)\s*(-?\d+)\s*$";
    let re = Regex::new(pattern).map_err(|err| format!("compile stop expression regex: {err}"))?;
    let captures = re
        .captures(trimmed)
        .ok_or_else(|| format!("unsupported stop expression: {trimmed:?}"))?;

    let operator = match captures.get(1).map(|m| m.as_str()).unwrap_or_default() {
        "==" => StopExprOperator::Eq,
        "!=" => StopExprOperator::NotEq,
        ">" => StopExprOperator::Gt,
        ">=" => StopExprOperator::Gte,
        "<" => StopExprOperator::Lt,
        "<=" => StopExprOperator::Lte,
        other => return Err(format!("unsupported stop operator: {other}")),
    };
    let rhs = captures
        .get(2)
        .map(|m| m.as_str())
        .unwrap_or_default()
        .parse::<i64>()
        .map_err(|err| format!("invalid stop expression number: {err}"))?;
    Ok(StopExpr { operator, rhs })
}

pub fn eval_stop_expr(expr: &StopExpr, ctx: &StopExprContext) -> bool {
    match expr.operator {
        StopExprOperator::Eq => ctx.tasks_open == expr.rhs,
        StopExprOperator::NotEq => ctx.tasks_open != expr.rhs,
        StopExprOperator::Gt => ctx.tasks_open > expr.rhs,
        StopExprOperator::Gte => ctx.tasks_open >= expr.rhs,
        StopExprOperator::Lt => ctx.tasks_open < expr.rhs,
        StopExprOperator::Lte => ctx.tasks_open <= expr.rhs,
    }
}

pub fn eval_stop_expr_text(expr: &str, ctx: &StopExprContext) -> Result<bool, String> {
    let parsed = parse_stop_expr(expr)?;
    Ok(eval_stop_expr(&parsed, ctx))
}

pub fn parse_stop_tool_bool(output: &str) -> Option<bool> {
    let token = output.split_whitespace().next()?.to_ascii_lowercase();
    match token.as_str() {
        "1" | "true" | "yes" | "y" | "stop" => Some(true),
        "0" | "false" | "no" | "n" | "continue" => Some(false),
        _ => None,
    }
}

pub fn run_stop_tool(
    work_dir: &Path,
    spec: &StopToolSpec,
    timeout: Duration,
) -> Result<StopToolRunResult, String> {
    let name = spec.name.trim();
    if name.is_empty() {
        return Err("stop tool name is required".to_string());
    }

    let command = if spec.args.is_empty() {
        name.to_string()
    } else {
        format!("{name} {}", spec.args.join(" "))
    };
    let mut cmd = Command::new(name);
    cmd.args(&spec.args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("spawn stop tool {command:?}: {err}"))?;

    let status = if timeout > Duration::ZERO {
        match child.wait_timeout(timeout) {
            Ok(Some(status)) => status,
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "stop tool {command:?} timed out after {}ms",
                    timeout.as_millis()
                ));
            }
            Err(err) => {
                return Err(format!("wait for stop tool {command:?}: {err}"));
            }
        }
    } else {
        child
            .wait()
            .map_err(|err| format!("wait for stop tool {command:?}: {err}"))?
    };

    let stdout = read_pipe(child.stdout.take());
    let stderr = read_pipe(child.stderr.take());
    let exit_code = status.code().unwrap_or(-1);
    let output_decision = parse_stop_tool_bool(&stdout).or_else(|| parse_stop_tool_bool(&stderr));
    let (should_stop, decision_source) = match output_decision {
        Some(value) => (value, StopToolDecisionSource::Output),
        None => (status.success(), StopToolDecisionSource::ExitStatus),
    };

    Ok(StopToolRunResult {
        command,
        exit_code,
        stdout,
        stderr,
        should_stop,
        decision_source,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantCommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub error: Option<String>,
}

pub fn quant_when_matches(when: &str, after_run: bool) -> bool {
    match when.trim().to_ascii_lowercase().as_str() {
        STOP_WHEN_BEFORE | "" => !after_run,
        STOP_WHEN_AFTER => after_run,
        STOP_WHEN_BOTH => true,
        _ => !after_run,
    }
}

pub fn quant_every_matches(every_n: i32, iteration_index: i32) -> bool {
    every_n > 0 && iteration_index > 0 && iteration_index % every_n == 0
}

pub fn normalize_decision(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case(STOP_DECISION_CONTINUE) {
        STOP_DECISION_CONTINUE
    } else {
        STOP_DECISION_STOP
    }
}

pub fn quant_should_evaluate(
    when: &str,
    every_n: i32,
    iteration_index: i32,
    after_run: bool,
) -> bool {
    quant_when_matches(when, after_run) && quant_every_matches(every_n, iteration_index)
}

pub fn parse_qual_signal(output: &str) -> Option<i32> {
    match output.split_whitespace().next() {
        Some("0") => Some(QUAL_SIGNAL_STOP),
        Some("1") => Some(QUAL_SIGNAL_CONTINUE),
        _ => None,
    }
}

pub fn normalize_on_invalid(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case(STOP_DECISION_STOP) {
        STOP_DECISION_STOP
    } else {
        STOP_DECISION_CONTINUE
    }
}

pub fn qual_invalid_output_requests_stop(on_invalid: &str) -> bool {
    normalize_on_invalid(on_invalid) == STOP_DECISION_STOP
}

pub fn qual_should_stop(output: &str, on_invalid: &str) -> bool {
    match parse_qual_signal(output) {
        Some(QUAL_SIGNAL_STOP) => true,
        Some(QUAL_SIGNAL_CONTINUE) => false,
        _ => qual_invalid_output_requests_stop(on_invalid),
    }
}

pub fn run_quant_command(work_dir: &Path, cmd: &str, timeout: Duration) -> QuantCommandResult {
    if cmd.trim().is_empty() {
        return QuantCommandResult {
            exit_code: -1,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            error: Some("empty command".to_string()),
        };
    }

    let mut child = match Command::new("bash")
        .arg("-lc")
        .arg(cmd)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return QuantCommandResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                error: Some(err.to_string()),
            };
        }
    };

    let mut timed_out = false;
    let mut error = None;

    let status = if timeout > Duration::ZERO {
        match child.wait_timeout(timeout) {
            Ok(Some(status)) => status,
            Ok(None) => {
                timed_out = true;
                if let Err(err) = child.kill() {
                    error = Some(format!("failed to kill timed out command: {err}"));
                }
                match child.wait() {
                    Ok(status) => status,
                    Err(err) => {
                        return QuantCommandResult {
                            exit_code: -1,
                            stdout: String::new(),
                            stderr: String::new(),
                            timed_out: true,
                            error: Some(format!("failed to wait for timed out command: {err}")),
                        };
                    }
                }
            }
            Err(err) => {
                return QuantCommandResult {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    error: Some(err.to_string()),
                };
            }
        }
    } else {
        match child.wait() {
            Ok(status) => status,
            Err(err) => {
                return QuantCommandResult {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    error: Some(err.to_string()),
                };
            }
        }
    };

    let stdout = read_pipe(child.stdout.take());
    let stderr = read_pipe(child.stderr.take());
    let exit_code = if timed_out {
        -1
    } else {
        status.code().unwrap_or(-1)
    };

    if timed_out && error.is_none() {
        error = Some("command timed out".to_string());
    }

    QuantCommandResult {
        exit_code,
        stdout,
        stderr,
        timed_out,
        error,
    }
}

fn read_pipe<T: Read>(mut pipe: Option<T>) -> String {
    let mut output = String::new();
    if let Some(reader) = pipe.as_mut() {
        let _ = reader.read_to_string(&mut output);
    }
    output
}

// ---------------------------------------------------------------------------
// Quantitative rule matching (exit/stdout/stderr)
// ---------------------------------------------------------------------------

/// Configuration for quantitative stop rules.
///
/// Mirrors Go `LoopQuantStopConfig` from `internal/models/loop_stop.go`.
#[derive(Debug, Clone, Default)]
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

/// Result of evaluating a quantitative rule against a command result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantMatchResult {
    pub matched: bool,
    pub reason: String,
}

/// Normalize a stream mode string to one of `"any"`, `"empty"`, or `"nonempty"`.
pub fn normalize_stream_mode(value: &str) -> &'static str {
    match value.trim().to_lowercase().as_str() {
        "empty" => "empty",
        "nonempty" => "nonempty",
        "any" => "any",
        _ => "any",
    }
}

/// Check whether `exit_code` matches the configured exit code criteria.
///
/// An empty `codes` slice means exit code matching is disabled (always matches).
/// When `invert` is true, the match is inverted (match when NOT in the list).
pub fn match_exit_codes(exit_code: i32, codes: &[i32], invert: bool) -> bool {
    if codes.is_empty() {
        return true;
    }
    let is_in = codes.contains(&exit_code);
    if invert {
        !is_in
    } else {
        is_in
    }
}

/// Check whether a stream value satisfies the given mode.
///
/// - `"empty"`: matches when `s` is empty or whitespace-only.
/// - `"nonempty"`: matches when `s` contains non-whitespace content.
/// - `"any"` (default): always matches.
pub fn matches_stream_mode(mode: &str, s: &str) -> bool {
    let mode = normalize_stream_mode(mode);
    let empty = s.trim().is_empty();
    match mode {
        "empty" => empty,
        "nonempty" => !empty,
        _ => true,
    }
}

/// Compile a regex pattern, returning `None` for empty/whitespace patterns.
fn compile_regex(pattern: &str) -> Result<Option<Regex>, String> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Regex::new(trimmed).map(Some).map_err(|e| e.to_string())
}

/// Evaluate all quantitative rule criteria against a command result.
///
/// Returns a matched result when all configured criteria pass, or
/// a not-matched result with a reason when any criterion fails.
/// All criteria are AND-ed together.
///
/// Mirrors Go `quantRuleMatches` from `internal/loop/stop_rules.go`.
pub fn quant_rule_matches(cfg: &QuantStopConfig, res: &QuantCommandResult) -> QuantMatchResult {
    if cfg.cmd.trim().is_empty() {
        return QuantMatchResult {
            matched: false,
            reason: "empty cmd".to_string(),
        };
    }

    let stdout_mode = normalize_stream_mode(&cfg.stdout_mode);
    let stderr_mode = normalize_stream_mode(&cfg.stderr_mode);
    let no_exit = cfg.exit_codes.is_empty();
    let no_stream_mode = stdout_mode == "any" && stderr_mode == "any";
    let no_regex = cfg.stdout_regex.trim().is_empty() && cfg.stderr_regex.trim().is_empty();

    if no_exit && no_stream_mode && no_regex {
        return QuantMatchResult {
            matched: false,
            reason: "no match criteria configured".to_string(),
        };
    }

    if !match_exit_codes(res.exit_code, &cfg.exit_codes, cfg.exit_invert) {
        return QuantMatchResult {
            matched: false,
            reason: format!("exit_code={} not matched", res.exit_code),
        };
    }
    if !matches_stream_mode(stdout_mode, &res.stdout) {
        return QuantMatchResult {
            matched: false,
            reason: format!("stdout_mode={stdout_mode} not matched"),
        };
    }
    if !matches_stream_mode(stderr_mode, &res.stderr) {
        return QuantMatchResult {
            matched: false,
            reason: format!("stderr_mode={stderr_mode} not matched"),
        };
    }

    match compile_regex(&cfg.stdout_regex) {
        Err(e) => {
            return QuantMatchResult {
                matched: false,
                reason: format!("invalid stdout_regex: {e}"),
            };
        }
        Ok(Some(re)) if !re.is_match(&res.stdout) => {
            return QuantMatchResult {
                matched: false,
                reason: "stdout_regex not matched".to_string(),
            };
        }
        _ => {}
    }

    match compile_regex(&cfg.stderr_regex) {
        Err(e) => {
            return QuantMatchResult {
                matched: false,
                reason: format!("invalid stderr_regex: {e}"),
            };
        }
        Ok(Some(re)) if !re.is_match(&res.stderr) => {
            return QuantMatchResult {
                matched: false,
                reason: "stderr_regex not matched".to_string(),
            };
        }
        _ => {}
    }

    QuantMatchResult {
        matched: true,
        reason: "matched".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn run_stop_tool_ok(
        work_dir: &Path,
        spec: &StopToolSpec,
        timeout: Duration,
    ) -> StopToolRunResult {
        match run_stop_tool(work_dir, spec, timeout) {
            Ok(res) => res,
            Err(err) => panic!("run_stop_tool unexpectedly failed: {err}"),
        }
    }

    fn run_stop_tool_err(work_dir: &Path, spec: &StopToolSpec, timeout: Duration) -> String {
        match run_stop_tool(work_dir, spec, timeout) {
            Ok(res) => panic!("run_stop_tool unexpectedly succeeded: {:?}", res),
            Err(err) => err,
        }
    }

    fn parse_stop_expr_ok(expr: &str) -> StopExpr {
        match parse_stop_expr(expr) {
            Ok(parsed) => parsed,
            Err(err) => panic!("parse_stop_expr unexpectedly failed for {expr:?}: {err}"),
        }
    }

    fn parse_stop_expr_err(expr: &str) -> String {
        match parse_stop_expr(expr) {
            Ok(parsed) => panic!(
                "parse_stop_expr unexpectedly succeeded for {expr:?}: {:?}",
                parsed
            ),
            Err(err) => err,
        }
    }

    fn eval_stop_expr_ok(expr: &str, ctx: &StopExprContext) -> bool {
        match eval_stop_expr_text(expr, ctx) {
            Ok(value) => value,
            Err(err) => panic!("eval_stop_expr_text unexpectedly failed for {expr:?}: {err}"),
        }
    }

    #[test]
    fn parse_stop_tool_bool_accepts_common_tokens() {
        assert_eq!(parse_stop_tool_bool("true"), Some(true));
        assert_eq!(parse_stop_tool_bool("TRUE"), Some(true));
        assert_eq!(parse_stop_tool_bool("stop now"), Some(true));
        assert_eq!(parse_stop_tool_bool("false"), Some(false));
        assert_eq!(parse_stop_tool_bool("continue"), Some(false));
        assert_eq!(parse_stop_tool_bool("0"), Some(false));
        assert_eq!(parse_stop_tool_bool("1"), Some(true));
        assert_eq!(parse_stop_tool_bool("maybe"), None);
        assert_eq!(parse_stop_tool_bool(""), None);
    }

    #[test]
    fn stop_tool_uses_output_boolean_when_present() {
        let spec = StopToolSpec {
            name: "bash".to_string(),
            args: vec!["-lc".to_string(), "echo false; exit 0".to_string()],
        };
        let res = run_stop_tool_ok(Path::new("."), &spec, Duration::from_secs(1));
        assert_eq!(res.exit_code, 0);
        assert!(!res.should_stop);
        assert_eq!(res.decision_source, StopToolDecisionSource::Output);
    }

    #[test]
    fn stop_tool_falls_back_to_exit_status_when_output_not_boolean() {
        let true_spec = StopToolSpec {
            name: "bash".to_string(),
            args: vec!["-lc".to_string(), "echo not-a-bool; exit 0".to_string()],
        };
        let true_res = run_stop_tool_ok(Path::new("."), &true_spec, Duration::from_secs(1));
        assert!(true_res.should_stop);
        assert_eq!(true_res.decision_source, StopToolDecisionSource::ExitStatus);

        let false_spec = StopToolSpec {
            name: "bash".to_string(),
            args: vec!["-lc".to_string(), "echo still-not-bool; exit 3".to_string()],
        };
        let false_res = run_stop_tool_ok(Path::new("."), &false_spec, Duration::from_secs(1));
        assert_eq!(false_res.exit_code, 3);
        assert!(!false_res.should_stop);
        assert_eq!(
            false_res.decision_source,
            StopToolDecisionSource::ExitStatus
        );
    }

    #[test]
    fn stop_tool_surfaces_spawn_failures_clearly() {
        let spec = StopToolSpec {
            name: "__forge_missing_stop_tool_binary__".to_string(),
            args: Vec::new(),
        };
        let err = run_stop_tool_err(Path::new("."), &spec, Duration::from_secs(1));
        assert!(err.contains("spawn stop tool"));
        assert!(err.contains("__forge_missing_stop_tool_binary__"));
    }

    #[test]
    fn stop_tool_timeout_is_reported_cleanly() {
        let spec = StopToolSpec {
            name: "bash".to_string(),
            args: vec!["-lc".to_string(), "sleep 2".to_string()],
        };
        let err = run_stop_tool_err(Path::new("."), &spec, Duration::from_millis(50));
        assert!(err.contains("timed out"));
    }

    #[test]
    fn stop_tool_respects_work_dir() {
        let temp = TempDir::new("forge-loop-stop-tool");
        let spec = StopToolSpec {
            name: "bash".to_string(),
            args: vec!["-lc".to_string(), "pwd".to_string()],
        };
        let res = run_stop_tool_ok(temp.path(), &spec, Duration::from_secs(1));
        let expected = temp.path().to_string_lossy().into_owned();
        assert!(res.stdout.contains(&expected));
    }

    #[test]
    fn empty_command_returns_error_result() {
        let res = run_quant_command(Path::new("."), "   ", Duration::from_secs(1));
        assert_eq!(res.exit_code, -1);
        assert!(res.stdout.is_empty());
        assert!(res.stderr.is_empty());
        assert!(!res.timed_out);
        assert_eq!(res.error.as_deref(), Some("empty command"));
    }

    #[test]
    fn captures_stdout_stderr_and_exit_code() {
        let res = run_quant_command(
            Path::new("."),
            "echo hello && echo warn >&2 && exit 7",
            Duration::from_secs(1),
        );
        assert_eq!(res.exit_code, 7);
        assert!(res.stdout.contains("hello"));
        assert!(res.stderr.contains("warn"));
        assert!(!res.timed_out);
    }

    #[test]
    fn timeout_sets_exit_code_minus_one_and_marks_timeout() {
        let res = run_quant_command(Path::new("."), "sleep 2", Duration::from_millis(50));
        assert_eq!(res.exit_code, -1);
        assert!(res.timed_out);
    }

    #[test]
    fn command_runs_in_given_work_dir() {
        let temp = TempDir::new("forge-loop-stop-rules");
        let res = run_quant_command(temp.path(), "pwd", Duration::from_secs(1));
        assert_eq!(res.exit_code, 0);
        let expected = temp.path().to_string_lossy().into_owned();
        assert!(
            res.stdout.contains(&expected),
            "expected cwd in stdout, got: {}",
            res.stdout
        );
    }

    #[test]
    fn missing_command_returns_shell_error_code() {
        let res = run_quant_command(
            Path::new("."),
            "__forge_missing_binary_for_test__",
            Duration::from_secs(1),
        );
        assert_eq!(res.exit_code, 127);
        assert!(!res.timed_out);
    }

    #[test]
    fn parse_stop_expr_supports_count_tasks_open_comparisons() {
        let parsed = parse_stop_expr_ok("count(tasks.open) == 0");
        assert_eq!(parsed.operator, StopExprOperator::Eq);
        assert_eq!(parsed.rhs, 0);

        let parsed = parse_stop_expr_ok("count(tasks.open) > 20");
        assert_eq!(parsed.operator, StopExprOperator::Gt);
        assert_eq!(parsed.rhs, 20);

        let parsed = parse_stop_expr_ok(" COUNT(tasks.open) <= -5 ");
        assert_eq!(parsed.operator, StopExprOperator::Lte);
        assert_eq!(parsed.rhs, -5);
    }

    #[test]
    fn parse_stop_expr_rejects_unsupported_shapes() {
        let err = parse_stop_expr_err("");
        assert!(err.contains("required"));

        let err = parse_stop_expr_err("tasks.open == 0");
        assert!(err.contains("unsupported stop expression"));

        let err = parse_stop_expr_err("count(tasks.closed) == 0");
        assert!(err.contains("unsupported stop expression"));
    }

    #[test]
    fn eval_stop_expr_compares_against_context_tasks_open() {
        let ctx = StopExprContext { tasks_open: 7 };
        assert!(eval_stop_expr_ok("count(tasks.open) > 5", &ctx));
        assert!(eval_stop_expr_ok("count(tasks.open) >= 7", &ctx));
        assert!(eval_stop_expr_ok("count(tasks.open) != 0", &ctx));
        assert!(!eval_stop_expr_ok("count(tasks.open) < 3", &ctx));
        assert!(!eval_stop_expr_ok("count(tasks.open) == 0", &ctx));
    }

    #[test]
    fn parse_stop_tool_bool_accepts_common_truthy_and_falsy_tokens() {
        assert_eq!(parse_stop_tool_bool("1"), Some(true));
        assert_eq!(parse_stop_tool_bool("true"), Some(true));
        assert_eq!(parse_stop_tool_bool("yes"), Some(true));
        assert_eq!(parse_stop_tool_bool("stop"), Some(true));

        assert_eq!(parse_stop_tool_bool("0"), Some(false));
        assert_eq!(parse_stop_tool_bool("false"), Some(false));
        assert_eq!(parse_stop_tool_bool("no"), Some(false));
        assert_eq!(parse_stop_tool_bool("continue"), Some(false));
    }

    #[test]
    fn run_stop_tool_true_output_sets_should_stop_true() {
        let spec = StopToolSpec {
            name: "sh".to_string(),
            args: vec!["-c".to_string(), "printf '1\\n'".to_string()],
        };
        let res = run_stop_tool_ok(Path::new("."), &spec, Duration::from_secs(1));
        assert_eq!(res.exit_code, 0);
        assert!(res.should_stop);
    }

    #[test]
    fn run_stop_tool_false_output_sets_should_stop_false() {
        let spec = StopToolSpec {
            name: "sh".to_string(),
            args: vec!["-c".to_string(), "printf '0\\n'".to_string()],
        };
        let res = run_stop_tool_ok(Path::new("."), &spec, Duration::from_secs(1));
        assert_eq!(res.exit_code, 0);
        assert!(!res.should_stop);
    }

    #[test]
    fn run_stop_tool_empty_output_defaults_to_stop() {
        let spec = StopToolSpec {
            name: "sh".to_string(),
            args: vec!["-c".to_string(), ":".to_string()],
        };
        let res = run_stop_tool_ok(Path::new("."), &spec, Duration::from_secs(1));
        assert_eq!(res.exit_code, 0);
        assert!(res.should_stop);
    }

    #[test]
    fn run_stop_tool_non_zero_exit_falls_back_to_exit_status() {
        let spec = StopToolSpec {
            name: "sh".to_string(),
            args: vec!["-c".to_string(), "echo fail >&2; exit 7".to_string()],
        };
        let res = run_stop_tool_ok(Path::new("."), &spec, Duration::from_secs(1));
        assert_eq!(res.exit_code, 7);
        assert!(res.stderr.contains("fail"));
        assert!(!res.should_stop);
        assert_eq!(res.decision_source, StopToolDecisionSource::ExitStatus);
    }

    #[test]
    fn run_stop_tool_missing_command_reports_spawn_error() {
        let spec = StopToolSpec {
            name: "__forge_missing_stop_tool_for_test__".to_string(),
            args: Vec::new(),
        };
        let err = run_stop_tool_err(Path::new("."), &spec, Duration::from_secs(1));
        assert!(err.contains("spawn stop tool"), "{err}");
    }

    #[test]
    fn run_stop_tool_invalid_output_falls_back_to_exit_status() {
        let spec = StopToolSpec {
            name: "sh".to_string(),
            args: vec!["-c".to_string(), "printf 'maybe\\n'".to_string()],
        };
        let res = run_stop_tool_ok(Path::new("."), &spec, Duration::from_secs(1));
        assert_eq!(res.exit_code, 0);
        assert_eq!(res.stdout.trim(), "maybe");
        assert!(res.should_stop);
        assert_eq!(res.decision_source, StopToolDecisionSource::ExitStatus);
    }

    #[test]
    fn quant_when_defaults_to_before() {
        assert!(quant_when_matches("", false));
        assert!(!quant_when_matches("", true));
        assert!(quant_when_matches("unknown", false));
        assert!(!quant_when_matches("unknown", true));
    }

    #[test]
    fn quant_when_before_after_both_semantics_match_go() {
        assert!(quant_when_matches("before", false));
        assert!(!quant_when_matches("before", true));
        assert!(!quant_when_matches("after", false));
        assert!(quant_when_matches("after", true));
        assert!(quant_when_matches("both", false));
        assert!(quant_when_matches("both", true));
    }

    #[test]
    fn quant_every_requires_positive_values_and_modulo_match() {
        assert!(!quant_every_matches(0, 1));
        assert!(!quant_every_matches(-1, 1));
        assert!(!quant_every_matches(1, 0));
        assert!(!quant_every_matches(2, 3));
        assert!(quant_every_matches(1, 1));
        assert!(quant_every_matches(2, 4));
    }

    #[test]
    fn normalize_decision_defaults_to_stop_and_allows_continue() {
        assert_eq!(normalize_decision("continue"), STOP_DECISION_CONTINUE);
        assert_eq!(normalize_decision(" Continue "), STOP_DECISION_CONTINUE);
        assert_eq!(normalize_decision("stop"), STOP_DECISION_STOP);
        assert_eq!(normalize_decision(""), STOP_DECISION_STOP);
        assert_eq!(normalize_decision("maybe"), STOP_DECISION_STOP);
    }

    #[test]
    fn quant_should_evaluate_combines_when_and_every_checks() {
        assert!(quant_should_evaluate("before", 2, 2, false));
        assert!(!quant_should_evaluate("before", 2, 2, true));
        assert!(quant_should_evaluate("after", 3, 3, true));
        assert!(!quant_should_evaluate("after", 3, 2, true));
        assert!(quant_should_evaluate("both", 5, 10, false));
        assert!(quant_should_evaluate("both", 5, 10, true));
    }

    #[test]
    fn parse_qual_signal_accepts_only_zero_or_one_first_token() {
        assert_eq!(parse_qual_signal("0"), Some(QUAL_SIGNAL_STOP));
        assert_eq!(parse_qual_signal("1"), Some(QUAL_SIGNAL_CONTINUE));
        assert_eq!(parse_qual_signal("0 stop"), Some(QUAL_SIGNAL_STOP));
        assert_eq!(parse_qual_signal("1 continue"), Some(QUAL_SIGNAL_CONTINUE));
        assert_eq!(parse_qual_signal(""), None);
        assert_eq!(parse_qual_signal("   "), None);
        assert_eq!(parse_qual_signal("2"), None);
        assert_eq!(parse_qual_signal("01"), None);
        assert_eq!(parse_qual_signal("continue"), None);
    }

    #[test]
    fn normalize_on_invalid_defaults_to_continue_and_accepts_stop() {
        assert_eq!(normalize_on_invalid(""), STOP_DECISION_CONTINUE);
        assert_eq!(normalize_on_invalid("continue"), STOP_DECISION_CONTINUE);
        assert_eq!(normalize_on_invalid(" Continue "), STOP_DECISION_CONTINUE);
        assert_eq!(normalize_on_invalid("weird"), STOP_DECISION_CONTINUE);
        assert_eq!(normalize_on_invalid("stop"), STOP_DECISION_STOP);
        assert_eq!(normalize_on_invalid(" STOP "), STOP_DECISION_STOP);
    }

    #[test]
    fn invalid_output_stop_policy_is_explicit_only() {
        assert!(qual_invalid_output_requests_stop("stop"));
        assert!(qual_invalid_output_requests_stop(" Stop "));
        assert!(!qual_invalid_output_requests_stop("continue"));
        assert!(!qual_invalid_output_requests_stop(""));
        assert!(!qual_invalid_output_requests_stop("unknown"));
    }

    #[test]
    fn qual_should_stop_combines_signal_and_invalid_policy() {
        assert!(qual_should_stop("0", "continue"));
        assert!(!qual_should_stop("1", "stop"));
        assert!(qual_should_stop("garbage", "stop"));
        assert!(!qual_should_stop("garbage", "continue"));
    }

    // -----------------------------------------------------------------------
    // match_exit_codes tests
    // -----------------------------------------------------------------------

    #[test]
    fn exit_codes_empty_always_matches() {
        assert!(match_exit_codes(0, &[], false));
        assert!(match_exit_codes(42, &[], false));
        assert!(match_exit_codes(-1, &[], false));
        assert!(match_exit_codes(0, &[], true));
    }

    #[test]
    fn exit_codes_match_when_in_list() {
        assert!(match_exit_codes(0, &[0], false));
        assert!(match_exit_codes(0, &[0, 1, 2], false));
        assert!(match_exit_codes(2, &[0, 1, 2], false));
        assert!(!match_exit_codes(5, &[0, 1, 2], false));
    }

    #[test]
    fn exit_codes_invert_matches_when_not_in_list() {
        assert!(!match_exit_codes(0, &[0], true));
        assert!(match_exit_codes(5, &[0, 1, 2], true));
        assert!(!match_exit_codes(1, &[0, 1, 2], true));
    }

    #[test]
    fn exit_codes_negative_exit_code() {
        assert!(match_exit_codes(-1, &[-1], false));
        assert!(!match_exit_codes(-1, &[0], false));
        assert!(match_exit_codes(-1, &[0], true));
    }

    // -----------------------------------------------------------------------
    // normalize_stream_mode tests
    // -----------------------------------------------------------------------

    #[test]
    fn normalize_stream_mode_known_values() {
        assert_eq!(normalize_stream_mode("empty"), "empty");
        assert_eq!(normalize_stream_mode("nonempty"), "nonempty");
        assert_eq!(normalize_stream_mode("any"), "any");
    }

    #[test]
    fn normalize_stream_mode_case_insensitive() {
        assert_eq!(normalize_stream_mode("EMPTY"), "empty");
        assert_eq!(normalize_stream_mode("NonEmpty"), "nonempty");
        assert_eq!(normalize_stream_mode("ANY"), "any");
    }

    #[test]
    fn normalize_stream_mode_trims_whitespace() {
        assert_eq!(normalize_stream_mode("  empty  "), "empty");
        assert_eq!(normalize_stream_mode("\tnonempty\n"), "nonempty");
    }

    #[test]
    fn normalize_stream_mode_defaults_to_any() {
        assert_eq!(normalize_stream_mode(""), "any");
        assert_eq!(normalize_stream_mode("invalid"), "any");
        assert_eq!(normalize_stream_mode("   "), "any");
    }

    // -----------------------------------------------------------------------
    // matches_stream_mode tests
    // -----------------------------------------------------------------------

    #[test]
    fn stream_mode_any_matches_everything() {
        assert!(matches_stream_mode("any", ""));
        assert!(matches_stream_mode("any", "hello"));
        assert!(matches_stream_mode("any", "   "));
    }

    #[test]
    fn stream_mode_empty_matches_only_empty() {
        assert!(matches_stream_mode("empty", ""));
        assert!(matches_stream_mode("empty", "   "));
        assert!(matches_stream_mode("empty", "\t\n"));
        assert!(!matches_stream_mode("empty", "hello"));
        assert!(!matches_stream_mode("empty", "  x  "));
    }

    #[test]
    fn stream_mode_nonempty_matches_only_nonempty() {
        assert!(!matches_stream_mode("nonempty", ""));
        assert!(!matches_stream_mode("nonempty", "   "));
        assert!(matches_stream_mode("nonempty", "hello"));
        assert!(matches_stream_mode("nonempty", " x "));
    }

    #[test]
    fn stream_mode_unknown_defaults_to_any() {
        assert!(matches_stream_mode("unknown", ""));
        assert!(matches_stream_mode("unknown", "hello"));
        assert!(matches_stream_mode("", "anything"));
    }

    // -----------------------------------------------------------------------
    // quant_rule_matches tests
    // -----------------------------------------------------------------------

    fn make_result(exit_code: i32, stdout: &str, stderr: &str) -> QuantCommandResult {
        QuantCommandResult {
            exit_code,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            timed_out: false,
            error: None,
        }
    }

    #[test]
    fn rule_empty_cmd_does_not_match() {
        let cfg = QuantStopConfig {
            cmd: "".to_string(),
            exit_codes: vec![0],
            ..Default::default()
        };
        let m = quant_rule_matches(&cfg, &make_result(0, "", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "empty cmd");
    }

    #[test]
    fn rule_no_criteria_does_not_match() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            ..Default::default()
        };
        let m = quant_rule_matches(&cfg, &make_result(0, "", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "no match criteria configured");
    }

    #[test]
    fn rule_exit_code_match() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            exit_codes: vec![0],
            ..Default::default()
        };
        let m = quant_rule_matches(&cfg, &make_result(0, "ok", ""));
        assert!(m.matched);
        assert_eq!(m.reason, "matched");
    }

    #[test]
    fn rule_exit_code_no_match() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            exit_codes: vec![0],
            ..Default::default()
        };
        let m = quant_rule_matches(&cfg, &make_result(1, "", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "exit_code=1 not matched");
    }

    #[test]
    fn rule_exit_code_invert_match() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            exit_codes: vec![0],
            exit_invert: true,
            ..Default::default()
        };
        let m = quant_rule_matches(&cfg, &make_result(1, "", ""));
        assert!(m.matched);
    }

    #[test]
    fn rule_exit_code_invert_no_match() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            exit_codes: vec![0],
            exit_invert: true,
            ..Default::default()
        };
        let m = quant_rule_matches(&cfg, &make_result(0, "", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "exit_code=0 not matched");
    }

    #[test]
    fn rule_multiple_exit_codes() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            exit_codes: vec![0, 2, 5],
            ..Default::default()
        };
        assert!(quant_rule_matches(&cfg, &make_result(0, "", "")).matched);
        assert!(quant_rule_matches(&cfg, &make_result(2, "", "")).matched);
        assert!(quant_rule_matches(&cfg, &make_result(5, "", "")).matched);
        assert!(!quant_rule_matches(&cfg, &make_result(1, "", "")).matched);
        assert!(!quant_rule_matches(&cfg, &make_result(3, "", "")).matched);
    }

    #[test]
    fn rule_stdout_mode_empty() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            stdout_mode: "empty".to_string(),
            ..Default::default()
        };
        assert!(quant_rule_matches(&cfg, &make_result(0, "", "")).matched);
        let m = quant_rule_matches(&cfg, &make_result(0, "output", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "stdout_mode=empty not matched");
    }

    #[test]
    fn rule_stdout_mode_nonempty() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            stdout_mode: "nonempty".to_string(),
            ..Default::default()
        };
        assert!(quant_rule_matches(&cfg, &make_result(0, "output", "")).matched);
        let m = quant_rule_matches(&cfg, &make_result(0, "", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "stdout_mode=nonempty not matched");
    }

    #[test]
    fn rule_stderr_mode_empty() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            stderr_mode: "empty".to_string(),
            ..Default::default()
        };
        assert!(quant_rule_matches(&cfg, &make_result(0, "", "")).matched);
        let m = quant_rule_matches(&cfg, &make_result(0, "", "err"));
        assert!(!m.matched);
        assert_eq!(m.reason, "stderr_mode=empty not matched");
    }

    #[test]
    fn rule_stderr_mode_nonempty() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            stderr_mode: "nonempty".to_string(),
            ..Default::default()
        };
        assert!(quant_rule_matches(&cfg, &make_result(0, "", "err")).matched);
        let m = quant_rule_matches(&cfg, &make_result(0, "", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "stderr_mode=nonempty not matched");
    }

    #[test]
    fn rule_stdout_regex_match() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            stdout_regex: r"PASS".to_string(),
            ..Default::default()
        };
        assert!(quant_rule_matches(&cfg, &make_result(0, "ok PASS done", "")).matched);
        let m = quant_rule_matches(&cfg, &make_result(0, "FAIL", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "stdout_regex not matched");
    }

    #[test]
    fn rule_stderr_regex_match() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            stderr_regex: r"error\d+".to_string(),
            ..Default::default()
        };
        assert!(quant_rule_matches(&cfg, &make_result(0, "", "error42 found")).matched);
        let m = quant_rule_matches(&cfg, &make_result(0, "", "warning only"));
        assert!(!m.matched);
        assert_eq!(m.reason, "stderr_regex not matched");
    }

    #[test]
    fn rule_invalid_stdout_regex() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            stdout_regex: r"[invalid".to_string(),
            ..Default::default()
        };
        let m = quant_rule_matches(&cfg, &make_result(0, "", ""));
        assert!(!m.matched);
        assert!(m.reason.starts_with("invalid stdout_regex:"));
    }

    #[test]
    fn rule_invalid_stderr_regex() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            stderr_regex: r"[invalid".to_string(),
            ..Default::default()
        };
        let m = quant_rule_matches(&cfg, &make_result(0, "", ""));
        assert!(!m.matched);
        assert!(m.reason.starts_with("invalid stderr_regex:"));
    }

    #[test]
    fn rule_empty_regex_ignored() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            exit_codes: vec![0],
            stdout_regex: "".to_string(),
            stderr_regex: "   ".to_string(),
            ..Default::default()
        };
        assert!(quant_rule_matches(&cfg, &make_result(0, "anything", "anything")).matched);
    }

    #[test]
    fn rule_combined_exit_and_mode_and_regex() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            exit_codes: vec![0],
            stdout_mode: "nonempty".to_string(),
            stderr_mode: "empty".to_string(),
            stdout_regex: r"PASS".to_string(),
            ..Default::default()
        };

        // All criteria pass
        assert!(quant_rule_matches(&cfg, &make_result(0, "PASS tests", "")).matched);

        // Exit code fails
        let m = quant_rule_matches(&cfg, &make_result(1, "PASS tests", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "exit_code=1 not matched");

        // Stdout mode fails (empty)
        let m = quant_rule_matches(&cfg, &make_result(0, "", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "stdout_mode=nonempty not matched");

        // Stderr mode fails (nonempty)
        let m = quant_rule_matches(&cfg, &make_result(0, "PASS tests", "err"));
        assert!(!m.matched);
        assert_eq!(m.reason, "stderr_mode=empty not matched");

        // Stdout regex fails
        let m = quant_rule_matches(&cfg, &make_result(0, "FAIL tests", ""));
        assert!(!m.matched);
        assert_eq!(m.reason, "stdout_regex not matched");
    }

    #[test]
    fn rule_whitespace_only_treated_as_empty() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            stdout_mode: "empty".to_string(),
            ..Default::default()
        };
        assert!(quant_rule_matches(&cfg, &make_result(0, "  \t\n  ", "")).matched);
    }

    #[test]
    fn rule_timeout_exit_code_minus_one() {
        let cfg = QuantStopConfig {
            cmd: "test".to_string(),
            exit_codes: vec![-1],
            ..Default::default()
        };
        let res = QuantCommandResult {
            exit_code: -1,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: true,
            error: Some("command timed out".to_string()),
        };
        assert!(quant_rule_matches(&cfg, &res).matched);
    }

    // -----------------------------------------------------------------------
    // TempDir helper
    // -----------------------------------------------------------------------

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "{prefix}-{}-{}",
                std::process::id(),
                monotonic_nanos()
            ));
            if let Err(err) = fs::create_dir_all(&path) {
                panic!("failed creating temp dir {}: {err}", path.display());
            }
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn monotonic_nanos() -> u128 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        }
    }
}
