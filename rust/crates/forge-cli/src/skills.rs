use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A directory entry from walk_dir: (relative_path, is_directory, file_contents).
pub type DirEntry = (String, bool, Option<Vec<u8>>);

/// Output from running the skills command (test helper).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Minimal profile config for skills installation (mirrors Go's config.ProfileConfig subset).
#[derive(Debug, Clone)]
pub struct SkillsProfile {
    pub name: String,
    pub harness: String,
    pub auth_home: String,
}

/// Minimal pool config (mirrors Go's config.PoolConfig subset).
#[derive(Debug, Clone)]
pub struct SkillsPool {
    pub name: String,
    pub profiles: Vec<String>,
}

/// Minimal config required for skills (mirrors Go's config.Config subset).
#[derive(Debug, Clone)]
pub struct SkillsConfig {
    pub profiles: Vec<SkillsProfile>,
    pub pools: Vec<SkillsPool>,
    pub default_pool: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SkillsConfigYaml {
    #[serde(default)]
    profiles: Vec<SkillsProfileYaml>,
    #[serde(default)]
    pools: Vec<SkillsPoolYaml>,
    #[serde(default)]
    default_pool: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SkillsProfileYaml {
    #[serde(default)]
    name: String,
    #[serde(default)]
    harness: String,
    #[serde(default)]
    auth_home: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SkillsPoolYaml {
    #[serde(default)]
    name: String,
    #[serde(default)]
    profiles: Vec<String>,
}

/// Result of installing skills to one destination.
#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub dest: String,
    pub harnesses: Vec<String>,
    pub created: Vec<String>,
    pub skipped: Vec<String>,
}

/// Top-level JSON output from `skills bootstrap`.
#[derive(Debug, Clone, Serialize)]
struct BootstrapOutput {
    source: String,
    installed: Vec<InstallResult>,
}

// ---------------------------------------------------------------------------
// Builtin skills (embedded at compile time)
// ---------------------------------------------------------------------------

/// Embedded builtin skill files. Each entry is (relative_path, contents).
fn builtin_skill_files() -> Vec<(&'static str, &'static [u8])> {
    vec![
        (
            "agent-communication/SKILL.md",
            include_bytes!("../../../../internal/skills/builtin/agent-communication/SKILL.md"),
        ),
        (
            "agent-communication/references/fmail-quickref.md",
            include_bytes!(
                "../../../../internal/skills/builtin/agent-communication/references/fmail-quickref.md"
            ),
        ),
        (
            "issue-tracking/SKILL.md",
            include_bytes!("../../../../internal/skills/builtin/issue-tracking/SKILL.md"),
        ),
        (
            "issue-tracking/references/tk-quickref.md",
            include_bytes!(
                "../../../../internal/skills/builtin/issue-tracking/references/tk-quickref.md"
            ),
        ),
        (
            "session-protocol/SKILL.md",
            include_bytes!("../../../../internal/skills/builtin/session-protocol/SKILL.md"),
        ),
        (
            "session-protocol/references/end-session-checklist.md",
            include_bytes!(
                "../../../../internal/skills/builtin/session-protocol/references/end-session-checklist.md"
            ),
        ),
        (
            "workflow-pattern/SKILL.md",
            include_bytes!("../../../../internal/skills/builtin/workflow-pattern/SKILL.md"),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Backend trait – abstracts filesystem + config for testability
// ---------------------------------------------------------------------------

pub trait SkillsBackend {
    /// Return the current working directory.
    fn resolve_working_dir(&self) -> Result<PathBuf, String>;
    /// Load forge config (or None if not available).
    fn load_config(&self) -> Result<Option<SkillsConfig>, String>;
    /// Check if a path is an existing directory.
    fn is_dir(&self, path: &Path) -> bool;
    /// Check if a path exists (file or directory).
    fn path_exists(&self, path: &Path) -> bool;
    /// Create directory and all parents.
    fn create_dir_all(&self, path: &Path) -> Result<(), String>;
    /// Write data to a file.
    fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), String>;
    /// Walk a source directory recursively, returning entries as `DirEntry` tuples.
    #[allow(clippy::type_complexity)]
    fn walk_dir(&self, source: &Path) -> Result<Vec<DirEntry>, String>;
    /// User home directory.
    fn home_dir(&self) -> Result<PathBuf, String>;
}

// ---------------------------------------------------------------------------
// Filesystem backend (production)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
pub struct FilesystemSkillsBackend;

impl SkillsBackend for FilesystemSkillsBackend {
    fn resolve_working_dir(&self) -> Result<PathBuf, String> {
        std::env::current_dir().map_err(|err| format!("failed to resolve working directory: {err}"))
    }

    fn load_config(&self) -> Result<Option<SkillsConfig>, String> {
        let config_path = resolve_config_path(&self.home_dir()?);
        let raw = match std::fs::read_to_string(&config_path) {
            Ok(value) => value,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(format!(
                    "failed to read config file {}: {err}",
                    config_path.display()
                ))
            }
        };

        let parsed = parse_skills_config_yaml(&raw).map_err(|err| {
            format!(
                "failed to parse config file {}: {err}",
                config_path.display()
            )
        })?;
        Ok(Some(parsed))
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        std::fs::create_dir_all(path)
            .map_err(|err| format!("failed to create {}: {err}", path.display()))
    }

    fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), String> {
        std::fs::write(path, data)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    fn walk_dir(&self, source: &Path) -> Result<Vec<DirEntry>, String> {
        let mut entries = Vec::new();
        walk_dir_recursive(source, source, &mut entries)?;
        Ok(entries)
    }

    fn home_dir(&self) -> Result<PathBuf, String> {
        std::env::var("HOME")
            .map(PathBuf::from)
            .map_err(|_| "failed to get home directory".to_string())
    }
}

fn resolve_config_path(home: &Path) -> PathBuf {
    if let Some(path) = std::env::var_os("FORGE_CONFIG_PATH") {
        return PathBuf::from(path);
    }
    home.join(".config").join("forge").join("config.yaml")
}

fn parse_skills_config_yaml(raw: &str) -> Result<SkillsConfig, String> {
    let parsed: SkillsConfigYaml =
        serde_yaml::from_str(raw).map_err(|err| format!("yaml decode error: {err}"))?;
    Ok(SkillsConfig {
        profiles: parsed
            .profiles
            .into_iter()
            .filter(|profile| !profile.name.trim().is_empty() && !profile.harness.trim().is_empty())
            .map(|profile| SkillsProfile {
                name: profile.name,
                harness: profile.harness,
                auth_home: profile.auth_home,
            })
            .collect(),
        pools: parsed
            .pools
            .into_iter()
            .filter(|pool| !pool.name.trim().is_empty())
            .map(|pool| SkillsPool {
                name: pool.name,
                profiles: pool.profiles,
            })
            .collect(),
        default_pool: parsed.default_pool,
    })
}

fn walk_dir_recursive(root: &Path, current: &Path, out: &mut Vec<DirEntry>) -> Result<(), String> {
    let entries = std::fs::read_dir(current)
        .map_err(|err| format!("failed to read directory {}: {err}", current.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|err| format!("failed to read entry in {}: {err}", current.display()))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|err| format!("failed to compute relative path: {err}"))?
            .to_string_lossy()
            .to_string();
        if path.is_dir() {
            out.push((rel, true, None));
            walk_dir_recursive(root, &path, out)?;
        } else {
            let data = std::fs::read(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            out.push((rel, false, Some(data)));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// In-memory backend (testing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct InMemorySkillsBackend {
    pub working_dir: PathBuf,
    pub home: PathBuf,
    pub config: Option<SkillsConfig>,
    pub dirs: RefCell<Vec<PathBuf>>,
    pub files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    /// Source directory content for walk_dir.
    pub source_entries: Vec<DirEntry>,
}

impl InMemorySkillsBackend {
    pub fn new(working_dir: &str) -> Self {
        Self {
            working_dir: PathBuf::from(working_dir),
            home: PathBuf::from("/home/testuser"),
            ..Default::default()
        }
    }

    pub fn with_config(mut self, config: SkillsConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_dir(self, path: &str) -> Self {
        self.dirs.borrow_mut().push(PathBuf::from(path));
        self
    }

    pub fn with_source_entries(mut self, entries: Vec<DirEntry>) -> Self {
        self.source_entries = entries;
        self
    }

    pub fn written_files(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        self.files.borrow().clone()
    }
}

impl SkillsBackend for InMemorySkillsBackend {
    fn resolve_working_dir(&self) -> Result<PathBuf, String> {
        Ok(self.working_dir.clone())
    }

    fn load_config(&self) -> Result<Option<SkillsConfig>, String> {
        Ok(self.config.clone())
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.dirs.borrow().iter().any(|d| d == path)
    }

    fn path_exists(&self, path: &Path) -> bool {
        self.dirs.borrow().iter().any(|d| d == path) || self.files.borrow().contains_key(path)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        self.dirs.borrow_mut().push(path.to_path_buf());
        Ok(())
    }

    fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), String> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), data.to_vec());
        Ok(())
    }

    fn walk_dir(&self, _source: &Path) -> Result<Vec<DirEntry>, String> {
        Ok(self.source_entries.clone())
    }

    fn home_dir(&self) -> Result<PathBuf, String> {
        Ok(self.home.clone())
    }
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

enum SubCommand {
    Help,
    Bootstrap(BootstrapArgs),
}

struct BootstrapArgs {
    force: bool,
    path: String,
    all_profiles: bool,
    json: bool,
    jsonl: bool,
}

fn parse_args(args: &[String]) -> Result<SubCommand, String> {
    // args[0] == "skills"
    if args.len() < 2 {
        return Ok(SubCommand::Help);
    }

    match args[1].as_str() {
        "help" | "--help" | "-h" => Ok(SubCommand::Help),
        "bootstrap" => {
            let mut force = false;
            let mut path = String::new();
            let mut all_profiles = false;
            let mut json = false;
            let mut jsonl = false;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--force" | "-f" => force = true,
                    "--path" => {
                        i += 1;
                        if i >= args.len() {
                            return Err("--path requires a value".to_string());
                        }
                        path = args[i].clone();
                    }
                    "--all-profiles" => all_profiles = true,
                    "--json" => json = true,
                    "--jsonl" => jsonl = true,
                    other => {
                        return Err(format!("unknown flag for skills bootstrap: {other}"));
                    }
                }
                i += 1;
            }
            Ok(SubCommand::Bootstrap(BootstrapArgs {
                force,
                path,
                all_profiles,
                json,
                jsonl,
            }))
        }
        other => Err(format!("unknown skills subcommand: {other}")),
    }
}

// ---------------------------------------------------------------------------
// Core execution
// ---------------------------------------------------------------------------

fn resolve_harness_dest(
    base_dir: &str,
    harness: &str,
    auth_home: &str,
    home: &Path,
) -> Option<String> {
    if !auth_home.is_empty() {
        return Some(format!("{}/skills", auth_home));
    }

    if !base_dir.is_empty() {
        return match harness {
            "codex" => Some(format!("{base_dir}/.codex/skills")),
            "claude" | "claude_code" => Some(format!("{base_dir}/.claude/skills")),
            "opencode" => Some(format!("{base_dir}/.opencode/skills")),
            "pi" => Some(format!("{base_dir}/.pi/skills")),
            _ => None,
        };
    }

    match harness {
        "codex" => Some(format!("{}/.codex/skills", home.display())),
        "claude" | "claude_code" => Some(format!("{}/.claude/skills", home.display())),
        "opencode" => Some(format!("{}/.config/opencode/skills", home.display())),
        "pi" => Some(format!("{}/.pi/skills", home.display())),
        _ => None,
    }
}

fn select_profiles_for_skills(config: &SkillsConfig, all_profiles: bool) -> Vec<SkillsProfile> {
    if all_profiles {
        return config.profiles.clone();
    }

    if config.default_pool.is_empty() || config.pools.is_empty() {
        return config.profiles.clone();
    }

    let mut pool_profiles: Vec<String> = Vec::new();
    for pool in &config.pools {
        if pool.name == config.default_pool {
            pool_profiles = pool.profiles.clone();
            break;
        }
    }
    if pool_profiles.is_empty() {
        return config.profiles.clone();
    }

    let lookup: BTreeMap<String, &SkillsProfile> = config
        .profiles
        .iter()
        .map(|p| (p.name.clone(), p))
        .collect();

    let selected: Vec<SkillsProfile> = pool_profiles
        .iter()
        .filter_map(|name| lookup.get(name).map(|p| (*p).clone()))
        .collect();

    if selected.is_empty() {
        return config.profiles.clone();
    }

    selected
}

/// Install builtin skills into the given destination directory.
fn install_builtin_to_dest(
    dest: &str,
    force: bool,
    backend: &dyn SkillsBackend,
) -> Result<(Vec<String>, Vec<String>), String> {
    let mut created = Vec::new();
    let mut skipped = Vec::new();

    for (rel_path, data) in builtin_skill_files() {
        let target = format!("{dest}/{rel_path}");
        let target_path = Path::new(&target);

        if !force && backend.path_exists(target_path) {
            skipped.push(target);
            continue;
        }

        if let Some(parent) = target_path.parent() {
            backend.create_dir_all(parent)?;
        }
        backend.write_file(target_path, data)?;
        created.push(target);
    }

    Ok((created, skipped))
}

/// Install skills from a source directory into the given destination directory.
fn install_source_to_dest(
    source: &str,
    dest: &str,
    force: bool,
    backend: &dyn SkillsBackend,
) -> Result<(Vec<String>, Vec<String>), String> {
    let mut created = Vec::new();
    let mut skipped = Vec::new();

    let entries = backend.walk_dir(Path::new(source))?;
    for (rel_path, is_dir, contents) in &entries {
        let target = format!("{dest}/{rel_path}");
        let target_path = Path::new(&target);

        if *is_dir {
            backend.create_dir_all(target_path)?;
            continue;
        }

        if !force && backend.path_exists(target_path) {
            skipped.push(target);
            continue;
        }

        if let Some(parent) = target_path.parent() {
            backend.create_dir_all(parent)?;
        }

        if let Some(data) = contents {
            backend.write_file(target_path, data)?;
            created.push(target);
        }
    }

    Ok((created, skipped))
}

/// Install builtin skills to harness-specific destinations.
fn install_builtin_to_harnesses(
    base_dir: &str,
    profiles: &[SkillsProfile],
    force: bool,
    backend: &dyn SkillsBackend,
) -> Result<Vec<InstallResult>, String> {
    let home = backend.home_dir()?;
    let mut destinations: BTreeMap<String, InstallResult> = BTreeMap::new();

    for profile in profiles {
        if let Some(dest) =
            resolve_harness_dest(base_dir, &profile.harness, &profile.auth_home, &home)
        {
            let entry = destinations
                .entry(dest.clone())
                .or_insert_with(|| InstallResult {
                    dest,
                    harnesses: Vec::new(),
                    created: Vec::new(),
                    skipped: Vec::new(),
                });
            entry.harnesses.push(profile.harness.clone());
        }
    }

    let mut results = Vec::new();
    for (_, mut item) in destinations {
        let (created, skipped) = install_builtin_to_dest(&item.dest, force, backend)?;
        item.created = created;
        item.skipped = skipped;
        results.push(item);
    }

    Ok(results)
}

/// Install skills from source directory to harness-specific destinations.
fn install_to_harnesses(
    base_dir: &str,
    source_dir: &str,
    profiles: &[SkillsProfile],
    force: bool,
    backend: &dyn SkillsBackend,
) -> Result<Vec<InstallResult>, String> {
    let home = backend.home_dir()?;
    let mut destinations: BTreeMap<String, InstallResult> = BTreeMap::new();

    for profile in profiles {
        if let Some(dest) =
            resolve_harness_dest(base_dir, &profile.harness, &profile.auth_home, &home)
        {
            let entry = destinations
                .entry(dest.clone())
                .or_insert_with(|| InstallResult {
                    dest,
                    harnesses: Vec::new(),
                    created: Vec::new(),
                    skipped: Vec::new(),
                });
            entry.harnesses.push(profile.harness.clone());
        }
    }

    let mut results = Vec::new();
    for (_, mut item) in destinations {
        let (created, skipped) = install_source_to_dest(source_dir, &item.dest, force, backend)?;
        item.created = created;
        item.skipped = skipped;
        results.push(item);
    }

    Ok(results)
}

fn write_help(out: &mut dyn Write) -> Result<(), String> {
    writeln!(out, "Manage workspace skills").map_err(|e| e.to_string())?;
    writeln!(out).map_err(|e| e.to_string())?;
    writeln!(out, "Usage:").map_err(|e| e.to_string())?;
    writeln!(out, "  forge skills <subcommand>").map_err(|e| e.to_string())?;
    writeln!(out).map_err(|e| e.to_string())?;
    writeln!(out, "Subcommands:").map_err(|e| e.to_string())?;
    writeln!(
        out,
        "  bootstrap   Bootstrap repo skills and install to configured harnesses"
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn execute(
    args: &[String],
    backend: &dyn SkillsBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let subcmd = parse_args(args)?;

    match subcmd {
        SubCommand::Help => {
            write_help(stdout)?;
            Ok(())
        }
        SubCommand::Bootstrap(bargs) => execute_bootstrap(&bargs, backend, stdout),
    }
}

fn execute_bootstrap(
    args: &BootstrapArgs,
    backend: &dyn SkillsBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let repo_path = backend.resolve_working_dir()?;
    let repo_str = repo_path.to_string_lossy().to_string();

    let config = backend
        .load_config()?
        .ok_or_else(|| "config not loaded".to_string())?;

    let profiles = select_profiles_for_skills(&config, args.all_profiles);
    if profiles.is_empty() {
        return Err("no profiles configured for skills install".to_string());
    }

    let source_raw = args.path.trim().to_string();
    let (source, installed) = if !source_raw.is_empty() {
        let source = if Path::new(&source_raw).is_absolute() {
            source_raw
        } else {
            format!("{}/{}", repo_str, source_raw)
        };
        let installed = install_to_harnesses(&repo_str, &source, &profiles, args.force, backend)?;
        (source, installed)
    } else {
        let repo_skills = format!("{repo_str}/.agent-skills");
        if backend.is_dir(Path::new(&repo_skills)) {
            let installed =
                install_to_harnesses(&repo_str, &repo_skills, &profiles, args.force, backend)?;
            (repo_skills, installed)
        } else {
            let installed =
                install_builtin_to_harnesses(&repo_str, &profiles, args.force, backend)?;
            ("builtin".to_string(), installed)
        }
    };

    let output = BootstrapOutput { source, installed };

    if args.json || args.jsonl {
        let text = if args.jsonl {
            serde_json::to_string(&output).map_err(|e| format!("failed to marshal output: {e}"))?
        } else {
            serde_json::to_string(&output).map_err(|e| format!("failed to marshal output: {e}"))?
        };
        writeln!(stdout, "{text}").map_err(|e| e.to_string())?;
        return Ok(());
    }

    writeln!(stdout, "Skills source: {}", output.source).map_err(|e| e.to_string())?;
    writeln!(stdout, "Installed:").map_err(|e| e.to_string())?;
    for item in &output.installed {
        writeln!(
            stdout,
            "  - {} (harnesses: {})",
            item.dest,
            item.harnesses.join(", ")
        )
        .map_err(|e| e.to_string())?;
        if !item.created.is_empty() {
            writeln!(stdout, "    created: {} files", item.created.len())
                .map_err(|e| e.to_string())?;
        }
        if !item.skipped.is_empty() {
            writeln!(stdout, "    skipped: {} files", item.skipped.len())
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn run_with_backend(
    args: &[String],
    backend: &dyn SkillsBackend,
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

/// Test helper: run skills with string-slice args and capture output.
pub fn run_for_test(args: &[&str], backend: &InMemorySkillsBackend) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_with_backend(&owned, backend, &mut stdout, &mut stderr);
    CommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_config() -> SkillsConfig {
        SkillsConfig {
            profiles: vec![
                SkillsProfile {
                    name: "claude-main".to_string(),
                    harness: "claude".to_string(),
                    auth_home: String::new(),
                },
                SkillsProfile {
                    name: "codex-main".to_string(),
                    harness: "codex".to_string(),
                    auth_home: String::new(),
                },
            ],
            pools: vec![SkillsPool {
                name: "default".to_string(),
                profiles: vec!["claude-main".to_string()],
            }],
            default_pool: "default".to_string(),
        }
    }

    fn test_backend() -> InMemorySkillsBackend {
        InMemorySkillsBackend::new("/repo").with_config(test_config())
    }

    #[test]
    fn parse_skills_config_yaml_reads_profiles_pools_and_default_pool() {
        let raw = r#"
profiles:
  - name: claude-main
    harness: claude
    auth_home: /tmp/claude
  - name: codex-main
    harness: codex
pools:
  - name: default
    profiles: [claude-main]
default_pool: default
"#;

        let cfg = parse_skills_config_yaml(raw).expect("parse config yaml");
        assert_eq!(cfg.default_pool, "default");
        assert_eq!(cfg.profiles.len(), 2);
        assert_eq!(cfg.profiles[0].name, "claude-main");
        assert_eq!(cfg.profiles[0].auth_home, "/tmp/claude");
        assert_eq!(cfg.pools.len(), 1);
        assert_eq!(cfg.pools[0].name, "default");
        assert_eq!(cfg.pools[0].profiles, vec!["claude-main"]);
    }

    #[test]
    fn parse_skills_config_yaml_filters_invalid_profile_entries() {
        let raw = r#"
profiles:
  - name: ""
    harness: claude
  - name: codex-main
    harness: ""
  - name: valid
    harness: codex
"#;

        let cfg = parse_skills_config_yaml(raw).expect("parse config yaml");
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.profiles[0].name, "valid");
        assert_eq!(cfg.profiles[0].harness, "codex");
    }

    // -- Help ---------------------------------------------------------------

    #[test]
    fn skills_no_subcommand_shows_help() {
        let backend = test_backend();
        let out = run_for_test(&["skills"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Manage workspace skills"));
        assert!(out.stdout.contains("bootstrap"));
    }

    #[test]
    fn skills_help_flag_shows_help() {
        let backend = test_backend();
        let out = run_for_test(&["skills", "--help"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Manage workspace skills"));
    }

    #[test]
    fn skills_unknown_subcommand_errors() {
        let backend = test_backend();
        let out = run_for_test(&["skills", "invalid"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown skills subcommand: invalid"));
    }

    // -- Bootstrap ----------------------------------------------------------

    #[test]
    fn bootstrap_builtin_installs_skills() {
        let backend = test_backend();
        let out = run_for_test(&["skills", "bootstrap"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stdout.contains("Skills source: builtin"));
        assert!(out.stdout.contains("Installed:"));
        assert!(out.stdout.contains("created:"));
        // Should write files to .claude/skills since default pool only has claude-main
        let written = backend.written_files();
        assert!(!written.is_empty());
        let has_claude = written
            .keys()
            .any(|k| k.to_string_lossy().contains(".claude/skills"));
        assert!(
            has_claude,
            "expected .claude/skills files, got: {:?}",
            written.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn bootstrap_builtin_json_output() {
        let backend = test_backend();
        let out = run_for_test(&["skills", "bootstrap", "--json"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["source"], "builtin");
        assert!(parsed["installed"].is_array());
    }

    #[test]
    fn bootstrap_all_profiles_installs_to_all() {
        let backend = test_backend();
        let out = run_for_test(
            &["skills", "bootstrap", "--all-profiles", "--json"],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let installed = parsed["installed"].as_array().unwrap();
        // Should have both claude and codex destinations
        assert!(
            installed.len() >= 2,
            "expected at least 2 destinations, got {}",
            installed.len()
        );
    }

    #[test]
    fn bootstrap_repo_skills_when_present() {
        let backend = test_backend()
            .with_dir("/repo/.agent-skills")
            .with_source_entries(vec![
                ("my-skill".to_string(), true, None),
                (
                    "my-skill/SKILL.md".to_string(),
                    false,
                    Some(b"# My Skill".to_vec()),
                ),
            ]);
        let out = run_for_test(&["skills", "bootstrap", "--json"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["source"], "/repo/.agent-skills");
    }

    #[test]
    fn bootstrap_custom_path() {
        let backend = test_backend().with_source_entries(vec![(
            "custom-skill/SKILL.md".to_string(),
            false,
            Some(b"# Custom".to_vec()),
        )]);
        let out = run_for_test(
            &["skills", "bootstrap", "--path", "/custom/skills", "--json"],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["source"], "/custom/skills");
    }

    #[test]
    fn bootstrap_custom_relative_path() {
        let backend = test_backend().with_source_entries(vec![(
            "rel-skill/SKILL.md".to_string(),
            false,
            Some(b"# Relative".to_vec()),
        )]);
        let out = run_for_test(
            &["skills", "bootstrap", "--path", "my-skills", "--json"],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["source"], "/repo/my-skills");
    }

    #[test]
    fn bootstrap_force_overwrites() {
        let backend = test_backend();
        // First install
        let out1 = run_for_test(&["skills", "bootstrap", "--json"], &backend);
        assert_eq!(out1.exit_code, 0, "stderr: {}", out1.stderr);
        // Second install without force - should skip
        let out2 = run_for_test(&["skills", "bootstrap", "--json"], &backend);
        assert_eq!(out2.exit_code, 0, "stderr: {}", out2.stderr);
        let parsed2: serde_json::Value = serde_json::from_str(&out2.stdout).unwrap();
        let installed2 = &parsed2["installed"].as_array().unwrap()[0];
        assert!(
            !installed2["skipped"].as_array().unwrap().is_empty(),
            "expected skipped files on second install"
        );

        // Third install with force - should overwrite
        let out3 = run_for_test(&["skills", "bootstrap", "--force", "--json"], &backend);
        assert_eq!(out3.exit_code, 0, "stderr: {}", out3.stderr);
        let parsed3: serde_json::Value = serde_json::from_str(&out3.stdout).unwrap();
        let installed3 = &parsed3["installed"].as_array().unwrap()[0];
        assert!(
            !installed3["created"].as_array().unwrap().is_empty(),
            "expected created files with force"
        );
        assert_eq!(
            installed3["skipped"].as_array().unwrap().len(),
            0,
            "expected no skipped files with force"
        );
    }

    #[test]
    fn bootstrap_no_config_errors() {
        let backend = InMemorySkillsBackend::new("/repo");
        let out = run_for_test(&["skills", "bootstrap"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("config not loaded"));
    }

    #[test]
    fn bootstrap_no_profiles_errors() {
        let backend = InMemorySkillsBackend::new("/repo").with_config(SkillsConfig {
            profiles: vec![],
            pools: vec![],
            default_pool: String::new(),
        });
        let out = run_for_test(&["skills", "bootstrap"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("no profiles configured"));
    }

    #[test]
    fn bootstrap_path_requires_value() {
        let backend = test_backend();
        let out = run_for_test(&["skills", "bootstrap", "--path"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--path requires a value"));
    }

    // -- resolve_harness_dest ------------------------------------------------

    #[test]
    fn resolve_dest_prefers_auth_home() {
        let home = PathBuf::from("/home/test");
        let result = resolve_harness_dest("/repo", "codex", "/custom", &home);
        assert_eq!(result, Some("/custom/skills".to_string()));
    }

    #[test]
    fn resolve_dest_uses_base_dir() {
        let home = PathBuf::from("/home/test");
        assert_eq!(
            resolve_harness_dest("/repo", "codex", "", &home),
            Some("/repo/.codex/skills".to_string())
        );
        assert_eq!(
            resolve_harness_dest("/repo", "claude", "", &home),
            Some("/repo/.claude/skills".to_string())
        );
        assert_eq!(
            resolve_harness_dest("/repo", "claude_code", "", &home),
            Some("/repo/.claude/skills".to_string())
        );
        assert_eq!(
            resolve_harness_dest("/repo", "opencode", "", &home),
            Some("/repo/.opencode/skills".to_string())
        );
        assert_eq!(
            resolve_harness_dest("/repo", "pi", "", &home),
            Some("/repo/.pi/skills".to_string())
        );
    }

    #[test]
    fn resolve_dest_uses_home_for_empty_base() {
        let home = PathBuf::from("/home/test");
        assert_eq!(
            resolve_harness_dest("", "codex", "", &home),
            Some("/home/test/.codex/skills".to_string())
        );
        assert_eq!(
            resolve_harness_dest("", "opencode", "", &home),
            Some("/home/test/.config/opencode/skills".to_string())
        );
    }

    #[test]
    fn resolve_dest_unknown_harness_returns_none() {
        let home = PathBuf::from("/home/test");
        assert_eq!(resolve_harness_dest("/repo", "unknown", "", &home), None);
    }

    // -- select_profiles_for_skills -----------------------------------------

    #[test]
    fn select_profiles_all_returns_all() {
        let config = test_config();
        let selected = select_profiles_for_skills(&config, true);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn select_profiles_default_pool_filters() {
        let config = test_config();
        let selected = select_profiles_for_skills(&config, false);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "claude-main");
    }

    #[test]
    fn select_profiles_no_default_pool_returns_all() {
        let config = SkillsConfig {
            profiles: test_config().profiles,
            pools: vec![],
            default_pool: String::new(),
        };
        let selected = select_profiles_for_skills(&config, false);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn select_profiles_no_matching_pool_returns_all() {
        let config = SkillsConfig {
            profiles: test_config().profiles,
            pools: vec![SkillsPool {
                name: "other".to_string(),
                profiles: vec!["nonexistent".to_string()],
            }],
            default_pool: "default".to_string(),
        };
        let selected = select_profiles_for_skills(&config, false);
        assert_eq!(selected.len(), 2);
    }

    // -- human output formatting -------------------------------------------

    #[test]
    fn bootstrap_human_output_format() {
        let backend = test_backend();
        let out = run_for_test(&["skills", "bootstrap"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stdout.contains("Skills source:"));
        assert!(out.stdout.contains("Installed:"));
        assert!(out.stdout.contains("(harnesses:"));
    }

    // -- auth_home routing --------------------------------------------------

    #[test]
    fn bootstrap_auth_home_routes_to_custom_dest() {
        let config = SkillsConfig {
            profiles: vec![SkillsProfile {
                name: "custom-profile".to_string(),
                harness: "claude".to_string(),
                auth_home: "/custom/auth".to_string(),
            }],
            pools: vec![],
            default_pool: String::new(),
        };
        let backend = InMemorySkillsBackend::new("/repo").with_config(config);
        let out = run_for_test(&["skills", "bootstrap", "--json"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let installed = parsed["installed"].as_array().unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0]["dest"], "/custom/auth/skills");
    }
}
