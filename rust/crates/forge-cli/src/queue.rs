use std::collections::HashMap;
use std::io::Write;

use serde::Serialize;

mod sqlite_backend;
pub use sqlite_backend::SqliteQueueBackend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub status: String,
    pub position: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
}

pub trait QueueBackend {
    fn resolve_loop(&self, loop_ref: &str) -> Result<LoopRecord, String>;
    fn list_queue(&self, loop_id: &str) -> Result<Vec<QueueItem>, String>;
    fn clear_pending(&mut self, loop_id: &str) -> Result<usize, String>;
    fn remove_item(&mut self, loop_id: &str, item_id: &str) -> Result<(), String>;
    fn move_item(&mut self, loop_id: &str, item_id: &str, to: &str) -> Result<(), String>;
}

pub(crate) fn resolve_loop_ref(loops: &[LoopRecord], loop_ref: &str) -> Result<LoopRecord, String> {
    let trimmed = loop_ref.trim();
    if trimmed.is_empty() {
        return Err("loop name or ID required".to_string());
    }
    if loops.is_empty() {
        return Err(format!(
            "loop '{trimmed}' not found (no loops registered yet)"
        ));
    }

    if let Some(entry) = loops
        .iter()
        .find(|entry| entry.short_id.eq_ignore_ascii_case(trimmed))
    {
        return Ok(entry.clone());
    }
    if let Some(entry) = loops.iter().find(|entry| entry.id == trimmed) {
        return Ok(entry.clone());
    }
    if let Some(entry) = loops.iter().find(|entry| entry.name == trimmed) {
        return Ok(entry.clone());
    }

    let normalized = trimmed.to_ascii_lowercase();
    let mut matches: Vec<LoopRecord> = loops
        .iter()
        .filter(|entry| {
            entry.short_id.to_ascii_lowercase().starts_with(&normalized)
                || entry.id.starts_with(trimmed)
        })
        .cloned()
        .collect();

    if matches.len() == 1 {
        return Ok(matches.remove(0));
    }
    if matches.len() > 1 {
        matches.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.short_id.cmp(&right.short_id))
        });
        let labels = matches
            .iter()
            .map(format_loop_match)
            .collect::<Vec<String>>()
            .join(", ");
        return Err(format!(
            "loop '{trimmed}' is ambiguous; matches: {labels} (use a longer prefix or full ID)"
        ));
    }

    let example = &loops[0];
    Err(format!(
        "loop '{trimmed}' not found. Example input: '{}' or '{}'",
        example.name, example.short_id
    ))
}

fn format_loop_match(entry: &LoopRecord) -> String {
    format!("{} ({})", entry.name, entry.short_id)
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryQueueBackend {
    loops: Vec<LoopRecord>,
    queue_by_loop: HashMap<String, Vec<QueueItem>>,
}

impl InMemoryQueueBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        Self {
            loops,
            queue_by_loop: HashMap::new(),
        }
    }

    pub fn seed_queue(&mut self, loop_id: &str, items: Vec<QueueItem>) {
        let mut indexed = items;
        for (index, item) in indexed.iter_mut().enumerate() {
            item.position = (index + 1) as i64;
        }
        self.queue_by_loop.insert(loop_id.to_string(), indexed);
    }
}

impl QueueBackend for InMemoryQueueBackend {
    fn resolve_loop(&self, loop_ref: &str) -> Result<LoopRecord, String> {
        resolve_loop_ref(&self.loops, loop_ref)
    }

    fn list_queue(&self, loop_id: &str) -> Result<Vec<QueueItem>, String> {
        let mut items = self.queue_by_loop.get(loop_id).cloned().unwrap_or_default();
        items.sort_by_key(|item| item.position);
        Ok(items)
    }

    fn clear_pending(&mut self, loop_id: &str) -> Result<usize, String> {
        let Some(items) = self.queue_by_loop.get_mut(loop_id) else {
            return Ok(0);
        };
        let before = items.len();
        items.retain(|item| item.status != "pending");
        for (index, item) in items.iter_mut().enumerate() {
            item.position = (index + 1) as i64;
        }
        Ok(before.saturating_sub(items.len()))
    }

    fn remove_item(&mut self, loop_id: &str, item_id: &str) -> Result<(), String> {
        let Some(items) = self.queue_by_loop.get_mut(loop_id) else {
            return Err("queue item not found in loop".to_string());
        };
        let Some(index) = items.iter().position(|item| item.id == item_id) else {
            return Err("queue item not found in loop".to_string());
        };
        items.remove(index);
        for (new_position, item) in items.iter_mut().enumerate() {
            item.position = (new_position + 1) as i64;
        }
        Ok(())
    }

    fn move_item(&mut self, loop_id: &str, item_id: &str, to: &str) -> Result<(), String> {
        let Some(items) = self.queue_by_loop.get_mut(loop_id) else {
            return Err("no pending items".to_string());
        };

        let pending_ids: Vec<String> = items
            .iter()
            .filter(|item| item.status == "pending")
            .map(|item| item.id.clone())
            .collect();
        if pending_ids.is_empty() {
            return Err("no pending items".to_string());
        }

        if !pending_ids.iter().any(|id| id == item_id) {
            return Err("queue item not found".to_string());
        }

        let mut reordered_pending = pending_ids;
        let Some(index) = reordered_pending.iter().position(|id| id == item_id) else {
            return Err("queue item not found".to_string());
        };
        reordered_pending.remove(index);
        match to {
            "front" => reordered_pending.insert(0, item_id.to_string()),
            "back" => reordered_pending.push(item_id.to_string()),
            other => return Err(format!("unknown move target '{other}'")),
        }

        let mut position_by_id: HashMap<String, i64> = HashMap::new();
        for (position, id) in reordered_pending.iter().enumerate() {
            position_by_id.insert(id.clone(), (position + 1) as i64);
        }

        let pending_len = reordered_pending.len() as i64;
        for item in items.iter_mut() {
            if let Some(position) = position_by_id.get(&item.id) {
                item.position = *position;
            } else if item.status != "pending" {
                item.position = pending_len + 1;
            }
        }
        items.sort_by_key(|item| item.position);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Help,
    List {
        loop_ref: String,
        include_all: bool,
        json: bool,
        jsonl: bool,
    },
    Clear {
        loop_ref: String,
        json: bool,
        jsonl: bool,
        quiet: bool,
    },
    Remove {
        loop_ref: String,
        item_id: String,
        json: bool,
        jsonl: bool,
        quiet: bool,
    },
    Move {
        loop_ref: String,
        item_id: String,
        to: String,
        json: bool,
        jsonl: bool,
        quiet: bool,
    },
}

pub fn run_for_test(args: &[&str], backend: &mut dyn QueueBackend) -> CommandOutput {
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
    backend: &mut dyn QueueBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

fn execute(
    args: &[String],
    backend: &mut dyn QueueBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    match parse_args(args)? {
        Command::Help => {
            write_help(stdout).map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::List {
            loop_ref,
            include_all,
            json,
            jsonl,
        } => {
            let loop_entry = backend.resolve_loop(&loop_ref)?;
            let mut items = backend.list_queue(&loop_entry.id)?;
            if !include_all {
                items.retain(|item| item.status == "pending");
            }
            if json || jsonl {
                write_serialized(stdout, &items, jsonl)?;
                return Ok(());
            }
            if items.is_empty() {
                writeln!(stdout, "No queue items").map_err(|err| err.to_string())?;
                return Ok(());
            }
            for item in items {
                writeln!(
                    stdout,
                    "{}\t{}\t{}\t{}\t{}",
                    item.id, item.item_type, item.status, item.position, item.created_at
                )
                .map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Clear {
            loop_ref,
            json,
            jsonl,
            quiet,
        } => {
            let loop_entry = backend.resolve_loop(&loop_ref)?;
            let cleared = backend.clear_pending(&loop_entry.id)?;
            if json || jsonl {
                let payload = serde_json::json!({ "cleared": cleared });
                write_serialized(stdout, &payload, jsonl)?;
                return Ok(());
            }
            if !quiet {
                writeln!(stdout, "Cleared {} item(s)", cleared).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Remove {
            loop_ref,
            item_id,
            json,
            jsonl,
            quiet,
        } => {
            let loop_entry = backend.resolve_loop(&loop_ref)?;
            backend.remove_item(&loop_entry.id, &item_id)?;
            if json || jsonl {
                let payload = serde_json::json!({ "removed": item_id, "loop": loop_entry.name });
                write_serialized(stdout, &payload, jsonl)?;
                return Ok(());
            }
            if !quiet {
                writeln!(stdout, "Removed item {}", item_id).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Move {
            loop_ref,
            item_id,
            to,
            json,
            jsonl,
            quiet,
        } => {
            let loop_entry = backend.resolve_loop(&loop_ref)?;
            backend.move_item(&loop_entry.id, &item_id, &to)?;
            if json || jsonl {
                let payload = serde_json::json!({ "moved": item_id, "to": to });
                write_serialized(stdout, &payload, jsonl)?;
                return Ok(());
            }
            if !quiet {
                writeln!(stdout, "Moved item {} to {}", item_id, to)
                    .map_err(|err| err.to_string())?;
            }
            Ok(())
        }
    }
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Ok(Command::Help);
    }

    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "queue") {
        index += 1;
    }
    if args
        .get(index)
        .is_some_and(|token| token == "--help" || token == "-h")
    {
        return Ok(Command::Help);
    }

    // Accept global output flags before the subcommand (matches Cobra persistent flags).
    let mut default_json = false;
    let mut default_jsonl = false;
    let mut default_quiet = false;
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => default_json = true,
            "--jsonl" => default_jsonl = true,
            "--quiet" => default_quiet = true,
            "--help" | "-h" => return Ok(Command::Help),
            _ => break,
        }
        index += 1;
    }

    let Some(subcommand) = args.get(index) else {
        return Ok(Command::Help);
    };
    index += 1;

    match subcommand.as_str() {
        "ls" => parse_ls(args, index, default_json, default_jsonl, default_quiet),
        "clear" => parse_clear(args, index, default_json, default_jsonl, default_quiet),
        "rm" => parse_rm(args, index, default_json, default_jsonl, default_quiet),
        "move" => parse_move(args, index, default_json, default_jsonl, default_quiet),
        other => Err(format!("error: unknown queue subcommand '{other}'")),
    }
}

fn parse_ls(
    args: &[String],
    mut index: usize,
    default_json: bool,
    default_jsonl: bool,
    _default_quiet: bool,
) -> Result<Command, String> {
    let loop_ref = take_positional(args, &mut index, "loop")?;
    let mut include_all = false;
    let mut json = default_json;
    let mut jsonl = default_jsonl;
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--all" => include_all = true,
            "--json" => json = true,
            "--jsonl" => jsonl = true,
            // Root global flag; accepted for parity but has no effect on list output.
            "--quiet" => {}
            other => return Err(format!("error: unknown argument for queue ls: '{other}'")),
        }
        index += 1;
    }
    ensure_single_output_mode(json, jsonl)?;
    Ok(Command::List {
        loop_ref,
        include_all,
        json,
        jsonl,
    })
}

fn parse_clear(
    args: &[String],
    mut index: usize,
    default_json: bool,
    default_jsonl: bool,
    default_quiet: bool,
) -> Result<Command, String> {
    let loop_ref = take_positional(args, &mut index, "loop")?;
    let mut json = default_json;
    let mut jsonl = default_jsonl;
    let mut quiet = default_quiet;
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => json = true,
            "--jsonl" => jsonl = true,
            "--quiet" => quiet = true,
            other => {
                return Err(format!(
                    "error: unknown argument for queue clear: '{other}'"
                ))
            }
        }
        index += 1;
    }
    ensure_single_output_mode(json, jsonl)?;
    Ok(Command::Clear {
        loop_ref,
        json,
        jsonl,
        quiet,
    })
}

fn parse_rm(
    args: &[String],
    mut index: usize,
    default_json: bool,
    default_jsonl: bool,
    default_quiet: bool,
) -> Result<Command, String> {
    let loop_ref = take_positional(args, &mut index, "loop")?;
    let item_id = take_positional(args, &mut index, "item-id")?;
    let mut json = default_json;
    let mut jsonl = default_jsonl;
    let mut quiet = default_quiet;
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => json = true,
            "--jsonl" => jsonl = true,
            "--quiet" => quiet = true,
            other => return Err(format!("error: unknown argument for queue rm: '{other}'")),
        }
        index += 1;
    }
    ensure_single_output_mode(json, jsonl)?;
    Ok(Command::Remove {
        loop_ref,
        item_id,
        json,
        jsonl,
        quiet,
    })
}

fn parse_move(
    args: &[String],
    mut index: usize,
    default_json: bool,
    default_jsonl: bool,
    default_quiet: bool,
) -> Result<Command, String> {
    let loop_ref = take_positional(args, &mut index, "loop")?;
    let item_id = take_positional(args, &mut index, "item-id")?;
    let mut json = default_json;
    let mut jsonl = default_jsonl;
    let mut quiet = default_quiet;
    let mut to = "front".to_string();
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => json = true,
            "--jsonl" => jsonl = true,
            "--quiet" => quiet = true,
            "--to" => {
                to = args
                    .get(index + 1)
                    .cloned()
                    .ok_or_else(|| "error: missing value for --to".to_string())?;
                index += 1;
            }
            other => return Err(format!("error: unknown argument for queue move: '{other}'")),
        }
        index += 1;
    }
    ensure_single_output_mode(json, jsonl)?;
    Ok(Command::Move {
        loop_ref,
        item_id,
        to,
        json,
        jsonl,
        quiet,
    })
}

fn ensure_single_output_mode(json: bool, jsonl: bool) -> Result<(), String> {
    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }
    Ok(())
}

fn take_positional(args: &[String], index: &mut usize, label: &str) -> Result<String, String> {
    let value = args
        .get(*index)
        .cloned()
        .ok_or_else(|| format!("error: missing required argument <{label}>"))?;
    *index += 1;
    Ok(value)
}

fn write_help(out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(out, "Manage loop queues")?;
    writeln!(out)?;
    writeln!(out, "Subcommands:")?;
    writeln!(out, "  ls <loop>")?;
    writeln!(out, "  clear <loop>")?;
    writeln!(out, "  rm <loop> <item-id>")?;
    writeln!(out, "  move <loop> <item-id> --to front|back")?;
    Ok(())
}

fn write_serialized(
    stdout: &mut dyn Write,
    value: &impl Serialize,
    jsonl: bool,
) -> Result<(), String> {
    if jsonl {
        serde_json::to_writer(&mut *stdout, value).map_err(|err| err.to_string())?;
    } else {
        serde_json::to_writer_pretty(&mut *stdout, value).map_err(|err| err.to_string())?;
    }
    writeln!(stdout).map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_args, run_for_test, InMemoryQueueBackend, LoopRecord, QueueItem};

    #[test]
    fn parse_queue_help_when_empty() {
        let args: Vec<String> = vec!["queue".to_string()];
        let parsed = parse_args(&args);
        assert!(parsed.is_ok());
    }

    #[test]
    fn parse_rejects_json_and_jsonl() {
        let args = vec![
            "queue".to_string(),
            "ls".to_string(),
            "alpha".to_string(),
            "--json".to_string(),
            "--jsonl".to_string(),
        ];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "error: --json and --jsonl cannot be used together");
    }

    #[test]
    fn move_reorders_pending_items() {
        let loop_entry = LoopRecord {
            id: "loop-1".to_string(),
            short_id: "loop1".to_string(),
            name: "alpha".to_string(),
        };
        let mut backend = InMemoryQueueBackend::with_loops(vec![loop_entry.clone()]);
        backend.seed_queue(
            &loop_entry.id,
            vec![
                QueueItem {
                    id: "q1".to_string(),
                    item_type: "message_append".to_string(),
                    status: "pending".to_string(),
                    position: 1,
                    created_at: "2025-01-01T00:00:00Z".to_string(),
                },
                QueueItem {
                    id: "q2".to_string(),
                    item_type: "stop_graceful".to_string(),
                    status: "pending".to_string(),
                    position: 2,
                    created_at: "2025-01-01T00:00:01Z".to_string(),
                },
            ],
        );
        let out = run_for_test(
            &["queue", "move", "alpha", "q2", "--to", "front", "--json"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(
            out.stdout,
            "{\n  \"moved\": \"q2\",\n  \"to\": \"front\"\n}\n"
        );
    }
}
