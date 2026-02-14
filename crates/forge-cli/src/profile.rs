use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use serde::Serialize;

use crate::profile_catalog::{AuthStatus, ProfileCatalogStore};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub harness: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub auth_kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub auth_home: String,
    pub prompt_mode: String,
    pub command_template: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    pub max_concurrency: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileDoctorReport {
    pub profile: String,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileCreateInput {
    pub name: String,
    pub harness: String,
    pub auth_kind: Option<String>,
    pub auth_home: Option<String>,
    pub prompt_mode: Option<String>,
    pub command_template: Option<String>,
    pub model: Option<String>,
    pub extra_args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub max_concurrency: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfilePatch {
    pub name: Option<String>,
    pub auth_kind: Option<String>,
    pub auth_home: Option<String>,
    pub prompt_mode: Option<String>,
    pub command_template: Option<String>,
    pub model: Option<String>,
    pub extra_args: Option<Vec<String>>,
    pub env: Option<BTreeMap<String, String>>,
    pub max_concurrency: Option<i32>,
}

pub trait ProfileBackend {
    fn list_profiles(&self) -> Result<Vec<Profile>, String>;
    fn create_profile(&mut self, input: ProfileCreateInput) -> Result<Profile, String>;
    fn update_profile(&mut self, name: &str, patch: ProfilePatch) -> Result<Profile, String>;
    fn delete_profile(&mut self, name: &str) -> Result<(), String>;
    fn set_cooldown(&mut self, name: &str, until: &str) -> Result<Profile, String>;
    fn clear_cooldown(&mut self, name: &str) -> Result<Profile, String>;
    fn doctor_profile(&self, name: &str) -> Result<ProfileDoctorReport, String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryProfileBackend {
    profiles: Vec<Profile>,
    next_id: usize,
    tick: usize,
}

impl InMemoryProfileBackend {
    fn next_identifier(&mut self) -> String {
        self.next_id += 1;
        format!("profile-{:03}", self.next_id)
    }

    fn next_timestamp(&mut self) -> String {
        self.tick += 1;
        let minutes = (self.tick / 60) % 60;
        let seconds = self.tick % 60;
        format!("2026-01-01T00:{minutes:02}:{seconds:02}Z")
    }

    fn resolve_profile_index(&self, reference: &str) -> Option<usize> {
        self.profiles
            .iter()
            .position(|profile| profile.name == reference || profile.id == reference)
    }

    fn ensure_unique_name(&self, name: &str, ignore_index: Option<usize>) -> Result<(), String> {
        for (index, profile) in self.profiles.iter().enumerate() {
            if profile.name == name && Some(index) != ignore_index {
                return Err(format!("profile \"{name}\" already exists"));
            }
        }
        Ok(())
    }
}

impl ProfileBackend for InMemoryProfileBackend {
    fn list_profiles(&self) -> Result<Vec<Profile>, String> {
        Ok(self.profiles.clone())
    }

    fn create_profile(&mut self, input: ProfileCreateInput) -> Result<Profile, String> {
        self.ensure_unique_name(&input.name, None)?;

        let harness = normalize_harness(&input.harness)?;
        let auth_kind = input.auth_kind.unwrap_or_else(|| harness.clone());
        let prompt_mode = input
            .prompt_mode
            .unwrap_or_else(|| default_prompt_mode(&harness).to_string());
        validate_prompt_mode(&prompt_mode)?;

        let command_template = input
            .command_template
            .unwrap_or_else(|| default_command_template(&harness).to_string());
        let model = input.model.unwrap_or_else(|| default_model(&harness));

        let max_concurrency = input.max_concurrency.unwrap_or(1);
        if max_concurrency < 1 {
            return Err("max concurrency must be >= 1".to_string());
        }

        let stamp = self.next_timestamp();
        let profile = Profile {
            id: self.next_identifier(),
            name: input.name,
            harness,
            auth_kind,
            auth_home: input.auth_home.unwrap_or_default(),
            prompt_mode,
            command_template,
            model,
            extra_args: input.extra_args,
            env: input.env,
            max_concurrency,
            cooldown_until: None,
            created_at: stamp.clone(),
            updated_at: stamp,
        };

        self.profiles.push(profile.clone());
        Ok(profile)
    }

    fn update_profile(&mut self, name: &str, patch: ProfilePatch) -> Result<Profile, String> {
        let index = match self.resolve_profile_index(name) {
            Some(value) => value,
            None => return Err(format!("profile not found: {name}")),
        };

        if let Some(next_name) = patch.name.as_deref() {
            self.ensure_unique_name(next_name, Some(index))?;
        }
        if let Some(next_prompt_mode) = patch.prompt_mode.as_deref() {
            validate_prompt_mode(next_prompt_mode)?;
        }
        if let Some(next_max_concurrency) = patch.max_concurrency {
            if next_max_concurrency < 1 {
                return Err("max concurrency must be >= 1".to_string());
            }
        }

        let stamp = self.next_timestamp();
        let profile = &mut self.profiles[index];

        if let Some(next_name) = patch.name {
            profile.name = next_name;
        }
        if let Some(next_auth_kind) = patch.auth_kind {
            profile.auth_kind = next_auth_kind;
        }
        if let Some(next_auth_home) = patch.auth_home {
            profile.auth_home = next_auth_home;
        }
        if let Some(next_prompt_mode) = patch.prompt_mode {
            profile.prompt_mode = next_prompt_mode;
        }
        if let Some(next_command) = patch.command_template {
            profile.command_template = next_command;
        }
        if let Some(next_model) = patch.model {
            profile.model = next_model;
        }
        if let Some(next_extra_args) = patch.extra_args {
            profile.extra_args = next_extra_args;
        }
        if let Some(next_env) = patch.env {
            profile.env = next_env;
        }
        if let Some(next_max_concurrency) = patch.max_concurrency {
            profile.max_concurrency = next_max_concurrency;
        }

        profile.updated_at = stamp;
        Ok(profile.clone())
    }

    fn delete_profile(&mut self, name: &str) -> Result<(), String> {
        let index = match self.resolve_profile_index(name) {
            Some(value) => value,
            None => return Err(format!("profile not found: {name}")),
        };
        self.profiles.remove(index);
        Ok(())
    }

    fn set_cooldown(&mut self, name: &str, until: &str) -> Result<Profile, String> {
        let index = match self.resolve_profile_index(name) {
            Some(value) => value,
            None => return Err(format!("profile not found: {name}")),
        };
        let stamp = self.next_timestamp();
        let profile = &mut self.profiles[index];
        profile.cooldown_until = Some(until.to_string());
        profile.updated_at = stamp;
        Ok(profile.clone())
    }

    fn clear_cooldown(&mut self, name: &str) -> Result<Profile, String> {
        let index = match self.resolve_profile_index(name) {
            Some(value) => value,
            None => return Err(format!("profile not found: {name}")),
        };
        let stamp = self.next_timestamp();
        let profile = &mut self.profiles[index];
        profile.cooldown_until = None;
        profile.updated_at = stamp;
        Ok(profile.clone())
    }

    fn doctor_profile(&self, name: &str) -> Result<ProfileDoctorReport, String> {
        let index = match self.resolve_profile_index(name) {
            Some(value) => value,
            None => return Err(format!("profile not found: {name}")),
        };

        let profile = &self.profiles[index];
        let mut checks = Vec::new();

        if !profile.auth_home.is_empty() {
            let ok = Path::new(&profile.auth_home).exists();
            let details = if ok {
                profile.auth_home.clone()
            } else {
                "auth home not found".to_string()
            };
            checks.push(DoctorCheck {
                name: "auth_home".to_string(),
                ok,
                details,
            });
        }

        let command = first_command_segment(&profile.command_template);
        if !command.is_empty() {
            let ok = is_command_available(&command);
            let details = if ok {
                command.clone()
            } else {
                format!("command not found: {command}")
            };
            checks.push(DoctorCheck {
                name: "command".to_string(),
                ok,
                details,
            });
        }

        Ok(ProfileDoctorReport {
            profile: profile.name.clone(),
            checks,
        })
    }
}

// ---------------------------------------------------------------------------
// SQLite-backed profile backend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SqliteProfileBackend {
    db_path: PathBuf,
}

impl SqliteProfileBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
        }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))
    }

    fn db_to_cli(p: forge_db::profile_repository::Profile) -> Profile {
        Profile {
            id: p.id,
            name: p.name,
            harness: p.harness,
            auth_kind: p.auth_kind,
            auth_home: p.auth_home,
            prompt_mode: p.prompt_mode,
            command_template: p.command_template,
            model: p.model,
            extra_args: p.extra_args,
            env: p.env.into_iter().collect(),
            max_concurrency: p.max_concurrency as i32,
            cooldown_until: p.cooldown_until,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

impl ProfileBackend for SqliteProfileBackend {
    fn list_profiles(&self) -> Result<Vec<Profile>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }
        let db = self.open_db()?;
        let repo = forge_db::profile_repository::ProfileRepository::new(&db);
        let profiles = match repo.list() {
            Ok(profiles) => profiles,
            Err(err) if err.to_string().contains("no such table: profiles") => {
                return Ok(Vec::new())
            }
            Err(err) => return Err(err.to_string()),
        };
        Ok(profiles.into_iter().map(Self::db_to_cli).collect())
    }

    fn create_profile(&mut self, input: ProfileCreateInput) -> Result<Profile, String> {
        let db = self.open_db()?;
        let repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let harness = normalize_harness(&input.harness)?;
        let auth_kind = input.auth_kind.unwrap_or_else(|| harness.clone());
        let prompt_mode = input
            .prompt_mode
            .unwrap_or_else(|| default_prompt_mode(&harness).to_string());
        validate_prompt_mode(&prompt_mode)?;

        let command_template = input
            .command_template
            .unwrap_or_else(|| default_command_template(&harness).to_string());
        let model = input.model.unwrap_or_else(|| default_model(&harness));

        let max_concurrency = input.max_concurrency.unwrap_or(1);
        if max_concurrency < 1 {
            return Err("max concurrency must be >= 1".to_string());
        }

        let mut db_profile = forge_db::profile_repository::Profile {
            name: input.name,
            harness,
            auth_kind,
            auth_home: input.auth_home.unwrap_or_default(),
            prompt_mode,
            command_template,
            model,
            extra_args: input.extra_args,
            env: input.env.into_iter().collect(),
            max_concurrency: max_concurrency as i64,
            ..forge_db::profile_repository::Profile::default()
        };

        repo.create(&mut db_profile).map_err(|err| {
            if err.to_string().contains("already exists") {
                format!("profile \"{}\" already exists", db_profile.name)
            } else {
                err.to_string()
            }
        })?;

        Ok(Self::db_to_cli(db_profile))
    }

    fn update_profile(&mut self, name: &str, patch: ProfilePatch) -> Result<Profile, String> {
        let db = self.open_db()?;
        let repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let mut db_profile = repo.get_by_name(name).map_err(|err| {
            if err.to_string().contains("not found") {
                format!("profile not found: {name}")
            } else {
                err.to_string()
            }
        })?;

        if let Some(ref next_name) = patch.name {
            // Check uniqueness if renaming
            if next_name != &db_profile.name {
                if let Ok(_existing) = repo.get_by_name(next_name) {
                    return Err(format!("profile \"{next_name}\" already exists"));
                }
            }
            db_profile.name = next_name.clone();
        }
        if let Some(ref next_prompt_mode) = patch.prompt_mode {
            validate_prompt_mode(next_prompt_mode)?;
            db_profile.prompt_mode = next_prompt_mode.clone();
        }
        if let Some(next_max_concurrency) = patch.max_concurrency {
            if next_max_concurrency < 1 {
                return Err("max concurrency must be >= 1".to_string());
            }
            db_profile.max_concurrency = next_max_concurrency as i64;
        }
        if let Some(next_auth_kind) = patch.auth_kind {
            db_profile.auth_kind = next_auth_kind;
        }
        if let Some(next_auth_home) = patch.auth_home {
            db_profile.auth_home = next_auth_home;
        }
        if let Some(next_command) = patch.command_template {
            db_profile.command_template = next_command;
        }
        if let Some(next_model) = patch.model {
            db_profile.model = next_model;
        }
        if let Some(next_extra_args) = patch.extra_args {
            db_profile.extra_args = next_extra_args;
        }
        if let Some(next_env) = patch.env {
            db_profile.env = next_env.into_iter().collect();
        }

        repo.update(&mut db_profile)
            .map_err(|err| err.to_string())?;
        Ok(Self::db_to_cli(db_profile))
    }

    fn delete_profile(&mut self, name: &str) -> Result<(), String> {
        let db = self.open_db()?;
        let repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let db_profile = repo.get_by_name(name).map_err(|err| {
            if err.to_string().contains("not found") {
                format!("profile not found: {name}")
            } else {
                err.to_string()
            }
        })?;

        repo.delete(&db_profile.id).map_err(|err| err.to_string())
    }

    fn set_cooldown(&mut self, name: &str, until: &str) -> Result<Profile, String> {
        let db = self.open_db()?;
        let repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let db_profile = repo.get_by_name(name).map_err(|err| {
            if err.to_string().contains("not found") {
                format!("profile not found: {name}")
            } else {
                err.to_string()
            }
        })?;

        repo.set_cooldown(&db_profile.id, Some(until))
            .map_err(|err| err.to_string())?;

        // Re-read to get updated timestamps
        let updated = repo.get(&db_profile.id).map_err(|err| err.to_string())?;
        Ok(Self::db_to_cli(updated))
    }

    fn clear_cooldown(&mut self, name: &str) -> Result<Profile, String> {
        let db = self.open_db()?;
        let repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let db_profile = repo.get_by_name(name).map_err(|err| {
            if err.to_string().contains("not found") {
                format!("profile not found: {name}")
            } else {
                err.to_string()
            }
        })?;

        repo.set_cooldown(&db_profile.id, None)
            .map_err(|err| err.to_string())?;

        let updated = repo.get(&db_profile.id).map_err(|err| err.to_string())?;
        Ok(Self::db_to_cli(updated))
    }

    fn doctor_profile(&self, name: &str) -> Result<ProfileDoctorReport, String> {
        let db = self.open_db()?;
        let repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let db_profile = repo.get_by_name(name).map_err(|err| {
            if err.to_string().contains("not found") {
                format!("profile not found: {name}")
            } else {
                err.to_string()
            }
        })?;

        let mut checks = Vec::new();

        if !db_profile.auth_home.is_empty() {
            let ok = Path::new(&db_profile.auth_home).exists();
            let details = if ok {
                db_profile.auth_home.clone()
            } else {
                "auth home not found".to_string()
            };
            checks.push(DoctorCheck {
                name: "auth_home".to_string(),
                ok,
                details,
            });
        }

        let command = first_command_segment(&db_profile.command_template);
        if !command.is_empty() {
            let ok = is_command_available(&command);
            let details = if ok {
                command.clone()
            } else {
                format!("command not found: {command}")
            };
            checks.push(DoctorCheck {
                name: "command".to_string(),
                ok,
                details,
            });
        }

        Ok(ProfileDoctorReport {
            profile: db_profile.name,
            checks,
        })
    }
}

fn resolve_database_path() -> PathBuf {
    crate::runtime_paths::resolve_database_path()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Help,
    List,
    Add(ProfileCreateInput),
    Edit {
        name: String,
        patch: ProfilePatch,
    },
    Remove {
        name: String,
    },
    Init,
    Doctor {
        name: String,
    },
    CooldownSet {
        name: String,
        until: String,
    },
    CooldownClear {
        name: String,
    },
    CatalogStatus,
    CatalogInit {
        node_id: String,
        harness_counts: BTreeMap<String, u32>,
    },
    CatalogAuth {
        node_id: String,
        profile_id: String,
        auth_status: AuthStatus,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    command: Command,
    json: bool,
    jsonl: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProfileInitDetection {
    harnesses: Vec<String>,
    aliases: Vec<AliasDetection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AliasDetection {
    name: String,
    harness: String,
    command: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    auth_home: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProfileInitResult {
    imported: usize,
    profiles: Vec<String>,
    harnesses: Vec<String>,
    aliases: Vec<AliasDetection>,
}

const PROFILE_INIT_HARNESS_BINARIES: &[(&str, &[&str])] = &[
    ("amp", &["amp"]),
    ("claude", &["claude"]),
    ("codex", &["codex"]),
    ("droid", &["droid", "factory"]),
    ("opencode", &["opencode"]),
    ("pi", &["pi"]),
];

pub fn run_from_env_with_backend(backend: &mut dyn ProfileBackend) -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_backend(&args, backend, &mut stdout, &mut stderr)
}

pub fn run_for_test(args: &[&str], backend: &mut dyn ProfileBackend) -> CommandOutput {
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
    backend: &mut dyn ProfileBackend,
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
    backend: &mut dyn ProfileBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    match parsed.command {
        Command::Help => {
            write_help(stdout).map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::List => {
            let profiles = backend.list_profiles()?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &profiles, parsed.jsonl)?;
                return Ok(());
            }

            if profiles.is_empty() {
                writeln!(stdout, "No profiles found").map_err(|err| err.to_string())?;
                return Ok(());
            }

            let mut rows = Vec::new();
            for profile in profiles {
                rows.push(vec![
                    profile.name,
                    profile.harness,
                    profile.auth_kind,
                    profile.auth_home,
                    profile.max_concurrency.to_string(),
                    profile.cooldown_until.unwrap_or_default(),
                ]);
            }
            write_table(
                stdout,
                &[
                    "NAME",
                    "HARNESS",
                    "AUTH_KIND",
                    "AUTH_HOME",
                    "MAX_CONCURRENCY",
                    "COOLDOWN",
                ],
                &rows,
            )
        }
        Command::Add(input) => {
            let profile = backend.create_profile(input)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &profile, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(stdout, "Profile \"{}\" created", profile.name)
                .map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Edit { name, patch } => {
            let profile = backend.update_profile(&name, patch)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &profile, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(stdout, "Profile \"{}\" updated", profile.name)
                .map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Remove { name } => {
            backend.delete_profile(&name)?;
            if parsed.json || parsed.jsonl {
                let payload = serde_json::json!({"deleted": true, "profile": name});
                write_serialized(stdout, &payload, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(stdout, "Profile \"{}\" removed", name).map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Init => {
            let detection = detect_profile_init()?;
            let result = instantiate_profiles_from_detection(backend, &detection)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &result, parsed.jsonl)?;
            } else if result.imported == 0 {
                writeln!(stdout, "No profiles imported from shell aliases/harnesses")
                    .map_err(|err| err.to_string())?;
            } else {
                writeln!(stdout, "Imported {} profiles", result.imported)
                    .map_err(|err| err.to_string())?;
                for profile in &result.profiles {
                    writeln!(stdout, "- {profile}").map_err(|err| err.to_string())?;
                }
            }
            Ok(())
        }
        Command::Doctor { name } => {
            let report = backend.doctor_profile(&name)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &report, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(stdout, "Profile {}", report.profile).map_err(|err| err.to_string())?;
            for check in report.checks {
                let status = if check.ok { "OK" } else { "FAIL" };
                writeln!(stdout, "- [{}] {}: {}", status, check.name, check.details)
                    .map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::CooldownSet { name, until } => {
            let resolved = parse_time_or_duration(&until)?;
            let profile = backend.set_cooldown(&name, &resolved)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &profile, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(
                stdout,
                "Profile \"{}\" cooldown set to {}",
                profile.name, resolved
            )
            .map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::CooldownClear { name } => {
            let profile = backend.clear_cooldown(&name)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &profile, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(stdout, "Profile \"{}\" cooldown cleared", profile.name)
                .map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::CatalogStatus => {
            let store = ProfileCatalogStore::open_from_env();
            let catalog = store.status()?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &catalog, parsed.jsonl)?;
                return Ok(());
            }

            if catalog.harness_counts.is_empty() {
                writeln!(stdout, "No profile catalog configured").map_err(|err| err.to_string())?;
                return Ok(());
            }

            writeln!(stdout, "Harness Counts").map_err(|err| err.to_string())?;
            for (harness, count) in &catalog.harness_counts {
                writeln!(stdout, "- {harness}: {count}").map_err(|err| err.to_string())?;
            }
            writeln!(stdout).map_err(|err| err.to_string())?;
            writeln!(stdout, "Node Profile Auth Status").map_err(|err| err.to_string())?;
            if catalog.nodes.is_empty() {
                writeln!(stdout, "(no nodes provisioned)").map_err(|err| err.to_string())?;
                return Ok(());
            }

            let mut rows = Vec::new();
            for node_id in catalog.nodes.keys() {
                if let Some(summary) = store.node_summary(node_id)? {
                    rows.push(vec![
                        node_id.clone(),
                        summary.total.to_string(),
                        summary.ok.to_string(),
                        summary.expired.to_string(),
                        summary.missing.to_string(),
                    ]);
                }
            }
            write_table(
                stdout,
                &["NODE", "TOTAL", "OK", "EXPIRED", "MISSING"],
                &rows,
            )
        }
        Command::CatalogInit {
            node_id,
            harness_counts,
        } => {
            let mut counts = harness_counts;
            if counts.is_empty() {
                let profiles = backend.list_profiles()?;
                for profile in profiles {
                    *counts.entry(profile.harness).or_insert(0) += 1;
                }
            }
            if counts.is_empty() {
                return Err(
                    "no harness counts provided and no local profiles to derive from".to_string(),
                );
            }
            let store = ProfileCatalogStore::open_from_env();
            let provisioned = store.provision_node(&node_id, &counts)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &provisioned, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(
                stdout,
                "Provisioned node {} with {} catalog profiles",
                provisioned.node_id,
                provisioned.profiles.len()
            )
            .map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::CatalogAuth {
            node_id,
            profile_id,
            auth_status,
        } => {
            let store = ProfileCatalogStore::open_from_env();
            let updated = store.set_auth_status(&node_id, &profile_id, auth_status)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &updated, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(
                stdout,
                "Updated {} on {}",
                profile_id.trim(),
                updated.node_id
            )
            .map_err(|err| err.to_string())?;
            Ok(())
        }
    }
}

fn detect_profile_init() -> Result<ProfileInitDetection, String> {
    let harnesses = detect_installed_harnesses();
    let alias_lines = collect_alias_lines()?;
    let aliases = parse_alias_lines(&alias_lines);
    Ok(ProfileInitDetection { harnesses, aliases })
}

fn detect_installed_harnesses() -> Vec<String> {
    let mut installed = BTreeSet::new();
    for (harness, binaries) in PROFILE_INIT_HARNESS_BINARIES {
        if binaries.iter().any(|binary| is_command_available(binary)) {
            installed.insert((*harness).to_string());
        }
    }
    installed.into_iter().collect()
}

fn collect_alias_lines() -> Result<Vec<String>, String> {
    let mut lines = Vec::new();

    if let Some(path) = alias_file_path() {
        match fs::read_to_string(&path) {
            Ok(raw) => {
                lines.extend(raw.lines().map(|line| line.to_string()));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("read alias file {}: {err}", path.display())),
        }
    }

    if env::var_os("FORGE_PROFILE_INIT_SKIP_ZSH_ALIAS").is_none() {
        lines.extend(alias_lines_from_zsh());
    }

    Ok(lines)
}

fn alias_file_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("FORGE_PROFILE_INIT_ALIAS_FILE") {
        return Some(PathBuf::from(path));
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".zsh_aliases"))
}

fn alias_lines_from_zsh() -> Vec<String> {
    let output = ProcessCommand::new("zsh").args(["-ic", "alias"]).output();
    match output {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| line.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_alias_lines(lines: &[String]) -> Vec<AliasDetection> {
    let mut by_name = BTreeMap::<String, AliasDetection>::new();
    for line in lines {
        let Some((name, command)) = parse_alias_line(line) else {
            continue;
        };
        if by_name.contains_key(&name) {
            continue;
        }
        let Some(harness) = detect_alias_harness(&name, &command) else {
            continue;
        };
        by_name.insert(
            name.clone(),
            AliasDetection {
                name,
                harness: harness.to_string(),
                auth_home: alias_auth_home(harness, &command),
                command,
            },
        );
    }
    by_name.into_values().collect()
}

fn parse_alias_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let payload = trimmed.strip_prefix("alias ")?;
    let (raw_name, raw_command) = payload.split_once('=')?;
    let name = raw_name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let command = strip_matching_quotes(raw_command.trim()).trim().to_string();
    if command.is_empty() {
        return None;
    }
    Some((name, command))
}

fn strip_matching_quotes(value: &str) -> &str {
    if value.len() >= 2 {
        let first = value.chars().next().unwrap_or_default();
        let last = value.chars().last().unwrap_or_default();
        if (first == '\'' && last == '\'') || (first == '"' && last == '"') {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn detect_alias_harness(name: &str, command: &str) -> Option<&'static str> {
    if let Some(binary) = first_executable_token(command) {
        if let Some(harness) = harness_from_binary(binary) {
            return Some(harness);
        }
    }
    harness_from_alias_name(name)
}

fn first_executable_token(command: &str) -> Option<&str> {
    for token in command.split_whitespace() {
        let token = strip_matching_quotes(token).trim();
        if token.is_empty() {
            continue;
        }
        if looks_like_assignment(token) {
            continue;
        }
        if matches!(token, "command" | "noglob" | "nocorrect") {
            continue;
        }
        return Some(
            token
                .rsplit('/')
                .next()
                .unwrap_or(token)
                .trim_end_matches(';'),
        );
    }
    None
}

fn looks_like_assignment(token: &str) -> bool {
    let Some((key, _)) = token.split_once('=') else {
        return false;
    };
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn harness_from_binary(binary: &str) -> Option<&'static str> {
    let normalized = binary.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "amp" => Some("amp"),
        "claude" | "claude-code" => Some("claude"),
        "codex" => Some("codex"),
        "droid" | "factory" => Some("droid"),
        "opencode" => Some("opencode"),
        "pi" => Some("pi"),
        _ => None,
    }
}

fn harness_from_alias_name(name: &str) -> Option<&'static str> {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.starts_with("oc") || normalized.contains("opencode") {
        return Some("opencode");
    }
    if normalized.starts_with("cc") || normalized.contains("claude") {
        return Some("claude");
    }
    if normalized.starts_with("codex") || normalized.starts_with("cx") {
        return Some("codex");
    }
    if normalized.starts_with("pi") {
        return Some("pi");
    }
    if normalized.starts_with("amp") {
        return Some("amp");
    }
    if normalized.starts_with("droid") || normalized.starts_with("factory") {
        return Some("droid");
    }
    None
}

fn alias_auth_home(harness: &str, command: &str) -> String {
    let keys: &[&str] = match harness {
        "amp" => &["AMP_HOME"],
        "claude" => &["CLAUDE_HOME", "CLAUDE_CONFIG_DIR"],
        "codex" => &["CODEX_HOME"],
        "droid" => &["DROID_HOME", "FACTORY_HOME"],
        "opencode" => &["OPENCODE_HOME"],
        "pi" => &["PI_HOME"],
        _ => &[],
    };
    if keys.is_empty() {
        return String::new();
    }

    for token in command.split_whitespace() {
        let token = strip_matching_quotes(token).trim();
        if let Some((key, value)) = token.split_once('=') {
            if keys.contains(&key) {
                return strip_matching_quotes(value).to_string();
            }
        }
    }
    String::new()
}

fn instantiate_profiles_from_detection(
    backend: &mut dyn ProfileBackend,
    detection: &ProfileInitDetection,
) -> Result<ProfileInitResult, String> {
    let mut used_names: BTreeSet<String> = backend
        .list_profiles()?
        .into_iter()
        .map(|profile| profile.name)
        .collect();
    let mut imported = Vec::new();
    let mut alias_harnesses = BTreeSet::new();

    for alias in &detection.aliases {
        alias_harnesses.insert(alias.harness.clone());
        let profile_name = next_profile_name(&used_names, &alias.name);
        let created = backend.create_profile(ProfileCreateInput {
            name: profile_name.clone(),
            harness: alias.harness.clone(),
            auth_kind: Some(alias.harness.clone()),
            auth_home: if alias.auth_home.is_empty() {
                None
            } else {
                Some(alias.auth_home.clone())
            },
            prompt_mode: None,
            command_template: Some(alias.command.clone()),
            model: None,
            extra_args: Vec::new(),
            env: BTreeMap::new(),
            max_concurrency: Some(1),
        })?;
        used_names.insert(created.name.clone());
        imported.push(created.name);
    }

    for harness in &detection.harnesses {
        if alias_harnesses.contains(harness) {
            continue;
        }
        let profile_name = next_profile_name(&used_names, harness);
        let created = backend.create_profile(ProfileCreateInput {
            name: profile_name,
            harness: harness.clone(),
            auth_kind: Some(harness.clone()),
            auth_home: None,
            prompt_mode: None,
            command_template: None,
            model: None,
            extra_args: Vec::new(),
            env: BTreeMap::new(),
            max_concurrency: Some(1),
        })?;
        used_names.insert(created.name.clone());
        imported.push(created.name);
    }

    Ok(ProfileInitResult {
        imported: imported.len(),
        profiles: imported,
        harnesses: detection.harnesses.clone(),
        aliases: detection.aliases.clone(),
    })
}

fn next_profile_name(used_names: &BTreeSet<String>, base: &str) -> String {
    let base = if base.trim().is_empty() {
        "profile"
    } else {
        base.trim()
    };
    let mut candidate = base.to_string();
    let mut suffix = 2usize;
    while used_names.contains(&candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    if args.is_empty() {
        return Ok(ParsedArgs {
            command: Command::Help,
            json: false,
            jsonl: false,
        });
    }

    let start = if args.first().is_some_and(|arg| arg == "profile") {
        1
    } else {
        0
    };

    let mut json = false;
    let mut jsonl = false;
    let mut subcommand: Option<String> = None;
    let mut subcommand_args: Vec<String> = Vec::new();

    let mut index = start;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json = true;
                index += 1;
                continue;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
                continue;
            }
            _ => {}
        }

        if subcommand.is_none() {
            subcommand = Some(args[index].clone());
        } else {
            subcommand_args.push(args[index].clone());
        }
        index += 1;
    }

    let command = match subcommand.as_deref() {
        None | Some("help") | Some("-h") | Some("--help") => Command::Help,
        Some("ls") | Some("list") => {
            ensure_no_args("profile ls", &subcommand_args)?;
            Command::List
        }
        Some("add") => parse_add_args(&subcommand_args)?,
        Some("edit") => parse_edit_args(&subcommand_args)?,
        Some("rm") | Some("remove") => parse_single_ref("profile rm", &subcommand_args, |name| {
            Command::Remove { name }
        })?,
        Some("doctor") => parse_single_ref("profile doctor", &subcommand_args, |name| {
            Command::Doctor { name }
        })?,
        Some("init") => {
            ensure_no_args("profile init", &subcommand_args)?;
            Command::Init
        }
        Some("cooldown") => parse_cooldown_args(&subcommand_args)?,
        Some("catalog") => parse_catalog_args(&subcommand_args)?,
        Some(other) => return Err(format!("unknown profile argument: {other}")),
    };

    Ok(ParsedArgs {
        command,
        json,
        jsonl,
    })
}

fn parse_add_args(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Err("profile add requires <harness>".to_string());
    }

    let harness = normalize_harness(&args[0])?;
    let mut input = ProfileCreateInput {
        name: String::new(),
        harness,
        auth_kind: None,
        auth_home: None,
        prompt_mode: None,
        command_template: None,
        model: None,
        extra_args: Vec::new(),
        env: BTreeMap::new(),
        max_concurrency: None,
    };

    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--name" => {
                input.name = next_value(args, index, "--name")?.to_string();
                index += 2;
            }
            "--auth-kind" => {
                input.auth_kind = Some(next_value(args, index, "--auth-kind")?.to_string());
                index += 2;
            }
            "--home" => {
                input.auth_home = Some(next_value(args, index, "--home")?.to_string());
                index += 2;
            }
            "--prompt-mode" => {
                input.prompt_mode = Some(next_value(args, index, "--prompt-mode")?.to_string());
                index += 2;
            }
            "--command" => {
                input.command_template = Some(next_value(args, index, "--command")?.to_string());
                index += 2;
            }
            "--model" => {
                input.model = Some(next_value(args, index, "--model")?.to_string());
                index += 2;
            }
            "--extra-arg" => {
                input
                    .extra_args
                    .push(next_value(args, index, "--extra-arg")?.to_string());
                index += 2;
            }
            "--env" => {
                let pair = next_value(args, index, "--env")?;
                let (key, value) = parse_env_pair(pair)?;
                input.env.insert(key, value);
                index += 2;
            }
            "--max-concurrency" => {
                let raw = next_value(args, index, "--max-concurrency")?;
                input.max_concurrency = Some(parse_i32(raw, "--max-concurrency")?);
                index += 2;
            }
            token => return Err(format!("unknown profile add flag: {token}")),
        }
    }

    if input.name.trim().is_empty() {
        return Err("--name is required".to_string());
    }

    Ok(Command::Add(input))
}

fn parse_edit_args(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Err("profile edit requires <name>".to_string());
    }

    let name = args[0].clone();
    let mut patch = ProfilePatch::default();
    let mut extra_args = Vec::new();
    let mut env_map = BTreeMap::new();
    let mut extra_arg_seen = false;
    let mut env_seen = false;

    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--name" => {
                patch.name = Some(next_value(args, index, "--name")?.to_string());
                index += 2;
            }
            "--auth-kind" => {
                patch.auth_kind = Some(next_value(args, index, "--auth-kind")?.to_string());
                index += 2;
            }
            "--home" => {
                patch.auth_home = Some(next_value(args, index, "--home")?.to_string());
                index += 2;
            }
            "--prompt-mode" => {
                patch.prompt_mode = Some(next_value(args, index, "--prompt-mode")?.to_string());
                index += 2;
            }
            "--command" => {
                patch.command_template = Some(next_value(args, index, "--command")?.to_string());
                index += 2;
            }
            "--model" => {
                patch.model = Some(next_value(args, index, "--model")?.to_string());
                index += 2;
            }
            "--extra-arg" => {
                extra_args.push(next_value(args, index, "--extra-arg")?.to_string());
                extra_arg_seen = true;
                index += 2;
            }
            "--env" => {
                let pair = next_value(args, index, "--env")?;
                let (key, value) = parse_env_pair(pair)?;
                env_map.insert(key, value);
                env_seen = true;
                index += 2;
            }
            "--max-concurrency" => {
                let raw = next_value(args, index, "--max-concurrency")?;
                patch.max_concurrency = Some(parse_i32(raw, "--max-concurrency")?);
                index += 2;
            }
            token => return Err(format!("unknown profile edit flag: {token}")),
        }
    }

    if extra_arg_seen {
        patch.extra_args = Some(extra_args);
    }
    if env_seen {
        patch.env = Some(env_map);
    }

    Ok(Command::Edit { name, patch })
}

fn parse_cooldown_args(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Err("profile cooldown requires a subcommand (set|clear)".to_string());
    }

    match args[0].as_str() {
        "set" => parse_cooldown_set_args(&args[1..]),
        "clear" => parse_single_ref("profile cooldown clear", &args[1..], |name| {
            Command::CooldownClear { name }
        }),
        other => Err(format!("unknown profile cooldown argument: {other}")),
    }
}

fn parse_cooldown_set_args(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Err("profile cooldown set requires <name>".to_string());
    }

    let name = args[0].clone();
    let mut until: Option<String> = None;

    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--until" => {
                until = Some(next_value(args, index, "--until")?.to_string());
                index += 2;
            }
            token => return Err(format!("unknown profile cooldown set flag: {token}")),
        }
    }

    let until = match until {
        Some(value) if !value.trim().is_empty() => value,
        _ => return Err("--until is required".to_string()),
    };

    Ok(Command::CooldownSet { name, until })
}

fn parse_catalog_args(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Ok(Command::CatalogStatus);
    }
    match args[0].as_str() {
        "status" => {
            ensure_no_args("profile catalog status", &args[1..])?;
            Ok(Command::CatalogStatus)
        }
        "init" => parse_catalog_init_args(&args[1..]),
        "auth" => parse_catalog_auth_args(&args[1..]),
        other => Err(format!("unknown profile catalog argument: {other}")),
    }
}

fn parse_catalog_init_args(args: &[String]) -> Result<Command, String> {
    let mut node_id = String::new();
    let mut harness_counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--node" => {
                node_id = next_value(args, index, "--node")?.to_string();
                index += 2;
            }
            "--count" => {
                let raw = next_value(args, index, "--count")?;
                let (harness, count) = parse_harness_count(raw)?;
                harness_counts.insert(harness, count);
                index += 2;
            }
            token => return Err(format!("unknown profile catalog init flag: {token}")),
        }
    }
    if node_id.trim().is_empty() {
        return Err("--node is required".to_string());
    }
    Ok(Command::CatalogInit {
        node_id,
        harness_counts,
    })
}

fn parse_catalog_auth_args(args: &[String]) -> Result<Command, String> {
    let mut node_id = String::new();
    let mut profile_id = String::new();
    let mut auth_status: Option<AuthStatus> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--node" => {
                node_id = next_value(args, index, "--node")?.to_string();
                index += 2;
            }
            "--profile" => {
                profile_id = next_value(args, index, "--profile")?.to_string();
                index += 2;
            }
            "--status" => {
                auth_status = Some(AuthStatus::parse(next_value(args, index, "--status")?)?);
                index += 2;
            }
            token => return Err(format!("unknown profile catalog auth flag: {token}")),
        }
    }
    if node_id.trim().is_empty() {
        return Err("--node is required".to_string());
    }
    if profile_id.trim().is_empty() {
        return Err("--profile is required".to_string());
    }
    let Some(auth_status) = auth_status else {
        return Err("--status is required".to_string());
    };
    Ok(Command::CatalogAuth {
        node_id,
        profile_id,
        auth_status,
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
    match args.get(index + 1) {
        Some(value) => Ok(value.as_str()),
        None => Err(format!("{flag} requires a value")),
    }
}

fn parse_env_pair(value: &str) -> Result<(String, String), String> {
    let mut parts = value.splitn(2, '=');
    let key = parts.next().unwrap_or_default().trim();
    let tail = parts.next();
    if key.is_empty() || tail.is_none() {
        return Err(format!("invalid env pair {value:?} (expected KEY=VALUE)"));
    }
    Ok((key.to_string(), tail.unwrap_or_default().to_string()))
}

fn parse_i32(value: &str, flag: &str) -> Result<i32, String> {
    value
        .parse::<i32>()
        .map_err(|_| format!("invalid value {value:?} for {flag}"))
}

fn parse_harness_count(value: &str) -> Result<(String, u32), String> {
    let Some((harness, count_raw)) = value.split_once('=') else {
        return Err(format!(
            "invalid harness count {:?} (expected harness=count)",
            value
        ));
    };
    let harness = harness.trim().to_ascii_lowercase();
    if harness.is_empty() {
        return Err(format!(
            "invalid harness count {:?} (harness missing)",
            value
        ));
    }
    let count = count_raw
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("invalid harness count {:?} (count must be u32)", value))?;
    if count == 0 {
        return Err(format!(
            "invalid harness count {:?} (count must be > 0)",
            value
        ));
    }
    Ok((harness, count))
}

fn normalize_harness(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "pi" | "opencode" | "codex" | "claude" | "claude-code" | "droid" | "factory" | "amp" => {
            Ok(if normalized == "claude-code" {
                "claude".to_string()
            } else if normalized == "factory" {
                "droid".to_string()
            } else {
                normalized
            })
        }
        _ => Err(format!("unknown harness {value:?}")),
    }
}

fn validate_prompt_mode(value: &str) -> Result<(), String> {
    match value {
        "env" | "stdin" | "path" => Ok(()),
        _ => Err(format!("invalid prompt mode {value:?}")),
    }
}

fn default_prompt_mode(harness: &str) -> &'static str {
    match harness {
        "codex" => "env",
        "claude" => "env",
        _ => "path",
    }
}

fn default_command_template(harness: &str) -> &'static str {
    match harness {
        "amp" => "amp",
        "codex" => "codex exec",
        "claude" => {
            "claude --dangerously-skip-permissions --verbose --output-format stream-json --include-partial-messages -p \"$FORGE_PROMPT_CONTENT\""
        }
        "opencode" => "opencode run",
        "droid" => "droid",
        _ => "pi -p \"{prompt}\"",
    }
}

fn default_model(harness: &str) -> String {
    match harness {
        "codex" => "gpt-5".to_string(),
        "claude" => "sonnet".to_string(),
        "opencode" => "default".to_string(),
        _ => String::new(),
    }
}

fn first_command_segment(command_template: &str) -> String {
    command_template
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn is_command_available(command: &str) -> bool {
    if command.is_empty() {
        return false;
    }

    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };

    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return true;
        }
    }
    false
}

/// Parse a time value as either a Go-style duration (e.g. `1h`, `30m`, `2h30m5s`)
/// or an RFC3339 timestamp. Returns a UTC RFC3339 string.
///
/// Mirrors Go `parseTimeOrDuration` from internal/cli/profile.go.
fn parse_time_or_duration(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("time value is required".to_string());
    }

    // Try parsing as a Go-style duration first.
    if let Some(duration) = parse_go_duration(value) {
        let now = chrono::Utc::now();
        let resolved = now + duration;
        return Ok(resolved.format("%Y-%m-%dT%H:%M:%SZ").to_string());
    }

    // Try parsing as RFC3339.
    match chrono::DateTime::parse_from_rfc3339(value) {
        Ok(dt) => Ok(dt.to_utc().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        Err(_) => Err(format!("invalid time {value:?}")),
    }
}

/// Parse a Go-style duration string (e.g. `1h`, `30m`, `2h30m5s`, `500ms`).
/// Returns `None` if the string is not a valid Go duration.
///
/// Go's `time.ParseDuration` accepts: h, m, s, ms, us/µs, ns.
fn parse_go_duration(s: &str) -> Option<chrono::Duration> {
    if s.is_empty() {
        return None;
    }

    let mut remaining = s;
    let mut total_nanos: i64 = 0;
    let mut found_any = false;

    while !remaining.is_empty() {
        // Parse optional leading sign or digits.
        let (number, rest) = parse_float_prefix(remaining)?;
        remaining = rest;

        // Parse unit.
        let (unit_nanos, rest) = parse_duration_unit(remaining)?;
        remaining = rest;

        total_nanos = total_nanos.checked_add((number * unit_nanos as f64) as i64)?;
        found_any = true;
    }

    if !found_any {
        return None;
    }

    Some(chrono::Duration::nanoseconds(total_nanos))
}

fn parse_float_prefix(s: &str) -> Option<(f64, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;

    // Skip leading sign.
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }

    let start = i;

    // Integer part.
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }

    // Fractional part.
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }

    if i == start
        && (s.is_empty() || (!bytes[0].is_ascii_digit() && bytes[0] != b'+' && bytes[0] != b'-'))
    {
        return None;
    }
    if i == 0 {
        return None;
    }

    let num_str = &s[..i];
    let number: f64 = num_str.parse().ok()?;
    Some((number, &s[i..]))
}

fn parse_duration_unit(s: &str) -> Option<(i64, &str)> {
    if let Some(rest) = s.strip_prefix("ns") {
        Some((1, rest))
    } else if let Some(rest) = s.strip_prefix("us").or_else(|| s.strip_prefix("µs")) {
        Some((1_000, rest))
    } else if let Some(rest) = s.strip_prefix("ms") {
        Some((1_000_000, rest))
    } else if let Some(rest) = s.strip_prefix('s') {
        Some((1_000_000_000, rest))
    } else if let Some(rest) = s.strip_prefix('m') {
        Some((60_000_000_000, rest))
    } else if let Some(rest) = s.strip_prefix('h') {
        Some((3_600_000_000_000, rest))
    } else {
        None
    }
}

fn write_serialized(
    out: &mut dyn Write,
    value: &impl Serialize,
    jsonl: bool,
) -> Result<(), String> {
    if jsonl {
        let as_value = serde_json::to_value(value).map_err(|err| err.to_string())?;
        if let serde_json::Value::Array(items) = as_value {
            for item in items {
                let line = serde_json::to_string(&item).map_err(|err| err.to_string())?;
                writeln!(out, "{line}").map_err(|err| err.to_string())?;
            }
            return Ok(());
        }
        let line = serde_json::to_string(&as_value).map_err(|err| err.to_string())?;
        writeln!(out, "{line}").map_err(|err| err.to_string())?;
        return Ok(());
    }

    let text = serde_json::to_string_pretty(value).map_err(|err| err.to_string())?;
    writeln!(out, "{text}").map_err(|err| err.to_string())?;
    Ok(())
}

fn write_help(out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(out, "forge profile - Manage harness profiles")?;
    writeln!(out)?;
    writeln!(out, "Usage:")?;
    writeln!(out, "  forge profile <command> [options]")?;
    writeln!(out)?;
    writeln!(out, "Commands:")?;
    writeln!(out, "  ls|list                 List profiles")?;
    writeln!(out, "  add <harness>           Add a profile")?;
    writeln!(out, "  edit <name>             Edit a profile")?;
    writeln!(out, "  rm <name>               Remove a profile")?;
    writeln!(
        out,
        "  init                    Initialize profiles from shell aliases"
    )?;
    writeln!(out, "  doctor <name>           Check profile configuration")?;
    writeln!(out, "  cooldown set <name>     Set profile cooldown")?;
    writeln!(out, "  cooldown clear <name>   Clear profile cooldown")?;
    writeln!(
        out,
        "  catalog status          Show mesh profile catalog status"
    )?;
    writeln!(out, "  catalog init --node <id> [--count harness=n]...")?;
    writeln!(
        out,
        "  catalog auth --node <id> --profile <pid> --status <ok|expired|missing>"
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
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    use super::{
        detect_installed_harnesses, instantiate_profiles_from_detection, parse_alias_line,
        parse_alias_lines, parse_go_duration, parse_time_or_duration, run_for_test, AliasDetection,
        InMemoryProfileBackend, ProfileInitDetection,
    };

    struct EnvVarGuard {
        key: String,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self {
                key: key.to_string(),
                previous,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous.take() {
                std::env::set_var(&self.key, value);
            } else {
                std::env::remove_var(&self.key);
            }
        }
    }

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "forge-profile-init-{tag}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        match LOCK.get_or_init(|| Mutex::new(())).lock() {
            Ok(guard) => guard,
            Err(_) => panic!("env test lock poisoned"),
        }
    }

    #[test]
    fn profile_add_list_edit_remove_flow() {
        let mut backend = InMemoryProfileBackend::default();

        let add = run_for_test(
            &[
                "profile",
                "add",
                "codex",
                "--name",
                "alpha",
                "--auth-kind",
                "codex",
                "--home",
                "/tmp/auth-alpha",
                "--model",
                "gpt-5.1",
                "--extra-arg",
                "--sandbox",
                "--env",
                "A=1",
                "--json",
            ],
            &mut backend,
        );
        assert_eq!(add.exit_code, 0);
        assert!(add.stderr.is_empty());
        assert!(add.stdout.contains("\"name\": \"alpha\""));

        let edit = run_for_test(
            &["profile", "edit", "alpha", "--model", "gpt-5.2", "--json"],
            &mut backend,
        );
        assert_eq!(edit.exit_code, 0);
        assert!(edit.stderr.is_empty());
        assert!(edit.stdout.contains("\"model\": \"gpt-5.2\""));

        let list = run_for_test(&["profile", "ls", "--json"], &mut backend);
        assert_eq!(list.exit_code, 0);
        assert!(list.stderr.is_empty());
        assert!(list.stdout.contains("\"alpha\""));

        let remove = run_for_test(&["profile", "rm", "alpha", "--json"], &mut backend);
        assert_eq!(remove.exit_code, 0);
        assert!(remove.stderr.is_empty());
        assert_eq!(
            remove.stdout,
            "{\n  \"deleted\": true,\n  \"profile\": \"alpha\"\n}\n"
        );
    }

    #[test]
    fn profile_cooldown_and_doctor_flow() {
        let mut backend = InMemoryProfileBackend::default();
        let _ = run_for_test(
            &["profile", "add", "claude", "--name", "beta", "--json"],
            &mut backend,
        );

        let set = run_for_test(
            &[
                "profile",
                "cooldown",
                "set",
                "beta",
                "--until",
                "2026-12-31T00:00:00Z",
                "--json",
            ],
            &mut backend,
        );
        assert_eq!(set.exit_code, 0);
        assert!(set.stderr.is_empty());
        assert!(set
            .stdout
            .contains("\"cooldown_until\": \"2026-12-31T00:00:00Z\""));

        let clear = run_for_test(
            &["profile", "cooldown", "clear", "beta", "--json"],
            &mut backend,
        );
        assert_eq!(clear.exit_code, 0);
        assert!(clear.stderr.is_empty());
        assert!(!clear.stdout.contains("cooldown_until"));

        let doctor = run_for_test(&["profile", "doctor", "beta", "--json"], &mut backend);
        assert_eq!(doctor.exit_code, 0);
        assert!(doctor.stderr.is_empty());
        assert!(doctor.stdout.contains("\"profile\": \"beta\""));
    }

    #[test]
    fn profile_add_claude_defaults_to_stream_json_command() {
        let mut backend = InMemoryProfileBackend::default();
        let add = run_for_test(
            &["profile", "add", "claude", "--name", "streamy", "--json"],
            &mut backend,
        );
        assert_eq!(add.exit_code, 0);
        assert!(add.stderr.is_empty());
        assert!(add.stdout.contains("\"prompt_mode\": \"env\""));
        assert!(add.stdout.contains("--output-format stream-json"));
        assert!(add.stdout.contains("--include-partial-messages"));
        assert!(add.stdout.contains("-p \\\"$FORGE_PROMPT_CONTENT\\\""));
    }

    #[test]
    fn profile_validation_paths() {
        let mut backend = InMemoryProfileBackend::default();

        let missing_name = run_for_test(&["profile", "add", "codex"], &mut backend);
        assert_eq!(missing_name.exit_code, 1);
        assert_eq!(missing_name.stderr, "--name is required\n");

        let bad_mode = run_for_test(
            &[
                "profile",
                "add",
                "codex",
                "--name",
                "alpha",
                "--prompt-mode",
                "interactive",
            ],
            &mut backend,
        );
        assert_eq!(bad_mode.exit_code, 1);
        assert!(bad_mode.stderr.contains("invalid prompt mode"));

        let bad_cooldown = run_for_test(&["profile", "cooldown", "set", "alpha"], &mut backend);
        assert_eq!(bad_cooldown.exit_code, 1);
        assert_eq!(bad_cooldown.stderr, "--until is required\n");
    }

    #[test]
    fn parse_alias_line_supports_common_forms() {
        let parsed = parse_alias_line("alias oc1='OPENCODE_HOME=~/.oc1 opencode run'");
        assert_eq!(
            parsed,
            Some((
                "oc1".to_string(),
                "OPENCODE_HOME=~/.oc1 opencode run".to_string()
            ))
        );

        let parsed = parse_alias_line("alias codex2=\"CODEX_HOME=~/.codex2 codex exec\"");
        assert_eq!(
            parsed,
            Some((
                "codex2".to_string(),
                "CODEX_HOME=~/.codex2 codex exec".to_string()
            ))
        );

        assert_eq!(parse_alias_line("export X=1"), None);
        assert_eq!(parse_alias_line("# comment"), None);
    }

    #[test]
    fn parse_alias_lines_is_deterministic_and_extracts_hints() {
        let lines = vec![
            "alias zz='echo ignored'".to_string(),
            "alias oc1='OPENCODE_HOME=~/.oc1 opencode run'".to_string(),
            "alias codex2='CODEX_HOME=~/.codex2 codex exec'".to_string(),
            "alias cc3='CLAUDE_HOME=~/.claude3 claude -p \"$FORGE_PROMPT_CONTENT\"'".to_string(),
            "alias oc1='OPENCODE_HOME=~/.override opencode run'".to_string(),
        ];
        let detected = parse_alias_lines(&lines);
        assert_eq!(detected.len(), 3);
        assert_eq!(detected[0].name, "cc3");
        assert_eq!(detected[0].harness, "claude");
        assert_eq!(detected[0].auth_home, "~/.claude3");
        assert_eq!(detected[1].name, "codex2");
        assert_eq!(detected[1].harness, "codex");
        assert_eq!(detected[1].auth_home, "~/.codex2");
        assert_eq!(detected[2].name, "oc1");
        assert_eq!(detected[2].harness, "opencode");
        assert_eq!(detected[2].auth_home, "~/.oc1");
    }

    #[test]
    fn instantiate_profiles_from_detection_creates_alias_and_harness_stubs() {
        let detection = ProfileInitDetection {
            harnesses: vec!["claude".to_string(), "codex".to_string()],
            aliases: vec![AliasDetection {
                name: "oc1".to_string(),
                harness: "opencode".to_string(),
                command: "OPENCODE_HOME=~/.oc1 opencode run".to_string(),
                auth_home: "~/.oc1".to_string(),
            }],
        };
        let mut backend = InMemoryProfileBackend::default();
        let result = match instantiate_profiles_from_detection(&mut backend, &detection) {
            Ok(result) => result,
            Err(err) => panic!("expected profile instantiation success: {err}"),
        };
        assert_eq!(result.imported, 3);
        assert_eq!(
            result.profiles,
            vec!["oc1".to_string(), "claude".to_string(), "codex".to_string()]
        );
        assert_eq!(result.harnesses, vec!["claude", "codex"]);

        let list = run_for_test(&["profile", "ls", "--json"], &mut backend);
        assert_eq!(list.exit_code, 0);
        assert!(list.stdout.contains("\"name\": \"oc1\""));
        assert!(list.stdout.contains("\"auth_home\": \"~/.oc1\""));
        assert!(list.stdout.contains("\"name\": \"claude\""));
        assert!(list.stdout.contains("\"name\": \"codex\""));
    }

    #[test]
    fn profile_init_uses_alias_fixture_file() {
        let _env_lock = env_test_lock();
        let _skip_zsh = EnvVarGuard::set("FORGE_PROFILE_INIT_SKIP_ZSH_ALIAS", "1");
        let _path_guard = EnvVarGuard::set("PATH", "/dev/null");
        let alias_path = temp_path("aliases");
        let _ = fs::write(
            &alias_path,
            "alias amp1='AMP_HOME=~/.amp1 amp run'\nalias d1='factory run'\n",
        );
        let _alias_file = EnvVarGuard::set(
            "FORGE_PROFILE_INIT_ALIAS_FILE",
            alias_path.to_string_lossy().as_ref(),
        );

        let mut backend = InMemoryProfileBackend::default();
        let out = run_for_test(&["profile", "init", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = match serde_json::from_str(&out.stdout) {
            Ok(parsed) => parsed,
            Err(err) => panic!("failed to parse profile init json output: {err}"),
        };
        assert_eq!(parsed["imported"], 2);
        assert_eq!(parsed["aliases"][0]["name"], "amp1");
        assert_eq!(parsed["aliases"][1]["name"], "d1");

        let _ = fs::remove_file(alias_path);
    }

    #[test]
    fn detect_installed_harnesses_from_path_fixture() {
        let _env_lock = env_test_lock();
        let bin_dir = temp_path("harness-bin");
        if let Err(err) = fs::create_dir_all(&bin_dir) {
            panic!("failed to create harness fixture bin dir: {err}");
        }
        if let Err(err) = fs::write(bin_dir.join("codex"), "") {
            panic!("failed to write codex fixture binary: {err}");
        }
        if let Err(err) = fs::write(bin_dir.join("factory"), "") {
            panic!("failed to write factory fixture binary: {err}");
        }
        let _path_guard = EnvVarGuard::set("PATH", bin_dir.to_string_lossy().as_ref());

        let harnesses = detect_installed_harnesses();
        assert_eq!(harnesses, vec!["codex".to_string(), "droid".to_string()]);

        let _ = fs::remove_dir_all(bin_dir);
    }

    fn assert_duration_secs(input: &str, expected_secs: i64) {
        match parse_go_duration(input) {
            Some(d) => assert_eq!(d.num_seconds(), expected_secs, "input: {input}"),
            None => panic!("parse_go_duration({input:?}) returned None"),
        }
    }

    #[test]
    fn parse_go_duration_basic_units() {
        assert_duration_secs("1h", 3600);
        assert_duration_secs("30m", 1800);
        assert_duration_secs("5s", 5);

        match parse_go_duration("500ms") {
            Some(d) => assert_eq!(d.num_milliseconds(), 500),
            None => panic!("parse_go_duration(\"500ms\") returned None"),
        }
        match parse_go_duration("100us") {
            Some(d) => assert_eq!(d.num_microseconds(), Some(100)),
            None => panic!("parse_go_duration(\"100us\") returned None"),
        }
        match parse_go_duration("200ns") {
            Some(d) => assert_eq!(d.num_nanoseconds(), Some(200)),
            None => panic!("parse_go_duration(\"200ns\") returned None"),
        }
    }

    #[test]
    fn parse_go_duration_compound() {
        assert_duration_secs("2h30m", 9000);
        assert_duration_secs("1h30m45s", 5445);
    }

    #[test]
    fn parse_go_duration_rejects_invalid() {
        assert!(parse_go_duration("").is_none());
        assert!(parse_go_duration("abc").is_none());
        assert!(parse_go_duration("1d").is_none());
        assert!(parse_go_duration("1w").is_none());
    }

    #[test]
    fn parse_time_or_duration_rfc3339() {
        match parse_time_or_duration("2026-12-31T00:00:00Z") {
            Ok(result) => assert_eq!(result, "2026-12-31T00:00:00Z"),
            Err(err) => panic!("parse_time_or_duration rfc3339 failed: {err}"),
        }
    }

    #[test]
    fn parse_time_or_duration_duration_resolves_to_rfc3339() {
        match parse_time_or_duration("1h") {
            Ok(result) => {
                assert!(result.ends_with('Z'));
                assert!(
                    chrono::DateTime::parse_from_rfc3339(&result).is_ok(),
                    "not valid RFC3339: {result}"
                );
            }
            Err(err) => panic!("parse_time_or_duration(\"1h\") failed: {err}"),
        }
    }

    #[test]
    fn parse_time_or_duration_rejects_invalid() {
        match parse_time_or_duration("not-a-time") {
            Err(msg) => assert!(msg.contains("invalid time"), "unexpected error: {msg}"),
            Ok(val) => panic!("expected error, got: {val}"),
        }
    }

    #[test]
    fn parse_time_or_duration_empty_fails() {
        assert!(parse_time_or_duration("").is_err());
    }

    #[test]
    fn cooldown_set_with_duration_resolves_to_timestamp() {
        let mut backend = InMemoryProfileBackend::default();
        let _ = run_for_test(
            &["profile", "add", "claude", "--name", "gamma"],
            &mut backend,
        );

        let set = run_for_test(
            &["profile", "cooldown", "set", "gamma", "--until", "1h"],
            &mut backend,
        );
        assert_eq!(set.exit_code, 0, "stderr: {}", set.stderr);
        assert!(set.stderr.is_empty());
        // The human output should contain a resolved RFC3339 timestamp, not "1h".
        assert!(
            !set.stdout.contains("1h"),
            "should resolve duration to timestamp: {}",
            set.stdout
        );
        assert!(set.stdout.contains("cooldown set to 20"));
    }

    #[test]
    fn cooldown_set_with_invalid_time_fails() {
        let mut backend = InMemoryProfileBackend::default();
        let _ = run_for_test(
            &["profile", "add", "claude", "--name", "delta"],
            &mut backend,
        );

        let set = run_for_test(
            &[
                "profile",
                "cooldown",
                "set",
                "delta",
                "--until",
                "not-a-time",
            ],
            &mut backend,
        );
        assert_eq!(set.exit_code, 1);
        assert!(set.stderr.contains("invalid time"));
    }

    #[test]
    fn catalog_init_and_auth_status_flow() {
        let _lock = env_test_lock();
        let catalog_root = temp_path("catalog-flow");
        let _data_dir = EnvVarGuard::set("FORGE_DATA_DIR", catalog_root.to_string_lossy().as_ref());

        let mut backend = InMemoryProfileBackend::default();
        let _ = run_for_test(
            &["profile", "add", "claude", "--name", "claude-main"],
            &mut backend,
        );
        let _ = run_for_test(
            &["profile", "add", "codex", "--name", "codex-main"],
            &mut backend,
        );
        let _ = run_for_test(
            &["profile", "add", "codex", "--name", "codex-shadow"],
            &mut backend,
        );

        let init = run_for_test(
            &["profile", "catalog", "init", "--node", "node-a"],
            &mut backend,
        );
        assert_eq!(init.exit_code, 0, "stderr={}", init.stderr);
        assert!(init.stdout.contains("Provisioned node node-a"));

        let auth = run_for_test(
            &[
                "profile",
                "catalog",
                "auth",
                "--node",
                "node-a",
                "--profile",
                "Codex2",
                "--status",
                "ok",
            ],
            &mut backend,
        );
        assert_eq!(auth.exit_code, 0, "stderr={}", auth.stderr);

        let status = run_for_test(&["profile", "--json", "catalog", "status"], &mut backend);
        assert_eq!(status.exit_code, 0, "stderr={}", status.stderr);
        assert!(status.stdout.contains("\"node-a\""));
        assert!(status.stdout.contains("\"Codex2\""));
        assert!(status.stdout.contains("\"ok\""));

        let _ = fs::remove_dir_all(catalog_root);
    }

    #[test]
    fn catalog_init_requires_node_flag() {
        let _lock = env_test_lock();
        let mut backend = InMemoryProfileBackend::default();
        let out = run_for_test(&["profile", "catalog", "init"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--node is required"));
    }
}
