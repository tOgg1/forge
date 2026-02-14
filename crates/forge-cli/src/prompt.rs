use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptBackendError {
    NotFound,
    Message(String),
}

pub trait PromptBackend {
    fn resolve_repo_path(&self) -> Result<PathBuf, String>;
    fn list_prompts(&self, repo_path: &Path) -> Result<Vec<String>, PromptBackendError>;
    fn read_prompt(
        &self,
        repo_path: &Path,
        prompt_name: &str,
    ) -> Result<String, PromptBackendError>;
    fn ensure_prompts_dir(&self, repo_path: &Path) -> Result<PathBuf, String>;
    fn copy_file(&self, source: &Path, dest: &Path) -> Result<(), String>;
    fn prompt_exists(&self, repo_path: &Path, prompt_name: &str) -> bool;
    fn edit_prompt(&self, repo_path: &Path, prompt_name: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy)]
pub struct FilesystemPromptBackend;

impl PromptBackend for FilesystemPromptBackend {
    fn resolve_repo_path(&self) -> Result<PathBuf, String> {
        env::current_dir().map_err(|err| err.to_string())
    }

    fn list_prompts(&self, repo_path: &Path) -> Result<Vec<String>, PromptBackendError> {
        let dir = repo_path.join(".forge").join("prompts");
        let entries = fs::read_dir(dir).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                PromptBackendError::NotFound
            } else {
                PromptBackendError::Message(err.to_string())
            }
        })?;

        let mut prompts = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|err| PromptBackendError::Message(err.to_string()))?;
            let file_type = entry
                .file_type()
                .map_err(|err| PromptBackendError::Message(err.to_string()))?;
            if file_type.is_dir() {
                continue;
            }

            let path = entry.path();
            if path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                && path.file_stem().and_then(|value| value.to_str()).is_some()
            {
                let stem = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default();
                prompts.push(stem.to_string());
            }
        }

        prompts.sort_unstable();
        Ok(prompts)
    }

    fn read_prompt(
        &self,
        repo_path: &Path,
        prompt_name: &str,
    ) -> Result<String, PromptBackendError> {
        let path = repo_path
            .join(".forge")
            .join("prompts")
            .join(format!("{prompt_name}.md"));
        fs::read_to_string(path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                PromptBackendError::NotFound
            } else {
                PromptBackendError::Message(err.to_string())
            }
        })
    }

    fn ensure_prompts_dir(&self, repo_path: &Path) -> Result<PathBuf, String> {
        let dir = repo_path.join(".forge").join("prompts");
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(dir)
    }

    fn copy_file(&self, source: &Path, dest: &Path) -> Result<(), String> {
        fs::copy(source, dest)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    fn prompt_exists(&self, repo_path: &Path, prompt_name: &str) -> bool {
        repo_path
            .join(".forge")
            .join("prompts")
            .join(format!("{prompt_name}.md"))
            .is_file()
    }

    fn edit_prompt(&self, repo_path: &Path, prompt_name: &str) -> Result<(), String> {
        let prompt_path = repo_path
            .join(".forge")
            .join("prompts")
            .join(format!("{prompt_name}.md"));

        let editor = env::var("EDITOR").map_err(|_| "EDITOR not set".to_string())?;
        let status = ProcessCommand::new(editor)
            .arg(prompt_path)
            .status()
            .map_err(|err| err.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("editor exited with {status}"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Help,
    List,
    Show { name: String },
    Validate { name: Option<String> },
    Add { name: String, source: PathBuf },
    Edit { name: String },
    SetDefault { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    command: Command,
    json: bool,
    jsonl: bool,
    quiet: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptDefinition {
    name: String,
    path: String,
    content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptValidationEntry {
    name: String,
    path: String,
    valid: bool,
    errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptValidationResult {
    valid: bool,
    checked: usize,
    results: Vec<PromptValidationEntry>,
}

pub fn run_from_env_with_backend(backend: &mut dyn PromptBackend) -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_backend(&args, backend, &mut stdout, &mut stderr)
}

pub fn run_for_test(args: &[&str], backend: &mut dyn PromptBackend) -> CommandOutput {
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
    backend: &mut dyn PromptBackend,
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
    backend: &mut dyn PromptBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    match parsed.command {
        Command::Help => {
            write_help(stdout).map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::List => {
            let repo_path = backend.resolve_repo_path()?;
            let prompts = match backend.list_prompts(&repo_path) {
                Ok(values) => values,
                Err(PromptBackendError::NotFound) => Vec::new(),
                Err(PromptBackendError::Message(message)) => return Err(message),
            };

            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &prompts, parsed.jsonl)?;
                return Ok(());
            }
            if prompts.is_empty() {
                writeln!(stdout, "No prompts found").map_err(|err| err.to_string())?;
                return Ok(());
            }
            for prompt in prompts {
                writeln!(stdout, "{prompt}").map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Show { name } => {
            let repo_path = backend.resolve_repo_path()?;
            let definition = load_prompt_definition(backend, &repo_path, &name)?;

            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &definition, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(stdout, "Prompt: {}", definition.name).map_err(|err| err.to_string())?;
            writeln!(stdout, "Path: {}", definition.path).map_err(|err| err.to_string())?;
            writeln!(stdout).map_err(|err| err.to_string())?;
            write!(stdout, "{}", definition.content).map_err(|err| err.to_string())?;
            if !definition.content.ends_with('\n') {
                writeln!(stdout).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Validate { name } => {
            let repo_path = backend.resolve_repo_path()?;
            let prompt_names = if let Some(prompt_name) = name {
                vec![prompt_name]
            } else {
                match backend.list_prompts(&repo_path) {
                    Ok(values) => values,
                    Err(PromptBackendError::NotFound) => Vec::new(),
                    Err(PromptBackendError::Message(message)) => return Err(message),
                }
            };

            let mut results = Vec::new();
            for prompt_name in prompt_names {
                match load_prompt_definition(backend, &repo_path, &prompt_name) {
                    Ok(definition) => {
                        let errors = validate_prompt_definition(&definition);
                        let valid = errors.is_empty();
                        results.push(PromptValidationEntry {
                            name: definition.name,
                            path: definition.path,
                            valid,
                            errors,
                        });
                    }
                    Err(err) => {
                        results.push(PromptValidationEntry {
                            name: prompt_name.clone(),
                            path: format!(".forge/prompts/{prompt_name}.md"),
                            valid: false,
                            errors: vec![err],
                        });
                    }
                }
            }

            let validation = PromptValidationResult {
                valid: results.iter().all(|entry| entry.valid),
                checked: results.len(),
                results,
            };

            if parsed.json || parsed.jsonl {
                write_serialized(stdout, &validation, parsed.jsonl)?;
            } else if validation.results.is_empty() {
                writeln!(stdout, "No prompts found").map_err(|err| err.to_string())?;
            } else {
                for entry in &validation.results {
                    if entry.valid {
                        writeln!(stdout, "valid: {}", entry.name).map_err(|err| err.to_string())?;
                    } else {
                        writeln!(stdout, "invalid: {}", entry.name)
                            .map_err(|err| err.to_string())?;
                        for error in &entry.errors {
                            writeln!(stdout, "- {}", error).map_err(|err| err.to_string())?;
                        }
                    }
                }
            }

            if !validation.valid {
                return Err("prompt validation failed".to_string());
            }
            Ok(())
        }
        Command::Add { name, source } => {
            let repo_path = backend.resolve_repo_path()?;
            let prompts_dir = backend.ensure_prompts_dir(&repo_path)?;
            let dest = prompts_dir.join(format!("{name}.md"));
            backend.copy_file(&source, &dest)?;

            if parsed.json || parsed.jsonl {
                #[derive(Serialize)]
                struct AddResponse {
                    path: String,
                    prompt: String,
                }
                let payload = AddResponse {
                    path: dest.display().to_string(),
                    prompt: name,
                };
                write_serialized(stdout, &payload, parsed.jsonl)?;
            } else if !parsed.quiet {
                writeln!(stdout, "Prompt \"{}\" added", name).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Edit { name } => {
            let repo_path = backend.resolve_repo_path()?;
            if !backend.prompt_exists(&repo_path, &name) {
                return Err(format!("prompt not found: {name}"));
            }
            backend.edit_prompt(&repo_path, &name)?;
            if !(parsed.quiet || parsed.json || parsed.jsonl) {
                writeln!(stdout, "Prompt \"{}\" updated", name).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::SetDefault { name } => {
            let repo_path = backend.resolve_repo_path()?;
            if !backend.prompt_exists(&repo_path, &name) {
                return Err(format!("prompt not found: {name}"));
            }
            let prompts_dir = backend.ensure_prompts_dir(&repo_path)?;
            let source = prompts_dir.join(format!("{name}.md"));
            let dest = prompts_dir.join("default.md");
            backend.copy_file(&source, &dest)?;

            if parsed.json || parsed.jsonl {
                #[derive(Serialize)]
                struct DefaultResponse {
                    default_prompt: String,
                }
                let payload = DefaultResponse {
                    default_prompt: name,
                };
                write_serialized(stdout, &payload, parsed.jsonl)?;
            } else if !parsed.quiet {
                writeln!(stdout, "Default prompt set to \"{}\"", name)
                    .map_err(|err| err.to_string())?;
            }
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
            quiet: false,
        });
    }

    let start = if args.first().is_some_and(|arg| arg == "prompt") {
        1
    } else {
        0
    };

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
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

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    let command = match subcommand.as_deref() {
        None | Some("help") | Some("-h") | Some("--help") => Command::Help,
        Some("ls") | Some("list") => {
            ensure_empty_args("prompt ls", &subcommand_args)?;
            Command::List
        }
        Some("show") | Some("get") => parse_show_args(&subcommand_args)?,
        Some("validate") => parse_validate_args(&subcommand_args)?,
        Some("add") => parse_add_args(&subcommand_args)?,
        Some("edit") => parse_edit_args(&subcommand_args)?,
        Some("set-default") => parse_set_default_args(&subcommand_args)?,
        Some(other) => return Err(format!("unknown prompt argument: {other}")),
    };

    Ok(ParsedArgs {
        command,
        json,
        jsonl,
        quiet,
    })
}

fn parse_show_args(args: &[String]) -> Result<Command, String> {
    match args.first() {
        Some(name) => {
            if args.len() > 1 {
                return Err(format!("unexpected argument for prompt show: {}", args[1]));
            }
            Ok(Command::Show { name: name.clone() })
        }
        None => Err("error: prompt show requires <name>".to_string()),
    }
}

fn parse_validate_args(args: &[String]) -> Result<Command, String> {
    match args.len() {
        0 => Ok(Command::Validate { name: None }),
        1 => Ok(Command::Validate {
            name: Some(args[0].clone()),
        }),
        _ => Err(format!(
            "unexpected argument for prompt validate: {}",
            args[1]
        )),
    }
}

fn parse_add_args(args: &[String]) -> Result<Command, String> {
    if args.len() != 2 {
        return Err("error: prompt add requires <name> <path>".to_string());
    }
    let name = args[0].clone();
    let source = PathBuf::from(args[1].clone());
    Ok(Command::Add { name, source })
}

fn parse_edit_args(args: &[String]) -> Result<Command, String> {
    match args.first() {
        Some(name) => {
            if args.len() > 1 {
                return Err(format!("unexpected argument for prompt edit: {}", args[1]));
            }
            Ok(Command::Edit { name: name.clone() })
        }
        None => Err("error: prompt edit requires <name>".to_string()),
    }
}

fn parse_set_default_args(args: &[String]) -> Result<Command, String> {
    match args.first() {
        Some(name) => {
            if args.len() > 1 {
                return Err(format!(
                    "unexpected argument for prompt set-default: {}",
                    args[1]
                ));
            }
            Ok(Command::SetDefault { name: name.clone() })
        }
        None => Err("error: prompt set-default requires <name>".to_string()),
    }
}

fn ensure_empty_args(command: &str, args: &[String]) -> Result<(), String> {
    if let Some(first) = args.first() {
        return Err(format!("unexpected argument for {command}: {first}"));
    }
    Ok(())
}

fn load_prompt_definition(
    backend: &dyn PromptBackend,
    repo_path: &Path,
    name: &str,
) -> Result<PromptDefinition, String> {
    let content = backend
        .read_prompt(repo_path, name)
        .map_err(|err| match err {
            PromptBackendError::NotFound => format!("prompt not found: {name}"),
            PromptBackendError::Message(message) => message,
        })?;

    Ok(PromptDefinition {
        name: name.to_string(),
        path: format!(".forge/prompts/{name}.md"),
        content,
    })
}

fn validate_prompt_definition(definition: &PromptDefinition) -> Vec<String> {
    let mut errors = Vec::new();
    if definition.content.trim().is_empty() {
        errors.push("content is empty".to_string());
    }
    errors
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
    writeln!(output, "{text}").map_err(|err| err.to_string())?;
    Ok(())
}

fn write_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "forge prompt - Manage per-repo prompt templates")?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  ls")?;
    writeln!(stdout, "  show <name>")?;
    writeln!(stdout, "  validate [name]")?;
    writeln!(stdout, "  add <name> <path>")?;
    writeln!(stdout, "  edit <name>")?;
    writeln!(stdout, "  set-default <name>")?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct InMemoryPromptBackend {
        repo_path: PathBuf,
        prompts: BTreeMap<String, String>,
    }

    impl InMemoryPromptBackend {
        fn with_prompt(mut self, name: &str, content: &str) -> Self {
            self.prompts.insert(name.to_string(), content.to_string());
            self
        }
    }

    impl PromptBackend for InMemoryPromptBackend {
        fn resolve_repo_path(&self) -> Result<PathBuf, String> {
            Ok(self.repo_path.clone())
        }

        fn list_prompts(&self, _repo_path: &Path) -> Result<Vec<String>, PromptBackendError> {
            Ok(self.prompts.keys().cloned().collect())
        }

        fn read_prompt(
            &self,
            _repo_path: &Path,
            prompt_name: &str,
        ) -> Result<String, PromptBackendError> {
            self.prompts
                .get(prompt_name)
                .cloned()
                .ok_or(PromptBackendError::NotFound)
        }

        fn ensure_prompts_dir(&self, repo_path: &Path) -> Result<PathBuf, String> {
            Ok(repo_path.join(".forge").join("prompts"))
        }

        fn copy_file(&self, _source: &Path, _dest: &Path) -> Result<(), String> {
            Ok(())
        }

        fn prompt_exists(&self, _repo_path: &Path, prompt_name: &str) -> bool {
            self.prompts.contains_key(prompt_name)
        }

        fn edit_prompt(&self, _repo_path: &Path, _prompt_name: &str) -> Result<(), String> {
            Ok(())
        }
    }

    fn parse_json(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap_or_else(|err| panic!("invalid json output: {err}"))
    }

    #[test]
    fn parse_show_requires_name() {
        let err = parse_args(&["prompt".to_string(), "show".to_string()]).unwrap_err();
        assert!(err.contains("prompt show requires <name>"));
    }

    #[test]
    fn parse_validate_accepts_optional_name() {
        let parsed = parse_args(&["prompt".to_string(), "validate".to_string()])
            .unwrap_or_else(|err| panic!("parse args: {err}"));
        assert_eq!(parsed.command, Command::Validate { name: None });

        let parsed = parse_args(&[
            "prompt".to_string(),
            "validate".to_string(),
            "review".to_string(),
        ])
        .unwrap_or_else(|err| panic!("parse args: {err}"));
        assert_eq!(
            parsed.command,
            Command::Validate {
                name: Some("review".to_string())
            }
        );
    }

    #[test]
    fn show_json_outputs_prompt_definition() {
        let mut backend = InMemoryPromptBackend::default().with_prompt("review", "Look for bugs");
        let out = run_for_test(&["prompt", "--json", "show", "review"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["name"], "review");
        assert_eq!(parsed["path"], ".forge/prompts/review.md");
        assert_eq!(parsed["content"], "Look for bugs");
    }

    #[test]
    fn validate_returns_nonzero_for_empty_prompt() {
        let mut backend = InMemoryPromptBackend::default().with_prompt("bad", "  \n");
        let out = run_for_test(&["prompt", "validate"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.contains("invalid: bad"));
        assert!(out.stdout.contains("content is empty"));
        assert!(out.stderr.contains("prompt validation failed"));
    }

    #[test]
    fn validate_json_reports_errors() {
        let mut backend = InMemoryPromptBackend::default().with_prompt("bad", "");
        let out = run_for_test(&["prompt", "--json", "validate"], &mut backend);
        assert_eq!(out.exit_code, 1);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["valid"], false);
        assert_eq!(parsed["checked"], 1);
        assert_eq!(parsed["results"][0]["name"], "bad");
        assert_eq!(parsed["results"][0]["valid"], false);
        assert_eq!(parsed["results"][0]["errors"][0], "content is empty");
    }

    #[test]
    fn validate_succeeds_for_non_empty_prompts() {
        let mut backend = InMemoryPromptBackend::default()
            .with_prompt("review", "Review code")
            .with_prompt("design", "Draft design");
        let out = run_for_test(&["prompt", "validate"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stdout.contains("valid: review"));
        assert!(out.stdout.contains("valid: design"));
    }
}
