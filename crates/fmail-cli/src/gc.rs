//! fmail gc command ported from Go `internal/fmail/gc.go`.

use crate::{CommandOutput, FmailBackend};

/// Run the gc command from test arguments.
pub fn run_gc_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    run_gc(&owned, backend)
}

fn run_gc(args: &[String], backend: &dyn FmailBackend) -> CommandOutput {
    match execute_gc(args, backend) {
        Ok(output) => output,
        Err((exit_code, message)) => CommandOutput {
            stdout: String::new(),
            stderr: format!("{message}\n"),
            exit_code,
        },
    }
}

#[derive(Debug)]
struct GcArgs {
    days: i64,
    dry_run: bool,
}

fn parse_gc_args(args: &[String]) -> Result<GcArgs, (i32, String)> {
    let mut days: i64 = 7;
    let mut dry_run = false;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--help" | "-h" => {
                let help = concat!(
                    "Usage: fmail gc [flags]\n",
                    "\n",
                    "Remove old messages\n",
                    "\n",
                    "Flags:\n",
                    "  --days int     Remove messages older than N days (default 7)\n",
                    "  --dry-run      Show what would be removed\n",
                );
                return Err((0, help.to_string()));
            }
            "--days" => {
                i += 1;
                if i >= args.len() {
                    return Err((2, "flag --days requires a value".to_string()));
                }
                days = args[i]
                    .parse::<i64>()
                    .map_err(|_| (2, format!("invalid --days value: {}", args[i])))?;
            }
            v if v.starts_with("--days=") => {
                let val = v.strip_prefix("--days=").unwrap_or("");
                days = val
                    .parse::<i64>()
                    .map_err(|_| (2, format!("invalid --days value: {val}")))?;
            }
            "--dry-run" => {
                dry_run = true;
            }
            v if v.starts_with("--") => {
                return Err((2, format!("unknown flag: {v}")));
            }
            _ => {
                return Err((2, "expected 0 arguments".to_string()));
            }
        }
        i += 1;
    }

    if days < 0 {
        return Err((2, "days must be >= 0".to_string()));
    }

    Ok(GcArgs { days, dry_run })
}

fn execute_gc(args: &[String], backend: &dyn FmailBackend) -> Result<CommandOutput, (i32, String)> {
    let parsed = match parse_gc_args(args) {
        Ok(p) => p,
        Err((0, help_text)) => {
            return Ok(CommandOutput {
                stdout: help_text,
                stderr: String::new(),
                exit_code: 0,
            });
        }
        Err(e) => return Err(e),
    };

    let result = backend
        .gc_messages(parsed.days, parsed.dry_run)
        .map_err(|e| (1, e))?;

    Ok(CommandOutput {
        stdout: result,
        stderr: String::new(),
        exit_code: 0,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::tests_common::MockFmailBackend;

    #[test]
    fn parse_no_args_defaults() {
        let args: Vec<String> = vec![];
        let parsed = parse_gc_args(&args).unwrap();
        assert_eq!(parsed.days, 7);
        assert!(!parsed.dry_run);
    }

    #[test]
    fn parse_days_flag() {
        let args: Vec<String> = vec!["--days".into(), "14".into()];
        let parsed = parse_gc_args(&args).unwrap();
        assert_eq!(parsed.days, 14);
    }

    #[test]
    fn parse_days_eq_flag() {
        let args: Vec<String> = vec!["--days=3".into()];
        let parsed = parse_gc_args(&args).unwrap();
        assert_eq!(parsed.days, 3);
    }

    #[test]
    fn parse_days_zero() {
        let args: Vec<String> = vec!["--days".into(), "0".into()];
        let parsed = parse_gc_args(&args).unwrap();
        assert_eq!(parsed.days, 0);
    }

    #[test]
    fn parse_negative_days_fails() {
        let args: Vec<String> = vec!["--days".into(), "-1".into()];
        let err = parse_gc_args(&args).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("days must be >= 0"));
    }

    #[test]
    fn parse_dry_run_flag() {
        let args: Vec<String> = vec!["--dry-run".into()];
        let parsed = parse_gc_args(&args).unwrap();
        assert!(parsed.dry_run);
    }

    #[test]
    fn parse_combined_flags() {
        let args: Vec<String> = vec!["--days".into(), "30".into(), "--dry-run".into()];
        let parsed = parse_gc_args(&args).unwrap();
        assert_eq!(parsed.days, 30);
        assert!(parsed.dry_run);
    }

    #[test]
    fn parse_unknown_flag() {
        let args: Vec<String> = vec!["--verbose".into()];
        let err = parse_gc_args(&args).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("unknown flag"));
    }

    #[test]
    fn parse_positional_arg_rejected() {
        let args: Vec<String> = vec!["something".into()];
        let err = parse_gc_args(&args).unwrap_err();
        assert_eq!(err.0, 2);
    }

    #[test]
    fn parse_days_missing_value() {
        let args: Vec<String> = vec!["--days".into()];
        let err = parse_gc_args(&args).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("requires a value"));
    }

    #[test]
    fn parse_days_invalid_value() {
        let args: Vec<String> = vec!["--days".into(), "abc".into()];
        let err = parse_gc_args(&args).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("invalid --days value"));
    }

    #[test]
    fn parse_help_flag() {
        let args: Vec<String> = vec!["--help".into()];
        let err = parse_gc_args(&args).unwrap_err();
        assert_eq!(err.0, 0);
    }

    #[test]
    fn gc_default_success() {
        let backend = MockFmailBackend::new();
        let result = run_gc_for_test(&[], &backend);
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn gc_dry_run_shows_files() {
        let mut backend = MockFmailBackend::new();
        backend.gc_result = Some("topics/task/20260101-120000-0001.json\n".to_string());
        let result = run_gc_for_test(&["--dry-run"], &backend);
        assert_eq!(result.exit_code, 0);
        assert!(result
            .stdout
            .contains("topics/task/20260101-120000-0001.json"));
    }

    #[test]
    fn gc_help_output() {
        let backend = MockFmailBackend::new();
        let result = run_gc_for_test(&["--help"], &backend);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Remove old messages"));
    }

    #[test]
    fn gc_negative_days_error() {
        let backend = MockFmailBackend::new();
        let result = run_gc_for_test(&["--days", "-1"], &backend);
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("days must be >= 0"));
    }
}
