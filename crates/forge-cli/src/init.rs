use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

// ---------------------------------------------------------------------------
// Backend trait – abstracts filesystem for testability
// ---------------------------------------------------------------------------

pub trait InitBackend {
    fn resolve_working_dir(&self) -> Result<PathBuf, String>;
    fn create_dir_all(&self, path: &Path) -> Result<(), String>;
    fn file_exists(&self, path: &Path) -> bool;
    fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), String>;
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, String>;
    fn read_dir_md_files(&self, dir: &Path) -> Result<Vec<(String, Vec<u8>)>, String>;
}

// ---------------------------------------------------------------------------
// Filesystem backend (production)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
pub struct FilesystemInitBackend;

impl InitBackend for FilesystemInitBackend {
    fn resolve_working_dir(&self) -> Result<PathBuf, String> {
        env::current_dir().map_err(|err| format!("failed to resolve working directory: {err}"))
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        fs::create_dir_all(path)
            .map_err(|err| format!("failed to create {}: {err}", path.display()))
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), String> {
        fs::write(path, data).map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    fn read_file(&self, path: &Path) -> Result<Vec<u8>, String> {
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))
    }

    fn read_dir_md_files(&self, dir: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
        let entries =
            fs::read_dir(dir).map_err(|err| format!("failed to read prompts directory: {err}"))?;
        let mut results = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|err| format!("failed to read prompts directory entry: {err}"))?;
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let data = fs::read(&path)
                .map_err(|err| format!("failed to open prompt {}: {err}", path.display()))?;
            results.push((name, data));
        }
        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// In-memory backend (testing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct InMemoryInitBackend {
    pub working_dir: PathBuf,
    pub dirs_created: std::cell::RefCell<Vec<PathBuf>>,
    pub files: std::cell::RefCell<std::collections::BTreeMap<PathBuf, Vec<u8>>>,
    pub prompts_from_files: Vec<(String, Vec<u8>)>,
}

impl InMemoryInitBackend {
    pub fn new(working_dir: &str) -> Self {
        Self {
            working_dir: PathBuf::from(working_dir),
            ..Default::default()
        }
    }

    pub fn with_file(self, path: &str, content: &[u8]) -> Self {
        self.files
            .borrow_mut()
            .insert(PathBuf::from(path), content.to_vec());
        self
    }

    pub fn with_prompts_from(mut self, files: Vec<(String, Vec<u8>)>) -> Self {
        self.prompts_from_files = files;
        self
    }

    pub fn file_content(&self, path: &str) -> Option<Vec<u8>> {
        self.files.borrow().get(&PathBuf::from(path)).cloned()
    }

    pub fn has_dir(&self, path: &str) -> bool {
        self.dirs_created
            .borrow()
            .iter()
            .any(|d| d == &PathBuf::from(path))
    }
}

impl InitBackend for InMemoryInitBackend {
    fn resolve_working_dir(&self) -> Result<PathBuf, String> {
        Ok(self.working_dir.clone())
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        self.dirs_created.borrow_mut().push(path.to_path_buf());
        Ok(())
    }

    fn file_exists(&self, path: &Path) -> bool {
        self.files.borrow().contains_key(path)
    }

    fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), String> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), data.to_vec());
        Ok(())
    }

    fn read_file(&self, path: &Path) -> Result<Vec<u8>, String> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| format!("file not found: {}", path.display()))
    }

    fn read_dir_md_files(&self, _dir: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
        Ok(self.prompts_from_files.clone())
    }
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct InitResult {
    repo_path: String,
    created: Vec<String>,
    skipped: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Human,
    Json,
    Jsonl,
}

// ---------------------------------------------------------------------------
// Parsed arguments
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    force: bool,
    prompts_from: Option<String>,
    no_create_prompt: bool,
    output: OutputMode,
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

pub fn run_from_env_with_backend(backend: &dyn InitBackend) -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_backend(&args, backend, &mut stdout, &mut stderr)
}

pub fn run_for_test(args: &[&str], backend: &dyn InitBackend) -> CommandOutput {
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
    backend: &dyn InitBackend,
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

// ---------------------------------------------------------------------------
// Core execution
// ---------------------------------------------------------------------------

fn execute(
    args: &[String],
    backend: &dyn InitBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    let repo_path = backend.resolve_working_dir()?;
    let forge_dir = repo_path.join(".forge");

    let mut created: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    // Create .forge subdirectories
    let subdirs = ["prompts", "templates", "sequences", "workflows", "ledgers"];
    for subdir in &subdirs {
        let path = forge_dir.join(subdir);
        backend.create_dir_all(&path)?;
        created.push(path.display().to_string());
    }

    // Write forge.yaml
    let config_path = forge_dir.join("forge.yaml");
    if write_if_missing(
        backend,
        &config_path,
        DEFAULT_FORGE_CONFIG.as_bytes(),
        parsed.force,
    )? {
        created.push(config_path.display().to_string());
    } else {
        skipped.push(config_path.display().to_string());
    }

    // Copy prompts from --prompts-from directory
    if let Some(ref prompts_from) = parsed.prompts_from {
        let prompts_dir = forge_dir.join("prompts");
        let md_files = backend.read_dir_md_files(Path::new(prompts_from))?;
        for (name, data) in md_files {
            let dest = prompts_dir.join(&name);
            backend.write_file(&dest, &data)?;
            created.push(dest.display().to_string());
        }
    }

    // Ensure .fmail/ in .gitignore
    let gitignore_path = repo_path.join(".gitignore");
    if ensure_gitignore_entry(backend, &gitignore_path, ".fmail/")? {
        created.push(format!("{} (.fmail/ entry)", gitignore_path.display()));
    }

    // Create PROMPT.md unless --no-create-prompt
    if !parsed.no_create_prompt {
        let prompt_path = repo_path.join("PROMPT.md");
        if write_if_missing(
            backend,
            &prompt_path,
            DEFAULT_PROMPT.as_bytes(),
            parsed.force,
        )? {
            created.push(prompt_path.display().to_string());
        } else {
            skipped.push(prompt_path.display().to_string());
        }
    }

    let result = InitResult {
        repo_path: repo_path.display().to_string(),
        created,
        skipped,
    };

    // Output
    match parsed.output {
        OutputMode::Json | OutputMode::Jsonl => {
            serde_json::to_writer(&mut *stdout, &result).map_err(|err| err.to_string())?;
            writeln!(stdout).map_err(|err| err.to_string())?;
        }
        OutputMode::Human => {
            writeln!(
                stdout,
                "Initialized Forge scaffolding in {}",
                result.repo_path
            )
            .map_err(|err| err.to_string())?;
            if !result.created.is_empty() {
                writeln!(stdout, "Created:").map_err(|err| err.to_string())?;
                for path in &result.created {
                    writeln!(stdout, "  - {path}").map_err(|err| err.to_string())?;
                }
            }
            if !result.skipped.is_empty() {
                writeln!(stdout, "Skipped:").map_err(|err| err.to_string())?;
                for path in &result.skipped {
                    writeln!(stdout, "  - {path}").map_err(|err| err.to_string())?;
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_if_missing(
    backend: &dyn InitBackend,
    path: &Path,
    data: &[u8],
    force: bool,
) -> Result<bool, String> {
    if !force && backend.file_exists(path) {
        return Ok(false);
    }
    backend.write_file(path, data)?;
    Ok(true)
}

fn ensure_gitignore_entry(
    backend: &dyn InitBackend,
    gitignore_path: &Path,
    entry: &str,
) -> Result<bool, String> {
    if !backend.file_exists(gitignore_path) {
        // No .gitignore exists, create one with the entry
        backend.write_file(gitignore_path, format!("{entry}\n").as_bytes())?;
        return Ok(true);
    }

    let content = backend.read_file(gitignore_path)?;
    let text = String::from_utf8_lossy(&content);

    // Check if entry already exists (exact line match)
    for line in split_lines(&text) {
        if line == entry {
            return Ok(false);
        }
    }

    // Append entry with proper newline handling
    let mut new_content = text.into_owned();
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(entry);
    new_content.push('\n');

    backend.write_file(gitignore_path, new_content.as_bytes())?;
    Ok(true)
}

fn split_lines(content: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = content.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'\n' {
            let mut end = i;
            if end > start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
            lines.push(&content[start..end]);
            start = i + 1;
        }
    }
    if start < content.len() {
        lines.push(&content[start..]);
    }
    lines
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;

    // Skip leading "init" token
    if args.get(index).is_some_and(|a| a == "init") {
        index += 1;
    }

    let mut force = false;
    let mut prompts_from: Option<String> = None;
    let mut no_create_prompt = false;
    let mut json = false;
    let mut jsonl = false;

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--force" | "-f" => {
                force = true;
                index += 1;
            }
            "--prompts-from" => {
                let val = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --prompts-from".to_string())?;
                prompts_from = Some(val.clone());
                index += 2;
            }
            "--no-create-prompt" => {
                no_create_prompt = true;
                index += 1;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
            }
            "--help" | "-h" => {
                // Help is handled at the router level; treat as no-op for now
                index += 1;
            }
            unknown => {
                return Err(format!("error: unknown argument for init: '{unknown}'"));
            }
        }
    }

    let output = if jsonl {
        OutputMode::Jsonl
    } else if json {
        OutputMode::Json
    } else {
        OutputMode::Human
    };

    Ok(ParsedArgs {
        force,
        prompts_from,
        no_create_prompt,
        output,
    })
}

// ---------------------------------------------------------------------------
// Default file contents (matching Go reference)
// ---------------------------------------------------------------------------

const DEFAULT_FORGE_CONFIG: &str = "# Forge loop config
# This file is committed with the repo.

default_prompt: PROMPT.md

# Optional ledger settings.
ledger:
  git_status: false
  git_diff_stat: false
";

const DEFAULT_PROMPT: &str = "# Prompt

Describe the task you want the loop to perform.
";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_scaffold() {
        let backend = InMemoryInitBackend::new("/repo");
        let out = run_for_test(&["init"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stderr.is_empty());

        // Directories created
        assert!(backend.has_dir("/repo/.forge/prompts"));
        assert!(backend.has_dir("/repo/.forge/templates"));
        assert!(backend.has_dir("/repo/.forge/sequences"));
        assert!(backend.has_dir("/repo/.forge/workflows"));
        assert!(backend.has_dir("/repo/.forge/ledgers"));

        // Files created
        assert!(backend.file_content("/repo/.forge/forge.yaml").is_some());
        assert!(backend.file_content("/repo/PROMPT.md").is_some());
        assert!(backend.file_content("/repo/.gitignore").is_some());

        // Human output
        assert!(out
            .stdout
            .contains("Initialized Forge scaffolding in /repo"));
        assert!(out.stdout.contains("Created:"));
    }

    #[test]
    fn init_no_create_prompt() {
        let backend = InMemoryInitBackend::new("/repo");
        let out = run_for_test(&["init", "--no-create-prompt"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(backend.file_content("/repo/PROMPT.md").is_none());
    }

    #[test]
    fn init_force_overwrites() {
        let backend =
            InMemoryInitBackend::new("/repo").with_file("/repo/.forge/forge.yaml", b"old config");
        let out = run_for_test(&["init", "--force", "--no-create-prompt"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let content = match backend.file_content("/repo/.forge/forge.yaml") {
            Some(value) => value,
            None => panic!("expected /repo/.forge/forge.yaml to exist"),
        };
        assert_eq!(content, DEFAULT_FORGE_CONFIG.as_bytes());
    }

    #[test]
    fn init_skips_existing_config() {
        let backend =
            InMemoryInitBackend::new("/repo").with_file("/repo/.forge/forge.yaml", b"old config");
        let out = run_for_test(&["init", "--no-create-prompt"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        // Config should NOT be overwritten
        let content = match backend.file_content("/repo/.forge/forge.yaml") {
            Some(value) => value,
            None => panic!("expected /repo/.forge/forge.yaml to exist"),
        };
        assert_eq!(content, b"old config");
        // Output should mention skipped
        assert!(out.stdout.contains("Skipped:"));
    }

    #[test]
    fn init_creates_gitignore_with_fmail() {
        let backend = InMemoryInitBackend::new("/repo");
        let out = run_for_test(&["init", "--no-create-prompt"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let content = match backend.file_content("/repo/.gitignore") {
            Some(value) => value,
            None => panic!("expected /repo/.gitignore to exist"),
        };
        assert_eq!(content, b".fmail/\n");
    }

    #[test]
    fn init_appends_fmail_to_existing_gitignore() {
        let backend =
            InMemoryInitBackend::new("/repo").with_file("/repo/.gitignore", b"node_modules/\n");
        let out = run_for_test(&["init", "--no-create-prompt"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let content = match backend.file_content("/repo/.gitignore") {
            Some(value) => value,
            None => panic!("expected /repo/.gitignore to exist"),
        };
        assert_eq!(content, b"node_modules/\n.fmail/\n");
    }

    #[test]
    fn init_does_not_duplicate_fmail_in_gitignore() {
        let backend = InMemoryInitBackend::new("/repo")
            .with_file("/repo/.gitignore", b"node_modules/\n.fmail/\n");
        let out = run_for_test(&["init", "--no-create-prompt"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let content = match backend.file_content("/repo/.gitignore") {
            Some(value) => value,
            None => panic!("expected /repo/.gitignore to exist"),
        };
        assert_eq!(content, b"node_modules/\n.fmail/\n");
    }

    #[test]
    fn init_prompts_from_copies_md_files() {
        let backend = InMemoryInitBackend::new("/repo")
            .with_prompts_from(vec![("review.md".to_string(), b"review prompt".to_vec())]);
        let out = run_for_test(
            &["init", "--prompts-from", "/seed", "--no-create-prompt"],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let content = match backend.file_content("/repo/.forge/prompts/review.md") {
            Some(value) => value,
            None => panic!("expected /repo/.forge/prompts/review.md to exist"),
        };
        assert_eq!(content, b"review prompt");
    }

    #[test]
    fn init_json_output() {
        let backend = InMemoryInitBackend::new("/repo");
        let out = run_for_test(&["init", "--json", "--no-create-prompt"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = match serde_json::from_str(&out.stdout) {
            Ok(value) => value,
            Err(err) => panic!("should be valid JSON: {err}"),
        };
        assert_eq!(parsed["repo_path"], "/repo");
        assert!(parsed["created"].is_array());
        assert!(parsed["skipped"].is_array());
    }

    #[test]
    fn init_unknown_flag_errors() {
        let backend = InMemoryInitBackend::new("/repo");
        let out = run_for_test(&["init", "--bogus"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown argument for init"));
    }

    #[test]
    fn init_force_short_flag() {
        let backend =
            InMemoryInitBackend::new("/repo").with_file("/repo/.forge/forge.yaml", b"old");
        let out = run_for_test(&["init", "-f", "--no-create-prompt"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let content = match backend.file_content("/repo/.forge/forge.yaml") {
            Some(value) => value,
            None => panic!("expected /repo/.forge/forge.yaml to exist"),
        };
        assert_eq!(content, DEFAULT_FORGE_CONFIG.as_bytes());
    }

    #[test]
    fn split_lines_handles_crlf() {
        let lines = split_lines("a\r\nb\nc\r\n");
        assert_eq!(lines, vec!["a", "b", "c"]);
    }

    #[test]
    fn split_lines_handles_trailing_content() {
        let lines = split_lines("a\nb");
        assert_eq!(lines, vec!["a", "b"]);
    }

    #[test]
    fn default_forge_config_matches_go() {
        assert!(DEFAULT_FORGE_CONFIG.contains("default_prompt: PROMPT.md"));
        assert!(DEFAULT_FORGE_CONFIG.contains("git_status: false"));
        assert!(DEFAULT_FORGE_CONFIG.contains("git_diff_stat: false"));
    }

    #[test]
    fn default_prompt_matches_go() {
        assert!(DEFAULT_PROMPT.starts_with("# Prompt"));
        assert!(DEFAULT_PROMPT.contains("Describe the task"));
    }
}
