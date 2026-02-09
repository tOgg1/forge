use std::collections::BTreeMap;
use std::env;
use std::io::Write;
use std::path::Path;

use serde::Serialize;

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Help,
    List,
    Add(ProfileCreateInput),
    Edit { name: String, patch: ProfilePatch },
    Remove { name: String },
    Init,
    Doctor { name: String },
    CooldownSet { name: String, until: String },
    CooldownClear { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    command: Command,
    json: bool,
    jsonl: bool,
}

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
            if parsed.json || parsed.jsonl {
                let payload = serde_json::json!({"imported": 0});
                write_serialized(stdout, &payload, parsed.jsonl)?;
            } else {
                writeln!(stdout, "No shell aliases found").map_err(|err| err.to_string())?;
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
            let profile = backend.set_cooldown(&name, &until)?;
            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &profile, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(
                stdout,
                "Profile \"{}\" cooldown set to {}",
                profile.name, until
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

fn normalize_harness(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "pi" | "opencode" | "codex" | "claude" | "claude-code" | "droid" | "factory" => {
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
        "claude" => "stdin",
        _ => "path",
    }
}

fn default_command_template(harness: &str) -> &'static str {
    match harness {
        "codex" => "codex exec",
        "claude" => "claude --print",
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
    use super::{run_for_test, InMemoryProfileBackend};

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
}
