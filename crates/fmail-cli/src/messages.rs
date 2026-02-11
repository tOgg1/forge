//! fmail messages command ported from Go `internal/fmail/messages.go`.

use crate::{CommandOutput, FmailBackend};

/// Run the messages command from test arguments.
pub fn run_messages_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    run_messages(&owned, backend)
}

fn run_messages(args: &[String], backend: &dyn FmailBackend) -> CommandOutput {
    match crate::log::execute_log(args, backend, true) {
        Ok(output) => output,
        Err((exit_code, message)) => CommandOutput {
            stdout: String::new(),
            stderr: format!("{message}\n"),
            exit_code,
        },
    }
}
