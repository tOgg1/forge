use std::collections::BTreeMap;
use std::env;
use std::io::Write;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pool {
    pub id: String,
    pub name: String,
    pub strategy: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PoolMemberView {
    pub profile_id: String,
    pub profile_name: String,
    pub harness: String,
    pub auth_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PoolView {
    pub pool: Pool,
    pub members: Vec<PoolMemberView>,
}

pub trait PoolBackend {
    fn list_pools(&self) -> Result<Vec<Pool>, String>;
    fn create_pool(&mut self, name: &str, strategy: &str) -> Result<Pool, String>;
    fn add_profiles(
        &mut self,
        pool_ref: &str,
        profile_refs: &[String],
    ) -> Result<(Pool, Vec<String>), String>;
    fn show_pool(&self, pool_ref: &str) -> Result<PoolView, String>;
    fn set_default(&mut self, pool_ref: &str) -> Result<Pool, String>;
}

#[derive(Debug, Clone)]
struct InMemoryMember {
    profile_id: String,
    profile_name: String,
    harness: String,
    auth_kind: String,
    position: usize,
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryPoolBackend {
    pools: Vec<Pool>,
    members: BTreeMap<String, Vec<InMemoryMember>>,
    next_pool_id: usize,
    next_profile_id: usize,
}

impl InMemoryPoolBackend {
    fn next_pool_id(&mut self) -> String {
        self.next_pool_id += 1;
        format!("pool-{:03}", self.next_pool_id)
    }

    fn next_profile_id(&mut self) -> String {
        self.next_profile_id += 1;
        format!("profile-{:03}", self.next_profile_id)
    }

    fn resolve_pool_index(&self, reference: &str) -> Option<usize> {
        self.pools
            .iter()
            .position(|pool| pool.name == reference || pool.id == reference)
    }

    fn has_default_pool(&self) -> bool {
        self.pools.iter().any(|pool| pool.is_default)
    }
}

impl PoolBackend for InMemoryPoolBackend {
    fn list_pools(&self) -> Result<Vec<Pool>, String> {
        Ok(self.pools.clone())
    }

    fn create_pool(&mut self, name: &str, strategy: &str) -> Result<Pool, String> {
        if self.pools.iter().any(|pool| pool.name == name) {
            return Err(format!("pool \"{name}\" already exists"));
        }
        let mut pool = Pool {
            id: self.next_pool_id(),
            name: name.to_string(),
            strategy: strategy.to_string(),
            is_default: false,
        };
        if !self.has_default_pool() {
            pool.is_default = true;
        }
        self.pools.push(pool.clone());
        Ok(pool)
    }

    fn add_profiles(
        &mut self,
        pool_ref: &str,
        profile_refs: &[String],
    ) -> Result<(Pool, Vec<String>), String> {
        let pool_index = match self.resolve_pool_index(pool_ref) {
            Some(index) => index,
            None => return Err(format!("pool not found: {pool_ref}")),
        };
        let pool = self.pools[pool_index].clone();
        let mut next_position = self
            .members
            .get(&pool.id)
            .map(|bucket| bucket.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|member| member.position)
            .max()
            .unwrap_or(0);
        let mut staged = Vec::new();
        let mut added = Vec::new();
        for profile in profile_refs {
            next_position += 1;
            staged.push(InMemoryMember {
                profile_id: self.next_profile_id(),
                profile_name: profile.clone(),
                harness: String::new(),
                auth_kind: String::new(),
                position: next_position,
            });
            added.push(profile.clone());
        }
        let bucket = self.members.entry(pool.id.clone()).or_default();
        bucket.extend(staged);
        Ok((pool, added))
    }

    fn show_pool(&self, pool_ref: &str) -> Result<PoolView, String> {
        let pool_index = match self.resolve_pool_index(pool_ref) {
            Some(index) => index,
            None => return Err(format!("pool not found: {pool_ref}")),
        };
        let pool = self.pools[pool_index].clone();
        let mut members = self.members.get(&pool.id).cloned().unwrap_or_default();
        members.sort_by_key(|member| member.position);
        let rendered = members
            .into_iter()
            .map(|member| PoolMemberView {
                profile_id: member.profile_id,
                profile_name: member.profile_name,
                harness: member.harness,
                auth_kind: member.auth_kind,
            })
            .collect();
        Ok(PoolView {
            pool,
            members: rendered,
        })
    }

    fn set_default(&mut self, pool_ref: &str) -> Result<Pool, String> {
        let target = match self.resolve_pool_index(pool_ref) {
            Some(index) => index,
            None => return Err(format!("pool not found: {pool_ref}")),
        };
        for (index, pool) in self.pools.iter_mut().enumerate() {
            pool.is_default = index == target;
        }
        Ok(self.pools[target].clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Help,
    List,
    Create {
        name: String,
        strategy: String,
    },
    Add {
        pool_ref: String,
        profiles: Vec<String>,
    },
    Show {
        pool_ref: String,
    },
    SetDefault {
        pool_ref: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    command: Command,
    json: bool,
    jsonl: bool,
}

pub fn run_from_env_with_backend(backend: &mut dyn PoolBackend) -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_backend(&args, backend, &mut stdout, &mut stderr)
}

pub fn run_for_test(args: &[&str], backend: &mut dyn PoolBackend) -> CommandOutput {
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
    backend: &mut dyn PoolBackend,
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
    backend: &mut dyn PoolBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    match parsed.command {
        Command::Help => {
            write_help(stdout).map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::List => {
            let pools = backend.list_pools()?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &pools, parsed.jsonl)?;
                return Ok(());
            }
            if pools.is_empty() {
                writeln!(stdout, "No pools found").map_err(|err| err.to_string())?;
                return Ok(());
            }
            let mut rows = Vec::new();
            for pool in pools {
                let members = backend.show_pool(&pool.id)?.members.len();
                rows.push(vec![
                    pool.name,
                    pool.strategy,
                    yes_no(pool.is_default),
                    members.to_string(),
                ]);
            }
            write_table(stdout, &["NAME", "STRATEGY", "DEFAULT", "MEMBERS"], &rows)?;
            Ok(())
        }
        Command::Create { name, strategy } => {
            let pool = backend.create_pool(&name, &strategy)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &pool, parsed.jsonl)?;
                return Ok(());
            }
            writeln!(stdout, "Pool \"{}\" created", pool.name).map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Add { pool_ref, profiles } => {
            let (pool, added) = backend.add_profiles(&pool_ref, &profiles)?;
            if parsed.json || parsed.jsonl {
                let payload = serde_json::json!({
                    "pool": pool.name,
                    "added": added,
                });
                write_serialized(stdout, &payload, parsed.jsonl)?;
                return Ok(());
            }
            writeln!(
                stdout,
                "Added {} to pool \"{}\"",
                added.join(", "),
                pool.name
            )
            .map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Show { pool_ref } => {
            let view = backend.show_pool(&pool_ref)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &view, parsed.jsonl)?;
                return Ok(());
            }
            writeln!(stdout, "Pool {}", view.pool.name).map_err(|err| err.to_string())?;
            writeln!(stdout, "Strategy: {}", view.pool.strategy).map_err(|err| err.to_string())?;
            writeln!(stdout, "Default: {}", yes_no(view.pool.is_default))
                .map_err(|err| err.to_string())?;
            writeln!(stdout).map_err(|err| err.to_string())?;
            if view.members.is_empty() {
                writeln!(stdout, "No members").map_err(|err| err.to_string())?;
                return Ok(());
            }
            let mut rows = Vec::new();
            for member in view.members {
                rows.push(vec![member.profile_name, member.harness, member.auth_kind]);
            }
            write_table(stdout, &["PROFILE", "HARNESS", "AUTH_KIND"], &rows)?;
            Ok(())
        }
        Command::SetDefault { pool_ref } => {
            let pool = backend.set_default(&pool_ref)?;
            if parsed.json || parsed.jsonl {
                let payload = serde_json::json!({ "default_pool": pool.name });
                write_serialized(stdout, &payload, parsed.jsonl)?;
                return Ok(());
            }
            writeln!(stdout, "Default pool set to \"{}\"", pool.name)
                .map_err(|err| err.to_string())?;
            Ok(())
        }
    }
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    if args.is_empty() {
        return Ok(ParsedArgs {
            command: Command::Help,
            json: false,
            jsonl: false,
        });
    }

    let start = if args.first().is_some_and(|arg| arg == "pool") {
        1
    } else {
        0
    };

    let mut json = false;
    let mut jsonl = false;
    let mut subcommand: Option<String> = None;
    let mut subcommand_args: Vec<String> = Vec::new();

    let mut idx = start;
    while idx < args.len() {
        match args[idx].as_str() {
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
        Some("ls") | Some("list") => {
            ensure_no_args("pool ls", &subcommand_args)?;
            Command::List
        }
        Some("create") => parse_create_args(&subcommand_args)?,
        Some("add") => parse_add_args(&subcommand_args)?,
        Some("show") => parse_single_ref("pool show", &subcommand_args, |pool_ref| {
            Command::Show { pool_ref }
        })?,
        Some("set-default") => {
            parse_single_ref("pool set-default", &subcommand_args, |pool_ref| {
                Command::SetDefault { pool_ref }
            })?
        }
        Some(other) => return Err(format!("unknown pool argument: {other}")),
    };

    Ok(ParsedArgs {
        command,
        json,
        jsonl,
    })
}

fn parse_create_args(args: &[String]) -> Result<Command, String> {
    let mut name: Option<String> = None;
    let mut strategy = "round_robin".to_string();
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--strategy" => {
                let value = next_value(args, idx, "--strategy")?;
                strategy = parse_pool_strategy(value)?;
                idx += 2;
            }
            token if token.starts_with("--") => {
                return Err(format!("unknown pool create flag: {token}"));
            }
            token => {
                if name.is_some() {
                    return Err(format!("unexpected argument for pool create: {token}"));
                }
                name = Some(token.to_string());
                idx += 1;
            }
        }
    }
    let name = match name {
        Some(value) => value,
        None => return Err("pool create requires <name>".to_string()),
    };
    Ok(Command::Create { name, strategy })
}

fn parse_add_args(args: &[String]) -> Result<Command, String> {
    if args.len() < 2 {
        return Err("pool add requires <pool> <profile...>".to_string());
    }
    Ok(Command::Add {
        pool_ref: args[0].clone(),
        profiles: args[1..].to_vec(),
    })
}

fn parse_single_ref<F>(name: &str, args: &[String], builder: F) -> Result<Command, String>
where
    F: FnOnce(String) -> Command,
{
    if args.len() != 1 {
        return Err(format!("{name} requires exactly 1 argument"));
    }
    Ok(builder(args[0].clone()))
}

fn ensure_no_args(name: &str, args: &[String]) -> Result<(), String> {
    if let Some(first) = args.first() {
        return Err(format!("unexpected argument for {name}: {first}"));
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

fn parse_pool_strategy(value: &str) -> Result<String, String> {
    match value.to_lowercase().as_str() {
        "round_robin" | "round-robin" | "rr" => Ok("round_robin".to_string()),
        _ => Err(format!("unknown pool strategy \"{value}\"")),
    }
}

fn yes_no(value: bool) -> String {
    if value {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

fn write_serialized(
    output: &mut dyn Write,
    value: &impl Serialize,
    jsonl: bool,
) -> Result<(), String> {
    if jsonl {
        let as_value = serde_json::to_value(value).map_err(|err| err.to_string())?;
        if let serde_json::Value::Array(items) = as_value {
            for item in items {
                let line = serde_json::to_string(&item).map_err(|err| err.to_string())?;
                writeln!(output, "{line}").map_err(|err| err.to_string())?;
            }
            return Ok(());
        }
        let line = serde_json::to_string(&as_value).map_err(|err| err.to_string())?;
        writeln!(output, "{line}").map_err(|err| err.to_string())?;
        return Ok(());
    }
    let text = serde_json::to_string_pretty(value).map_err(|err| err.to_string())?;
    writeln!(output, "{text}").map_err(|err| err.to_string())?;
    Ok(())
}

fn write_help(out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(out, "forge pool - Manage profile pools")?;
    writeln!(out)?;
    writeln!(out, "Usage:")?;
    writeln!(out, "  forge pool <command> [options]")?;
    writeln!(out)?;
    writeln!(out, "Commands:")?;
    writeln!(out, "  ls|list                 List pools")?;
    writeln!(out, "  create <name>           Create a pool")?;
    writeln!(out, "  add <pool> <profile..>  Add profiles to a pool")?;
    writeln!(out, "  show <name>             Show pool details")?;
    writeln!(out, "  set-default <name>      Set the default pool")?;
    writeln!(out)?;
    writeln!(out, "Flags:")?;
    writeln!(out, "  --json                  output JSON")?;
    writeln!(out, "  --jsonl                 output JSON lines")?;
    writeln!(
        out,
        "  --strategy <strategy>   create: strategy (round_robin)"
    )?;
    Ok(())
}

fn write_table(out: &mut dyn Write, headers: &[&str], rows: &[Vec<String>]) -> Result<(), String> {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index < widths.len() && cell.len() > widths[index] {
                widths[index] = cell.len();
            }
        }
    }

    let mut header_line = String::new();
    for (index, header) in headers.iter().enumerate() {
        if index > 0 {
            header_line.push_str("  ");
        }
        header_line.push_str(&format!("{header:<width$}", width = widths[index]));
    }
    writeln!(out, "{header_line}").map_err(|err| err.to_string())?;

    for row in rows {
        let mut line = String::new();
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                line.push_str("  ");
            }
            line.push_str(&format!("{cell:<width$}", width = widths[index]));
        }
        writeln!(out, "{line}").map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{run_for_test, InMemoryPoolBackend};

    #[test]
    fn pool_create_list_and_set_default_flow() {
        let mut backend = InMemoryPoolBackend::default();

        let create_a = run_for_test(&["pool", "create", "alpha"], &mut backend);
        assert_eq!(create_a.exit_code, 0);
        assert_eq!(create_a.stdout, "Pool \"alpha\" created\n");
        assert!(create_a.stderr.is_empty());

        let create_b = run_for_test(
            &["pool", "create", "beta", "--strategy", "rr", "--json"],
            &mut backend,
        );
        assert_eq!(create_b.exit_code, 0);
        assert!(create_b.stderr.is_empty());
        assert!(create_b.stdout.contains("\"name\": \"beta\""));
        assert!(create_b.stdout.contains("\"strategy\": \"round_robin\""));

        let set_default = run_for_test(&["pool", "set-default", "beta", "--json"], &mut backend);
        assert_eq!(set_default.exit_code, 0);
        assert!(set_default.stderr.is_empty());
        assert!(set_default.stdout.contains("\"default_pool\": \"beta\""));

        let listed = run_for_test(&["pool", "ls"], &mut backend);
        assert_eq!(listed.exit_code, 0);
        assert!(listed.stderr.is_empty());
        assert!(listed.stdout.contains("NAME"));
        assert!(listed.stdout.contains("alpha"));
        assert!(listed.stdout.contains("beta"));
    }

    #[test]
    fn pool_add_and_show_json_flow() {
        let mut backend = InMemoryPoolBackend::default();
        let create = run_for_test(&["pool", "create", "alpha"], &mut backend);
        assert_eq!(create.exit_code, 0);

        let add = run_for_test(
            &["pool", "add", "alpha", "profile-a", "profile-b", "--json"],
            &mut backend,
        );
        assert_eq!(add.exit_code, 0);
        assert!(add.stderr.is_empty());
        assert!(add.stdout.contains("\"pool\": \"alpha\""));
        assert!(add.stdout.contains("\"profile-a\""));
        assert!(add.stdout.contains("\"profile-b\""));

        let show = run_for_test(&["pool", "show", "alpha", "--json"], &mut backend);
        assert_eq!(show.exit_code, 0);
        assert!(show.stderr.is_empty());
        assert!(show.stdout.contains("\"pool\""));
        assert!(show.stdout.contains("\"members\""));
        assert!(show.stdout.contains("\"profile_name\": \"profile-a\""));
        assert!(show.stdout.contains("\"profile_name\": \"profile-b\""));
    }

    #[test]
    fn validation_and_error_paths() {
        let mut backend = InMemoryPoolBackend::default();

        let unknown_strategy = run_for_test(
            &["pool", "create", "alpha", "--strategy", "weighted"],
            &mut backend,
        );
        assert_eq!(unknown_strategy.exit_code, 1);
        assert!(unknown_strategy.stdout.is_empty());
        assert!(unknown_strategy
            .stderr
            .contains("unknown pool strategy \"weighted\""));

        let missing_add_args = run_for_test(&["pool", "add", "alpha"], &mut backend);
        assert_eq!(missing_add_args.exit_code, 1);
        assert!(missing_add_args.stdout.is_empty());
        assert!(missing_add_args
            .stderr
            .contains("pool add requires <pool> <profile...>"));

        let unknown_subcommand = run_for_test(&["pool", "unknown"], &mut backend);
        assert_eq!(unknown_subcommand.exit_code, 1);
        assert!(unknown_subcommand.stdout.is_empty());
        assert!(unknown_subcommand
            .stderr
            .contains("unknown pool argument: unknown"));
    }
}
