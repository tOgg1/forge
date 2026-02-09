//! fmail init command ported from Go `internal/fmail/init.go`.

use crate::{CommandOutput, FmailBackend};

/// Run the init command from test arguments.
pub fn run_init_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    run_init(&owned, backend)
}

fn run_init(args: &[String], backend: &dyn FmailBackend) -> CommandOutput {
    match execute_init(args, backend) {
        Ok(output) => output,
        Err((exit_code, message)) => CommandOutput {
            stdout: String::new(),
            stderr: format!("{message}\n"),
            exit_code,
        },
    }
}

#[derive(Debug)]
struct InitArgs {
    project: Option<String>,
    project_flag_set: bool,
}

fn parse_init_args(args: &[String]) -> Result<InitArgs, (i32, String)> {
    let mut project: Option<String> = None;
    let mut project_flag_set = false;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--help" | "-h" => {
                let help = concat!(
                    "Usage: fmail init [flags]\n",
                    "\n",
                    "Initialize a project mailbox\n",
                    "\n",
                    "Flags:\n",
                    "  --project string   Explicit project ID\n",
                );
                return Err((
                    0,
                    // Use exit_code 0 but put help in stdout — handled below
                    help.to_string(),
                ));
            }
            "--project" => {
                project_flag_set = true;
                i += 1;
                if i >= args.len() {
                    return Err((2, "flag --project requires a value".to_string()));
                }
                project = Some(args[i].clone());
            }
            v if v.starts_with("--project=") => {
                project_flag_set = true;
                let val = v.strip_prefix("--project=").unwrap_or("");
                project = Some(val.to_string());
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

    Ok(InitArgs {
        project,
        project_flag_set,
    })
}

fn execute_init(
    args: &[String],
    backend: &dyn FmailBackend,
) -> Result<CommandOutput, (i32, String)> {
    let parsed = match parse_init_args(args) {
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

    // Validate --project flag: if set, value must not be empty after trimming
    let project_id_override = if parsed.project_flag_set {
        let val = parsed.project.as_deref().unwrap_or("").trim().to_string();
        if val.is_empty() {
            return Err((2, "project id is required".to_string()));
        }
        Some(val)
    } else {
        None
    };

    // Init the store via backend
    backend
        .init_project(project_id_override.as_deref())
        .map_err(|e| (1, e))?;

    Ok(CommandOutput {
        stdout: String::new(),
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
    fn parse_no_args() {
        let args: Vec<String> = vec![];
        let parsed = parse_init_args(&args).unwrap();
        assert!(!parsed.project_flag_set);
        assert!(parsed.project.is_none());
    }

    #[test]
    fn parse_project_flag() {
        let args: Vec<String> = vec!["--project".into(), "my-proj".into()];
        let parsed = parse_init_args(&args).unwrap();
        assert!(parsed.project_flag_set);
        assert_eq!(parsed.project.as_deref(), Some("my-proj"));
    }

    #[test]
    fn parse_project_flag_eq() {
        let args: Vec<String> = vec!["--project=my-proj".into()];
        let parsed = parse_init_args(&args).unwrap();
        assert!(parsed.project_flag_set);
        assert_eq!(parsed.project.as_deref(), Some("my-proj"));
    }

    #[test]
    fn parse_project_flag_missing_value() {
        let args: Vec<String> = vec!["--project".into()];
        let err = parse_init_args(&args).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("requires a value"));
    }

    #[test]
    fn parse_unknown_flag() {
        let args: Vec<String> = vec!["--foo".into()];
        let err = parse_init_args(&args).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("unknown flag"));
    }

    #[test]
    fn parse_positional_arg_rejected() {
        let args: Vec<String> = vec!["something".into()];
        let err = parse_init_args(&args).unwrap_err();
        assert_eq!(err.0, 2);
    }

    #[test]
    fn parse_help_flag() {
        let args: Vec<String> = vec!["--help".into()];
        let err = parse_init_args(&args).unwrap_err();
        assert_eq!(err.0, 0); // help is exit 0
    }

    #[test]
    fn init_no_args_success() {
        let backend = MockFmailBackend::new();
        let result = run_init_for_test(&[], &backend);
        assert_eq!(result.exit_code, 0);
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn init_with_project_flag() {
        let backend = MockFmailBackend::new();
        let result = run_init_for_test(&["--project", "custom-id"], &backend);
        assert_eq!(result.exit_code, 0);
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn init_empty_project_flag_fails() {
        let backend = MockFmailBackend::new();
        let result = run_init_for_test(&["--project", "  "], &backend);
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("project id is required"));
    }

    #[test]
    fn init_empty_project_flag_eq_fails() {
        let backend = MockFmailBackend::new();
        let result = run_init_for_test(&["--project="], &backend);
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("project id is required"));
    }

    #[test]
    fn init_help_output() {
        let backend = MockFmailBackend::new();
        let result = run_init_for_test(&["--help"], &backend);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Initialize a project mailbox"));
    }
}
