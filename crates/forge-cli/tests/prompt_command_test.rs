use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_cli::prompt::{run_for_test, CommandOutput, PromptBackend, PromptBackendError};
use uuid::Uuid;

#[derive(Debug)]
struct TestPromptBackend {
    repo_path: PathBuf,
    edit_calls: RefCell<Vec<PathBuf>>,
    edit_error: Option<String>,
}

impl TestPromptBackend {
    fn new(repo_path: PathBuf) -> Self {
        Self {
            repo_path,
            edit_calls: RefCell::new(Vec::new()),
            edit_error: None,
        }
    }

    fn with_edit_error(repo_path: PathBuf, message: &str) -> Self {
        Self {
            repo_path,
            edit_calls: RefCell::new(Vec::new()),
            edit_error: Some(message.to_string()),
        }
    }

    fn edit_count(&self) -> usize {
        self.edit_calls.borrow().len()
    }
}

impl PromptBackend for TestPromptBackend {
    fn resolve_repo_path(&self) -> Result<PathBuf, String> {
        Ok(self.repo_path.clone())
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
            let entry = match entry {
                Ok(value) => value,
                Err(err) => return Err(PromptBackendError::Message(err.to_string())),
            };
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(err) => return Err(PromptBackendError::Message(err.to_string())),
            };
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
        if let Some(message) = &self.edit_error {
            return Err(message.clone());
        }

        let prompt_path = repo_path
            .join(".forge")
            .join("prompts")
            .join(format!("{prompt_name}.md"));
        self.edit_calls.borrow_mut().push(prompt_path);
        Ok(())
    }
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new() -> Self {
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(value) => value.as_nanos(),
            Err(err) => panic!("clock should be after unix epoch: {err}"),
        };
        let suffix = Uuid::new_v4();
        let path = std::env::temp_dir().join(format!(
            "forge-cli-prompt-test-{}-{nanos}-{suffix}",
            std::process::id()
        ));
        if let Err(err) = fs::create_dir_all(&path) {
            panic!("failed to create temp repo: {err}");
        }
        Self { path }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn prompt_ls_empty_human_matches_golden() {
    let repo = TempRepo::new();
    let mut backend = TestPromptBackend::new(repo.path.clone());

    let output = run(&["prompt", "ls"], &mut backend);
    assert_success(&output);
    assert_eq!(
        output.stdout,
        include_str!("golden/prompt/ls_empty_human.txt")
    );
}

#[test]
fn prompt_structured_outputs_match_goldens() {
    let repo = TempRepo::new();
    let source = repo.path.join("prompt-source.md");
    if let Err(err) = fs::write(&source, "hello") {
        panic!("failed to write prompt source: {err}");
    }

    let mut backend = TestPromptBackend::new(repo.path.clone());

    let add = run(
        &[
            "prompt",
            "add",
            "oracle-prompt",
            source.to_string_lossy().as_ref(),
            "--json",
        ],
        &mut backend,
    );
    assert_success(&add);
    let add_expected = include_str!("golden/prompt/add_json.txt")
        .replace("__REPO__", repo.path.to_string_lossy().as_ref());
    assert_eq!(add.stdout, add_expected);

    let list = run(&["prompt", "ls", "--json"], &mut backend);
    assert_success(&list);
    assert_eq!(list.stdout, include_str!("golden/prompt/ls_json.txt"));

    let set_default = run(
        &["prompt", "set-default", "oracle-prompt", "--json"],
        &mut backend,
    );
    assert_success(&set_default);
    assert_eq!(
        set_default.stdout,
        include_str!("golden/prompt/set_default_json.txt")
    );
}

#[test]
fn prompt_edit_human_and_quiet_paths() {
    let repo = TempRepo::new();
    let prompts_dir = repo.path.join(".forge").join("prompts");
    if let Err(err) = fs::create_dir_all(&prompts_dir) {
        panic!("failed to create prompts dir: {err}");
    }
    if let Err(err) = fs::write(prompts_dir.join("oracle-prompt.md"), "hello") {
        panic!("failed to seed prompt: {err}");
    }

    let mut backend = TestPromptBackend::new(repo.path.clone());
    let output = run(&["prompt", "edit", "oracle-prompt"], &mut backend);
    assert_success(&output);
    assert_eq!(output.stdout, include_str!("golden/prompt/edit_human.txt"));
    assert_eq!(backend.edit_count(), 1);

    let quiet = run(
        &["prompt", "edit", "oracle-prompt", "--quiet"],
        &mut backend,
    );
    assert_success(&quiet);
    assert!(quiet.stdout.is_empty());
    assert_eq!(backend.edit_count(), 2);
}

#[test]
fn prompt_integration_scenario_runs_end_to_end() {
    let repo = TempRepo::new();
    let source = repo.path.join("seed.md");
    if let Err(err) = fs::write(&source, "seed content") {
        panic!("failed to write seed prompt: {err}");
    }

    let mut backend = TestPromptBackend::new(repo.path.clone());

    let add = run(
        &["prompt", "add", "alpha", source.to_string_lossy().as_ref()],
        &mut backend,
    );
    assert_success(&add);
    assert_eq!(add.stdout, "Prompt \"alpha\" added\n");

    let set_default = run(&["prompt", "set-default", "alpha"], &mut backend);
    assert_success(&set_default);
    assert_eq!(set_default.stdout, "Default prompt set to \"alpha\"\n");

    let edit = run(&["prompt", "edit", "alpha"], &mut backend);
    assert_success(&edit);

    let list_jsonl = run(&["prompt", "ls", "--jsonl"], &mut backend);
    assert_success(&list_jsonl);
    assert_eq!(list_jsonl.stdout, "\"alpha\"\n\"default\"\n");

    let default_content = match fs::read_to_string(repo.path.join(".forge/prompts/default.md")) {
        Ok(value) => value,
        Err(err) => panic!("failed to read default prompt: {err}"),
    };
    assert_eq!(default_content, "seed content");
}

#[test]
fn prompt_missing_prompt_returns_error() {
    let repo = TempRepo::new();
    let mut backend = TestPromptBackend::new(repo.path.clone());

    let output = run(&["prompt", "edit", "missing"], &mut backend);
    assert_eq!(output.exit_code, 1);
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, "prompt not found: missing\n");
}

#[test]
fn prompt_editor_error_is_propagated() {
    let repo = TempRepo::new();
    let prompts_dir = repo.path.join(".forge").join("prompts");
    if let Err(err) = fs::create_dir_all(&prompts_dir) {
        panic!("failed to create prompts dir: {err}");
    }
    if let Err(err) = fs::write(prompts_dir.join("oracle-prompt.md"), "hello") {
        panic!("failed to seed prompt: {err}");
    }

    let mut backend = TestPromptBackend::with_edit_error(repo.path.clone(), "boom");
    let output = run(&["prompt", "edit", "oracle-prompt"], &mut backend);
    assert_eq!(output.exit_code, 1);
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, "boom\n");
}

#[test]
fn prompt_invalid_arity_is_reported() {
    let repo = TempRepo::new();
    let mut backend = TestPromptBackend::new(repo.path.clone());

    let output = run(&["prompt", "add", "only-name"], &mut backend);
    assert_eq!(output.exit_code, 1);
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, "error: prompt add requires <name> <path>\n");
}

fn run(args: &[&str], backend: &mut dyn PromptBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(output: &CommandOutput) {
    assert_eq!(output.exit_code, 0);
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        output.stderr
    );
}
