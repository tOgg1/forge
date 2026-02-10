use std::io::Write;

use fmail_core::agent_registry::AgentRecord;
use tabwriter::TabWriter;

use crate::{CommandOutput, FmailBackend};

pub fn run_who_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let mut json = false;
    let mut positional = 0usize;
    for arg in args {
        match *arg {
            "-h" | "--help" | "help" => {
                return CommandOutput {
                    stdout: format!("{HELP_TEXT}\n"),
                    stderr: String::new(),
                    exit_code: 0,
                };
            }
            "--json" => json = true,
            "" => {}
            v if v.starts_with('-') => {
                return CommandOutput {
                    stdout: String::new(),
                    stderr: format!("unknown flag: {v}\n"),
                    exit_code: 2,
                };
            }
            _ => {
                positional += 1;
            }
        }
    }
    if positional > 0 {
        return CommandOutput {
            stdout: String::new(),
            stderr: format!("expected at most 0 args, got {positional}\n"),
            exit_code: 2,
        };
    }

    let now = backend.now_utc();
    let records = match backend.list_agent_records() {
        Ok(v) => v,
        Err(e) => {
            return CommandOutput {
                stdout: String::new(),
                stderr: format!("{e}\n"),
                exit_code: 1,
            };
        }
    };

    if json {
        let stdout = match records {
            None => "null\n".to_string(),
            Some(v) => {
                // Go parity: json.MarshalIndent(..., "", "  ")
                let encoded =
                    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "null".to_string());
                format!("{encoded}\n")
            }
        };
        return CommandOutput {
            stdout,
            stderr: String::new(),
            exit_code: 0,
        };
    }

    let stdout = format_who_table(now, records.unwrap_or_default());
    CommandOutput {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}

fn format_who_table(now: chrono::DateTime<chrono::Utc>, mut records: Vec<AgentRecord>) -> String {
    records.sort_by(|a, b| a.name.cmp(&b.name));
    let mut tw = TabWriter::new(Vec::new());
    let _ = writeln!(&mut tw, "NAME\tLAST SEEN\tSTATUS");
    for record in records {
        let mut status = record.status.unwrap_or_default();
        status = status.trim().to_string();

        if status.is_empty() && !fmail_core::format::is_active(now, record.last_seen) {
            status = "offline".to_string();
        }
        if status.is_empty() {
            status = "-".to_string();
        }

        let last_seen = fmail_core::format::format_last_seen(now, record.last_seen);
        let _ = writeln!(&mut tw, "{}\t{}\t{}", record.name, last_seen, status);
    }

    let bytes = tabwriter_into_bytes(tw);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn tabwriter_into_bytes(mut tw: TabWriter<Vec<u8>>) -> Vec<u8> {
    loop {
        match tw.into_inner() {
            Ok(v) => return v,
            Err(e) => tw = e.into_inner(),
        }
    }
}

const HELP_TEXT: &str = "\
List known agents

Usage:
  fmail who [flags]

Flags:
  -h, --help   help for who
      --json   Output as JSON";
