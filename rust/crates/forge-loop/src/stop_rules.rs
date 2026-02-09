use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

pub const STOP_WHEN_BEFORE: &str = "before";
pub const STOP_WHEN_AFTER: &str = "after";
pub const STOP_WHEN_BOTH: &str = "both";

pub const STOP_DECISION_STOP: &str = "stop";
pub const STOP_DECISION_CONTINUE: &str = "continue";

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
        Some("0") => Some(0),
        Some("1") => Some(1),
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::{
        normalize_decision, parse_qual_signal, quant_every_matches, quant_should_evaluate,
        quant_when_matches, run_quant_command, STOP_DECISION_CONTINUE, STOP_DECISION_STOP,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        assert_eq!(parse_qual_signal("0"), Some(0));
        assert_eq!(parse_qual_signal("1"), Some(1));
        assert_eq!(parse_qual_signal("0 stop"), Some(0));
        assert_eq!(parse_qual_signal("1 continue"), Some(1));
        assert_eq!(parse_qual_signal(""), None);
        assert_eq!(parse_qual_signal("   "), None);
        assert_eq!(parse_qual_signal("2"), None);
        assert_eq!(parse_qual_signal("01"), None);
        assert_eq!(parse_qual_signal("continue"), None);
    }

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
