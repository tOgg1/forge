use std::collections::BTreeMap;
use std::env;
use std::io::Write;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoopKVEntry {
    pub created_at: String,
    pub id: String,
    pub key: String,
    pub loop_id: String,
    pub updated_at: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopEntry {
    pub id: String,
    pub name: String,
}

pub trait MemBackend {
    fn resolve_loop_by_ref(&self, loop_ref: &str) -> Result<LoopEntry, String>;
    fn set(&mut self, loop_id: &str, key: &str, value: &str) -> Result<(), String>;
    fn get(&self, loop_id: &str, key: &str) -> Result<LoopKVEntry, String>;
    fn list_by_loop(&self, loop_id: &str) -> Result<Vec<LoopKVEntry>, String>;
    fn delete(&mut self, loop_id: &str, key: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryMemBackend {
    loops_by_id: BTreeMap<String, String>,
    loops_by_name: BTreeMap<String, String>,
    records: BTreeMap<String, BTreeMap<String, LoopKVEntry>>,
    next_id: usize,
    tick: usize,
}

impl InMemoryMemBackend {
    pub fn seed_loop(&mut self, loop_id: &str, loop_name: &str) {
        self.loops_by_id
            .insert(loop_id.to_string(), loop_name.to_string());
        self.loops_by_name
            .insert(loop_name.to_string(), loop_id.to_string());
    }

    fn next_identifier(&mut self) -> String {
        self.next_id += 1;
        format!("kv-{:03}", self.next_id)
    }

    fn next_timestamp(&mut self) -> String {
        self.tick += 1;
        let minutes = (self.tick / 60) % 60;
        let seconds = self.tick % 60;
        format!("2026-01-01T00:{minutes:02}:{seconds:02}Z")
    }
}

impl MemBackend for InMemoryMemBackend {
    fn resolve_loop_by_ref(&self, loop_ref: &str) -> Result<LoopEntry, String> {
        let trimmed = loop_ref.trim();
        if trimmed.is_empty() {
            return Err("loop ref is required".to_string());
        }
        if let Some(name) = self.loops_by_id.get(trimmed) {
            return Ok(LoopEntry {
                id: trimmed.to_string(),
                name: name.clone(),
            });
        }
        if let Some(id) = self.loops_by_name.get(trimmed) {
            return Ok(LoopEntry {
                id: id.clone(),
                name: trimmed.to_string(),
            });
        }
        Err(format!("loop not found: {trimmed}"))
    }

    fn set(&mut self, loop_id: &str, key: &str, value: &str) -> Result<(), String> {
        let loop_id = loop_id.trim();
        let key = key.trim();
        if loop_id.is_empty() {
            return Err("loop_id is required".to_string());
        }
        if key.is_empty() {
            return Err("key is required".to_string());
        }
        if value.is_empty() {
            return Err("value is required".to_string());
        }

        let stamp = self.next_timestamp();
        if let Some(loop_bucket) = self.records.get_mut(loop_id) {
            if let Some(existing) = loop_bucket.get_mut(key) {
                existing.value = value.to_string();
                existing.updated_at = stamp;
                return Ok(());
            }
        }

        let id = self.next_identifier();
        self.records.entry(loop_id.to_string()).or_default().insert(
            key.to_string(),
            LoopKVEntry {
                created_at: stamp.clone(),
                id,
                key: key.to_string(),
                loop_id: loop_id.to_string(),
                updated_at: stamp,
                value: value.to_string(),
            },
        );
        Ok(())
    }

    fn get(&self, loop_id: &str, key: &str) -> Result<LoopKVEntry, String> {
        let loop_id = loop_id.trim();
        let key = key.trim();
        let Some(loop_bucket) = self.records.get(loop_id) else {
            return Err("loop kv not found".to_string());
        };
        let Some(entry) = loop_bucket.get(key) else {
            return Err("loop kv not found".to_string());
        };
        Ok(entry.clone())
    }

    fn list_by_loop(&self, loop_id: &str) -> Result<Vec<LoopKVEntry>, String> {
        let loop_id = loop_id.trim();
        let items = self
            .records
            .get(loop_id)
            .map(|bucket| bucket.values().cloned().collect())
            .unwrap_or_default();
        Ok(items)
    }

    fn delete(&mut self, loop_id: &str, key: &str) -> Result<(), String> {
        let loop_id = loop_id.trim();
        let key = key.trim();
        let Some(loop_bucket) = self.records.get_mut(loop_id) else {
            return Err("loop kv not found".to_string());
        };
        if loop_bucket.remove(key).is_none() {
            return Err("loop kv not found".to_string());
        }
        if loop_bucket.is_empty() {
            self.records.remove(loop_id);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Help,
    Set { key: String, value: String },
    Get { key: String },
    List,
    Remove { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    command: Command,
    loop_ref: Option<String>,
    json: bool,
    jsonl: bool,
    quiet: bool,
}

pub fn run_from_env_with_backend(backend: &mut dyn MemBackend) -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_backend(&args, backend, &mut stdout, &mut stderr)
}

pub fn run_for_test(args: &[&str], backend: &mut dyn MemBackend) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_with_backend(&owned_args, backend, &mut stdout, &mut stderr);
    let stdout = match String::from_utf8(stdout) {
        Ok(value) => value,
        Err(err) => panic!("stdout should be utf-8: {err}"),
    };
    let stderr = match String::from_utf8(stderr) {
        Ok(value) => value,
        Err(err) => panic!("stderr should be utf-8: {err}"),
    };
    CommandOutput {
        stdout,
        stderr,
        exit_code,
    }
}

pub fn run_with_backend(
    args: &[String],
    backend: &mut dyn MemBackend,
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
    backend: &mut dyn MemBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    match parsed.command {
        Command::Help => {
            write_help(stdout).map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Set { key, value } => {
            let loop_ref = require_loop_ref(parsed.loop_ref.as_deref())?;
            let loop_entry = backend.resolve_loop_by_ref(&loop_ref)?;
            backend.set(&loop_entry.id, &key, &value)?;

            if parsed.json || parsed.jsonl {
                let payload = serde_json::json!({
                    "loop": loop_entry.name,
                    "key": key,
                    "ok": true
                });
                write_serialized(stdout, &payload, parsed.jsonl)?;
            } else if !parsed.quiet {
                writeln!(stdout, "ok").map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Get { key } => {
            let loop_ref = require_loop_ref(parsed.loop_ref.as_deref())?;
            let loop_entry = backend.resolve_loop_by_ref(&loop_ref)?;
            let entry = backend.get(&loop_entry.id, &key)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &entry, parsed.jsonl)?;
            } else {
                writeln!(stdout, "{}", entry.value).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::List => {
            let loop_ref = require_loop_ref(parsed.loop_ref.as_deref())?;
            let loop_entry = backend.resolve_loop_by_ref(&loop_ref)?;
            let items = backend.list_by_loop(&loop_entry.id)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &items, parsed.jsonl)?;
                return Ok(());
            }
            if items.is_empty() {
                writeln!(stdout, "(empty)").map_err(|err| err.to_string())?;
                return Ok(());
            }
            for item in items {
                writeln!(stdout, "{}={}", item.key, item.value).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Remove { key } => {
            let loop_ref = require_loop_ref(parsed.loop_ref.as_deref())?;
            let loop_entry = backend.resolve_loop_by_ref(&loop_ref)?;
            backend.delete(&loop_entry.id, &key)?;
            if parsed.json || parsed.jsonl {
                let payload = serde_json::json!({
                    "loop": loop_entry.name,
                    "key": key,
                    "ok": true
                });
                write_serialized(stdout, &payload, parsed.jsonl)?;
            } else if !parsed.quiet {
                writeln!(stdout, "ok").map_err(|err| err.to_string())?;
            }
            Ok(())
        }
    }
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    if args.is_empty() {
        return Ok(ParsedArgs {
            command: Command::Help,
            loop_ref: None,
            json: false,
            jsonl: false,
            quiet: false,
        });
    }

    let start = if args.first().is_some_and(|arg| arg == "mem") {
        1
    } else {
        0
    };

    let mut loop_ref: Option<String> = None;
    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
    let mut subcommand: Option<String> = None;
    let mut subcommand_args: Vec<String> = Vec::new();

    let mut idx = start;
    while idx < args.len() {
        let token = args[idx].as_str();
        match token {
            "--loop" => {
                let value = next_value(args, idx, "--loop")?;
                loop_ref = Some(value.to_string());
                idx += 2;
                continue;
            }
            "--json" => {
                json = true;
                idx += 1;
                continue;
            }
            "--jsonl" => {
                jsonl = true;
                idx += 1;
                continue;
            }
            "--quiet" => {
                quiet = true;
                idx += 1;
                continue;
            }
            _ => {}
        }

        if subcommand.is_none() {
            subcommand = Some(args[idx].clone());
        } else {
            subcommand_args.push(args[idx].clone());
        }
        idx += 1;
    }

    let command = match subcommand.as_deref() {
        None | Some("help") | Some("-h") | Some("--help") => Command::Help,
        Some("set") => parse_set_args(&subcommand_args)?,
        Some("get") => parse_get_args(&subcommand_args)?,
        Some("ls") | Some("list") => {
            ensure_empty_args("mem ls", &subcommand_args)?;
            Command::List
        }
        Some("rm") | Some("remove") => parse_rm_args(&subcommand_args)?,
        Some(other) => return Err(format!("unknown mem argument: {other}")),
    };

    Ok(ParsedArgs {
        command,
        loop_ref,
        json,
        jsonl,
        quiet,
    })
}

fn parse_set_args(args: &[String]) -> Result<Command, String> {
    let mut key: Option<String> = None;
    let mut value: Option<String> = None;
    for token in args {
        if token.starts_with("--") {
            return Err(format!("unknown mem set flag: {token}"));
        }
        if key.is_none() {
            key = Some(token.to_string());
            continue;
        }
        if value.is_none() {
            value = Some(token.to_string());
            continue;
        }
        return Err(format!("unexpected argument for mem set: {token}"));
    }

    let key = match key {
        Some(v) => v,
        None => return Err("mem set requires <key> <value>".to_string()),
    };
    let value = match value {
        Some(v) => v,
        None => return Err("mem set requires <key> <value>".to_string()),
    };
    Ok(Command::Set { key, value })
}

fn parse_get_args(args: &[String]) -> Result<Command, String> {
    match args.first() {
        Some(key) => {
            if args.len() > 1 {
                return Err(format!("unexpected argument for mem get: {}", args[1]));
            }
            Ok(Command::Get { key: key.clone() })
        }
        None => Err("mem get requires <key>".to_string()),
    }
}

fn parse_rm_args(args: &[String]) -> Result<Command, String> {
    match args.first() {
        Some(key) => {
            if args.len() > 1 {
                return Err(format!("unexpected argument for mem rm: {}", args[1]));
            }
            Ok(Command::Remove { key: key.clone() })
        }
        None => Err("mem rm requires <key>".to_string()),
    }
}

fn ensure_empty_args(command: &str, args: &[String]) -> Result<(), String> {
    if let Some(first) = args.first() {
        return Err(format!("unexpected argument for {command}: {first}"));
    }
    Ok(())
}

fn next_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    let next = index + 1;
    match args.get(next) {
        Some(value) => Ok(value.as_str()),
        None => Err(format!("{flag} requires a value")),
    }
}

fn require_loop_ref(explicit: Option<&str>) -> Result<String, String> {
    if let Some(value) = explicit {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Ok(value) = env::var("FORGE_LOOP_ID") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Ok(value) = env::var("FORGE_LOOP_NAME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    Err("loop required (pass --loop or set FORGE_LOOP_ID)".to_string())
}

fn sort_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = map.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut out = serde_json::Map::new();
            for (k, v) in entries {
                out.insert(k, sort_json_value(v));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_json_value).collect())
        }
        other => other,
    }
}

fn write_serialized(
    output: &mut dyn Write,
    value: &impl Serialize,
    jsonl: bool,
) -> Result<(), String> {
    let mut as_value = serde_json::to_value(value).map_err(|err| err.to_string())?;
    as_value = sort_json_value(as_value);
    if jsonl {
        if let serde_json::Value::Array(items) = as_value {
            for item in items {
                let item = sort_json_value(item);
                let line = serde_json::to_string(&item).map_err(|err| err.to_string())?;
                writeln!(output, "{line}").map_err(|err| err.to_string())?;
            }
            return Ok(());
        }
        let line = serde_json::to_string(&as_value).map_err(|err| err.to_string())?;
        writeln!(output, "{line}").map_err(|err| err.to_string())?;
        return Ok(());
    }

    let text = serde_json::to_string_pretty(&as_value).map_err(|err| err.to_string())?;
    write!(output, "{text}\n\n").map_err(|err| err.to_string())?;
    Ok(())
}

fn write_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "forge mem - Persistent per-loop key/value memory")?;
    writeln!(stdout)?;
    writeln!(stdout, "Usage:")?;
    writeln!(stdout, "  forge mem [--loop <ref>] <command> [options]")?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  set <key> <value>  Set a memory key")?;
    writeln!(stdout, "  get <key>          Get a memory key")?;
    writeln!(stdout, "  ls                 List memory keys")?;
    writeln!(stdout, "  rm <key>           Remove a memory key")?;
    writeln!(stdout)?;
    writeln!(stdout, "Flags:")?;
    writeln!(
        stdout,
        "  --loop <ref>  loop ref (defaults to FORGE_LOOP_ID/FORGE_LOOP_NAME)"
    )?;
    writeln!(stdout, "  --json        output JSON")?;
    writeln!(stdout, "  --jsonl       output JSON lines")?;
    writeln!(
        stdout,
        "  --quiet       suppress human output for mutating commands"
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::sync::{Mutex, OnceLock};

    use super::{run_for_test, InMemoryMemBackend};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvRestore {
        saved: Vec<(String, Option<String>)>,
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in self.saved.drain(..).rev() {
                if let Some(current) = value {
                    env::set_var(key, current);
                } else {
                    env::remove_var(key);
                }
            }
        }
    }

    fn apply_env(updates: &[(&str, Option<&str>)]) -> EnvRestore {
        let mut saved = Vec::new();
        for (key, value) in updates {
            saved.push(((*key).to_string(), env::var(key).ok()));
            if let Some(actual) = value {
                env::set_var(key, actual);
            } else {
                env::remove_var(key);
            }
        }
        EnvRestore { saved }
    }

    #[test]
    fn mem_requires_loop_ref() {
        let _guard = match env_lock().lock() {
            Ok(lock) => lock,
            Err(poisoned) => poisoned.into_inner(),
        };
        let _env = apply_env(&[("FORGE_LOOP_ID", None), ("FORGE_LOOP_NAME", None)]);
        let mut backend = InMemoryMemBackend::default();
        backend.seed_loop("loop-123", "oracle-loop");
        let out = run_for_test(&["mem", "ls"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "loop required (pass --loop or set FORGE_LOOP_ID)\n"
        );
    }
}
