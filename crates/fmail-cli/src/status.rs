use crate::{CommandOutput, FmailBackend};

pub fn run_status_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let mut clear = false;
    let mut message: Option<String> = None;

    for arg in args {
        match *arg {
            "-h" | "--help" | "help" => {
                return CommandOutput {
                    stdout: format!("{HELP_TEXT}\n"),
                    stderr: String::new(),
                    exit_code: 0,
                };
            }
            "--clear" => clear = true,
            "" => {}
            v if v.starts_with('-') => {
                return CommandOutput {
                    stdout: String::new(),
                    stderr: format!("unknown flag: {v}\n"),
                    exit_code: 2,
                };
            }
            v => {
                if message.is_some() {
                    return CommandOutput {
                        stdout: String::new(),
                        stderr: "expected at most 1 argument\n".to_string(),
                        exit_code: 2,
                    };
                }
                message = Some(v.to_string());
            }
        }
    }

    if clear && message.is_some() {
        return CommandOutput {
            stdout: String::new(),
            stderr: "status does not take a message with --clear\n".to_string(),
            exit_code: 2,
        };
    }

    let agent = match backend.agent_name() {
        Ok(v) => v,
        Err(e) => {
            return CommandOutput {
                stdout: String::new(),
                stderr: format!("{e}\n"),
                exit_code: 1,
            };
        }
    };

    if message.is_none() && !clear {
        let record = match backend.read_agent_record(&agent) {
            Ok(v) => v,
            Err(e) => {
                return CommandOutput {
                    stdout: String::new(),
                    stderr: format!("load status: {e}\n"),
                    exit_code: 1,
                };
            }
        };

        let status = record
            .and_then(|r| r.status)
            .unwrap_or_default()
            .trim()
            .to_string();

        if status.is_empty() {
            return CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 0,
            };
        }

        return CommandOutput {
            stdout: format!("{status}\n"),
            stderr: String::new(),
            exit_code: 0,
        };
    }

    let status = if clear {
        String::new()
    } else {
        let v = message.unwrap_or_default();
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            return CommandOutput {
                stdout: String::new(),
                stderr: "status message is required\n".to_string(),
                exit_code: 2,
            };
        }
        trimmed
    };

    let host = backend.hostname();
    if let Err(e) = backend.set_agent_status(&agent, &status, &host) {
        return CommandOutput {
            stdout: String::new(),
            stderr: format!("update status: {e}\n"),
            exit_code: 1,
        };
    }

    CommandOutput {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 0,
    }
}

const HELP_TEXT: &str = "\
Show or set your status

Usage:
  fmail status [message] [--clear]

Examples:
  fmail status                 # Show your current status
  fmail status \"working on auth\"
  fmail status --clear";
