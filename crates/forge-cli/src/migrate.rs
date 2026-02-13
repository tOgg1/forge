use std::collections::BTreeMap;
use std::env;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use tabwriter::TabWriter;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MigrationStatus {
    pub version: i32,
    pub description: String,
    pub applied: bool,
    pub applied_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub trait MigrationBackend {
    fn migrate_up(&mut self) -> Result<usize, String>;
    fn migrate_to(&mut self, target_version: i32) -> Result<(), String>;
    fn migrate_down(&mut self, steps: i32) -> Result<usize, String>;
    fn migration_status(&mut self) -> Result<Vec<MigrationStatus>, String>;
    fn schema_version(&mut self) -> Result<i32, String>;
}

#[derive(Debug)]
pub struct SqliteMigrationBackend {
    db: forge_db::Db,
}

impl SqliteMigrationBackend {
    pub fn open_from_env() -> Result<Self, String> {
        let path = resolve_database_path();
        let cfg = forge_db::Config::new(path);
        let db = forge_db::Db::open(cfg).map_err(|err| err.to_string())?;
        Ok(Self { db })
    }
}

impl MigrationBackend for SqliteMigrationBackend {
    fn migrate_up(&mut self) -> Result<usize, String> {
        self.db.migrate_up().map_err(|err| err.to_string())
    }

    fn migrate_to(&mut self, target_version: i32) -> Result<(), String> {
        self.db
            .migrate_to(target_version)
            .map_err(|err| err.to_string())
    }

    fn migrate_down(&mut self, steps: i32) -> Result<usize, String> {
        self.db.migrate_down(steps).map_err(|err| err.to_string())
    }

    fn migration_status(&mut self) -> Result<Vec<MigrationStatus>, String> {
        let status = self.db.migration_status().map_err(|err| err.to_string())?;
        Ok(status
            .into_iter()
            .map(|row| MigrationStatus {
                version: row.version,
                description: row.description,
                applied: row.applied,
                applied_at: row.applied_at,
            })
            .collect())
    }

    fn schema_version(&mut self) -> Result<i32, String> {
        self.db.schema_version().map_err(|err| err.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationSpec {
    pub version: i32,
    pub description: &'static str,
}

#[derive(Debug, Clone)]
pub struct InMemoryMigrationBackend {
    specs: Vec<MigrationSpec>,
    current_version: i32,
    applied_at: BTreeMap<i32, String>,
    tick: usize,
}

impl Default for InMemoryMigrationBackend {
    fn default() -> Self {
        Self::new(default_specs())
    }
}

impl InMemoryMigrationBackend {
    pub fn new(specs: Vec<MigrationSpec>) -> Self {
        Self {
            specs,
            current_version: 0,
            applied_at: BTreeMap::new(),
            tick: 0,
        }
    }

    fn mark_applied(&mut self, version: i32) {
        self.tick += 1;
        let stamp = format!("tick-{:03}", self.tick);
        self.applied_at.insert(version, stamp);
    }

    fn max_known_version(&self) -> i32 {
        self.specs.last().map_or(0, |spec| spec.version)
    }

    fn version_exists(&self, version: i32) -> bool {
        version == 0 || self.specs.iter().any(|spec| spec.version == version)
    }
}

impl MigrationBackend for InMemoryMigrationBackend {
    fn migrate_up(&mut self) -> Result<usize, String> {
        let to_apply: Vec<i32> = self
            .specs
            .iter()
            .filter(|spec| spec.version > self.current_version)
            .map(|spec| spec.version)
            .collect();

        let applied = to_apply.len();
        for version in to_apply {
            self.mark_applied(version);
        }

        if let Some(last) = self.specs.last() {
            if applied > 0 {
                self.current_version = last.version;
            }
        }

        Ok(applied)
    }

    fn migrate_to(&mut self, target_version: i32) -> Result<(), String> {
        if target_version < 0 {
            return Err(format!(
                "target version {target_version} cannot be negative"
            ));
        }
        if !self.version_exists(target_version) {
            return Err(format!(
                "target version {target_version} not found (max {})",
                self.max_known_version()
            ));
        }

        if target_version > self.current_version {
            let to_apply: Vec<i32> = self
                .specs
                .iter()
                .filter(|spec| {
                    spec.version > self.current_version && spec.version <= target_version
                })
                .map(|spec| spec.version)
                .collect();
            for version in to_apply {
                self.mark_applied(version);
            }
        } else if target_version < self.current_version {
            let keep: Vec<i32> = self
                .specs
                .iter()
                .filter(|spec| spec.version <= target_version)
                .map(|spec| spec.version)
                .collect();
            self.applied_at.retain(|version, _| keep.contains(version));
        }

        self.current_version = target_version;
        Ok(())
    }

    fn migrate_down(&mut self, steps: i32) -> Result<usize, String> {
        if self.current_version == 0 || steps <= 0 {
            return Ok(0);
        }

        let mut applied_versions: Vec<i32> = self
            .specs
            .iter()
            .filter(|spec| spec.version <= self.current_version)
            .map(|spec| spec.version)
            .collect();
        applied_versions.sort_unstable();
        applied_versions.reverse();

        let mut rolled_back = 0usize;
        for version in applied_versions {
            if rolled_back >= steps as usize {
                break;
            }
            self.applied_at.remove(&version);
            rolled_back += 1;
        }

        self.current_version = self.applied_at.keys().copied().max().unwrap_or(0);

        Ok(rolled_back)
    }

    fn migration_status(&mut self) -> Result<Vec<MigrationStatus>, String> {
        let mut rows = Vec::with_capacity(self.specs.len());
        for spec in &self.specs {
            let applied_at = self
                .applied_at
                .get(&spec.version)
                .cloned()
                .unwrap_or_default();
            rows.push(MigrationStatus {
                version: spec.version,
                description: spec.description.to_string(),
                applied: spec.version <= self.current_version,
                applied_at,
            });
        }
        Ok(rows)
    }

    fn schema_version(&mut self) -> Result<i32, String> {
        Ok(self.current_version)
    }
}

pub fn run_from_env_with_backend(backend: &mut dyn MigrationBackend) -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_backend(&args, backend, &mut stdout, &mut stderr)
}

pub fn run_for_test(args: &[&str], backend: &mut dyn MigrationBackend) -> CommandOutput {
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
    backend: &mut dyn MigrationBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout, stderr) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

fn execute(
    args: &[String],
    backend: &mut dyn MigrationBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    match parsed.command {
        Command::Help => {
            write_help(stdout).map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Up { target_version } => {
            if target_version > 0 {
                backend
                    .migrate_to(target_version)
                    .map_err(|err| format!("migration failed: {err}"))?;
                writeln!(stderr, "Migrated to version {target_version}")
                    .map_err(|err| err.to_string())?;
                return Ok(());
            }

            let applied = backend
                .migrate_up()
                .map_err(|err| format!("migration failed: {err}"))?;
            if applied == 0 {
                writeln!(stderr, "No pending migrations").map_err(|err| err.to_string())?;
            } else {
                writeln!(stderr, "Applied {applied} migration(s)")
                    .map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Down { steps } => {
            let rolled_back = backend
                .migrate_down(steps)
                .map_err(|err| format!("rollback failed: {err}"))?;
            if rolled_back == 0 {
                writeln!(stderr, "No migrations to roll back").map_err(|err| err.to_string())?;
            } else {
                writeln!(stderr, "Rolled back {rolled_back} migration(s)")
                    .map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Status => {
            let status = backend
                .migration_status()
                .map_err(|err| format!("failed to get migration status: {err}"))?;
            if parsed.json {
                serde_json::to_writer_pretty(&mut *stdout, &status)
                    .map_err(|err| err.to_string())?;
                writeln!(stdout).map_err(|err| err.to_string())?;
                return Ok(());
            }

            let mut tw = TabWriter::new(&mut *stdout).padding(2);
            writeln!(tw, "VERSION\tDESCRIPTION\tSTATUS\tAPPLIED AT")
                .map_err(|err| err.to_string())?;
            writeln!(tw, "-------\t-----------\t------\t----------")
                .map_err(|err| err.to_string())?;
            for row in status {
                let status = if row.applied { "applied" } else { "pending" };
                let applied_at = if row.applied {
                    row.applied_at
                } else {
                    "-".to_string()
                };
                writeln!(
                    tw,
                    "{}\t{}\t{}\t{}",
                    row.version, row.description, status, applied_at
                )
                .map_err(|err| err.to_string())?;
            }
            tw.flush().map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Version => {
            let version = backend
                .schema_version()
                .map_err(|err| format!("failed to get schema version: {err}"))?;
            if parsed.json {
                serde_json::to_writer(&mut *stdout, &serde_json::json!({ "version": version }))
                    .map_err(|err| err.to_string())?;
                writeln!(stdout).map_err(|err| err.to_string())?;
                return Ok(());
            }

            writeln!(stderr, "Schema version: {version}").map_err(|err| err.to_string())?;
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Help,
    Up { target_version: i32 },
    Down { steps: i32 },
    Status,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    command: Command,
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    if args.is_empty() {
        return Ok(ParsedArgs {
            json: false,
            command: Command::Help,
        });
    }

    let mut index = 0usize;
    let mut json = false;
    if args.get(index).is_some_and(|arg| arg == "migrate") {
        index += 1;
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                return Ok(ParsedArgs {
                    json,
                    command: Command::Help,
                });
            }
            "up" => return parse_up(args, index + 1, json),
            "down" => return parse_down(args, index + 1, json),
            "status" => return parse_status(args, index + 1, json),
            "version" => return parse_version(args, index + 1, json),
            unknown => {
                return Err(format!(
                    "error: unknown migrate argument '{unknown}' (expected one of: up, down, status, version)"
                ));
            }
        }
    }

    Ok(ParsedArgs {
        json,
        command: Command::Help,
    })
}

fn parse_up(args: &[String], mut index: usize, mut json: bool) -> Result<ParsedArgs, String> {
    let mut target_version = 0;
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--to" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --to".to_string())?;
                target_version = parse_i32_flag("--to", value)?;
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                return Ok(ParsedArgs {
                    json,
                    command: Command::Help,
                });
            }
            unknown => {
                return Err(format!(
                    "error: unknown argument for migrate up: '{unknown}'"
                ));
            }
        }
    }

    Ok(ParsedArgs {
        json,
        command: Command::Up { target_version },
    })
}

fn parse_down(args: &[String], mut index: usize, mut json: bool) -> Result<ParsedArgs, String> {
    let mut steps = 1;
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--steps" | "-n" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("error: missing value for {token}"))?;
                steps = parse_i32_flag(token, value)?;
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                return Ok(ParsedArgs {
                    json,
                    command: Command::Help,
                });
            }
            unknown => {
                return Err(format!(
                    "error: unknown argument for migrate down: '{unknown}'"
                ))
            }
        }
    }

    Ok(ParsedArgs {
        json,
        command: Command::Down { steps },
    })
}

fn parse_status(args: &[String], mut index: usize, mut json: bool) -> Result<ParsedArgs, String> {
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                return Ok(ParsedArgs {
                    json,
                    command: Command::Help,
                });
            }
            unknown => {
                return Err(format!(
                    "error: unknown argument for migrate status: '{unknown}'"
                ))
            }
        }
    }
    Ok(ParsedArgs {
        json,
        command: Command::Status,
    })
}

fn parse_version(args: &[String], mut index: usize, mut json: bool) -> Result<ParsedArgs, String> {
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                return Ok(ParsedArgs {
                    json,
                    command: Command::Help,
                });
            }
            unknown => {
                return Err(format!(
                    "error: unknown argument for migrate version: '{unknown}'"
                ))
            }
        }
    }
    Ok(ParsedArgs {
        json,
        command: Command::Version,
    })
}

fn parse_i32_flag(flag: &str, value: &str) -> Result<i32, String> {
    value
        .parse::<i32>()
        .map_err(|_| format!("error: invalid value '{value}' for {flag}"))
}

fn write_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "Manage database schema migrations.")?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  up       Apply pending migrations")?;
    writeln!(stdout, "  down     Roll back migrations")?;
    writeln!(stdout, "  status   Show migration status")?;
    writeln!(stdout, "  version  Show current schema version")?;
    Ok(())
}

fn default_specs() -> Vec<MigrationSpec> {
    vec![
        MigrationSpec {
            version: 1,
            description: "initial schema",
        },
        MigrationSpec {
            version: 2,
            description: "node connection prefs",
        },
        MigrationSpec {
            version: 3,
            description: "queue item attempts",
        },
        MigrationSpec {
            version: 4,
            description: "usage history",
        },
        MigrationSpec {
            version: 5,
            description: "port allocations",
        },
        MigrationSpec {
            version: 6,
            description: "mail and file locks",
        },
        MigrationSpec {
            version: 7,
            description: "loop runtime",
        },
        MigrationSpec {
            version: 8,
            description: "loop short id",
        },
        MigrationSpec {
            version: 9,
            description: "loop limits",
        },
        MigrationSpec {
            version: 11,
            description: "loop kv",
        },
        MigrationSpec {
            version: 12,
            description: "loop work state",
        },
    ]
}

fn resolve_database_path() -> PathBuf {
    crate::runtime_paths::resolve_database_path()
}

#[cfg(test)]
mod tests {
    use super::{run_for_test, InMemoryMigrationBackend, MigrationBackend};

    #[test]
    fn migrate_up_and_down_flow() {
        let mut backend = InMemoryMigrationBackend::default();

        let up = run_for_test(&["migrate", "up"], &mut backend);
        assert_eq!(up.exit_code, 0);
        assert!(up.stdout.is_empty(), "stdout: {}", up.stdout);
        assert_eq!(up.stderr, "Applied 11 migration(s)\n");
        assert_eq!(backend.schema_version(), Ok(12));

        let down = run_for_test(&["migrate", "down", "-n", "2"], &mut backend);
        assert_eq!(down.exit_code, 0);
        assert!(down.stdout.is_empty(), "stdout: {}", down.stdout);
        assert_eq!(down.stderr, "Rolled back 2 migration(s)\n");
        assert_eq!(backend.schema_version(), Ok(9));
    }

    #[test]
    fn invalid_subcommand_exits_non_zero() {
        let mut backend = InMemoryMigrationBackend::default();
        let result = run_for_test(&["migrate", "unknown"], &mut backend);
        assert_eq!(result.exit_code, 1);
        assert!(result.stdout.is_empty());
        assert!(result.stderr.contains("unknown migrate argument"));
    }
}
