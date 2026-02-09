//! fmail topics command ported from Go `internal/fmail/topics.go`.

use crate::{CommandOutput, FmailBackend};

/// Run the topics command from test arguments.
pub fn run_topics_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    run_topics(&owned, backend)
}

fn run_topics(args: &[String], backend: &dyn FmailBackend) -> CommandOutput {
    match execute_topics(args, backend) {
        Ok(output) => output,
        Err((exit_code, message)) => CommandOutput {
            stdout: String::new(),
            stderr: format!("{message}\n"),
            exit_code,
        },
    }
}

fn execute_topics(
    args: &[String],
    backend: &dyn FmailBackend,
) -> Result<CommandOutput, (i32, String)> {
    let mut json = false;

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" | "help" => return Err((0, HELP_TEXT.to_string())),
            "--json" => json = true,
            flag if flag.starts_with('-') => {
                return Err((2, format!("unknown flag: {flag}")));
            }
            _ => return Err((2, "topics takes no arguments".to_string())),
        }
    }

    let topics = backend
        .list_topics()
        .map_err(|e| (1, format!("list topics: {e}")))?;
    let now = backend.now_utc();

    if json {
        let data = serde_json::to_string_pretty(&topics)
            .map_err(|e| (1, format!("encode topics: {e}")))?;
        return Ok(CommandOutput {
            stdout: format!("{data}\n"),
            stderr: String::new(),
            exit_code: 0,
        });
    }

    // Text output with tabwriter-style formatting
    let mut out = String::new();
    out.push_str("TOPIC\tMESSAGES\tLAST ACTIVITY\n");
    for topic in &topics {
        let last = match &topic.last_activity {
            Some(t) => fmail_core::format::format_relative(now, *t),
            None => "-".to_string(),
        };
        out.push_str(&format!("{}\t{}\t{}\n", topic.name, topic.messages, last));
    }

    // Format with tabwriter
    let formatted = format_tab_separated(&out);

    Ok(CommandOutput {
        stdout: formatted,
        stderr: String::new(),
        exit_code: 0,
    })
}

/// Simple tab-to-aligned-columns formatter.
fn format_tab_separated(input: &str) -> String {
    let lines: Vec<Vec<&str>> = input
        .lines()
        .map(|line| line.split('\t').collect())
        .collect();

    if lines.is_empty() {
        return String::new();
    }

    // Calculate max column widths
    let max_cols = lines.iter().map(|row| row.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; max_cols];
    for row in &lines {
        for (i, cell) in row.iter().enumerate() {
            if cell.len() > widths[i] {
                widths[i] = cell.len();
            }
        }
    }

    let mut result = String::new();
    for row in &lines {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                result.push_str("  "); // 2-space separator
            }
            if i < row.len() - 1 {
                // Left-pad all but last column
                result.push_str(&format!("{:<width$}", cell, width = widths[i]));
            } else {
                result.push_str(cell);
            }
        }
        result.push('\n');
    }
    result
}

const HELP_TEXT: &str = "\
List topics with activity

Usage:
  fmail topics [flags]

Flags:
      --json    Output as JSON
  -h, --help    Help for topics";
