use forge_loop::prompt_composition::{
    resolve_base_prompt, resolve_override_prompt, LoopPromptConfig, PromptOverridePayload,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn prompt_resolution_scenario_covers_base_default_and_override() {
    let temp = TempDir::new("forge-loop-prompt-scenario");
    let repo = temp.path();

    let forge_prompts = repo.join(".forge").join("prompts");
    if let Err(err) = fs::create_dir_all(&forge_prompts) {
        panic!("mkdir prompts: {err}");
    }
    let fallback = forge_prompts.join("default.md");
    if let Err(err) = fs::write(&fallback, "fallback default") {
        panic!("write fallback: {err}");
    }

    let prompt_md = repo.join("PROMPT.md");
    if let Err(err) = fs::write(&prompt_md, "repo prompt") {
        panic!("write prompt.md: {err}");
    }

    let repo_str = repo.display().to_string();

    let base = LoopPromptConfig {
        repo_path: repo_str.clone(),
        base_prompt_msg: String::new(),
        base_prompt_path: String::new(),
    };
    let base_prompt = match resolve_base_prompt(&base) {
        Ok(value) => value,
        Err(err) => panic!("base prompt resolve failed: {err}"),
    };
    assert_eq!(base_prompt.content, "repo prompt");
    assert_eq!(Path::new(&base_prompt.path), prompt_md.as_path());

    let custom = repo.join("custom.md");
    if let Err(err) = fs::write(&custom, "custom prompt") {
        panic!("write custom prompt: {err}");
    }
    let from_custom = LoopPromptConfig {
        repo_path: repo_str.clone(),
        base_prompt_msg: String::new(),
        base_prompt_path: "custom.md".to_string(),
    };
    let custom_prompt = match resolve_base_prompt(&from_custom) {
        Ok(value) => value,
        Err(err) => panic!("custom prompt resolve failed: {err}"),
    };
    assert_eq!(custom_prompt.content, "custom prompt");
    assert_eq!(Path::new(&custom_prompt.path), custom.as_path());

    let inline = LoopPromptConfig {
        repo_path: repo_str.clone(),
        base_prompt_msg: "inline base".to_string(),
        base_prompt_path: String::new(),
    };
    let inline_prompt = match resolve_base_prompt(&inline) {
        Ok(value) => value,
        Err(err) => panic!("inline prompt resolve failed: {err}"),
    };
    assert_eq!(inline_prompt.content, "inline base");
    assert!(!inline_prompt.from_file);

    let override_file = repo.join("override.md");
    if let Err(err) = fs::write(&override_file, "override file prompt") {
        panic!("write override file: {err}");
    }
    let override_from_path = PromptOverridePayload {
        prompt: "override.md".to_string(),
        is_path: true,
    };
    let override_path_prompt = match resolve_override_prompt(&repo_str, &override_from_path) {
        Ok(value) => value,
        Err(err) => panic!("override path resolve failed: {err}"),
    };
    assert_eq!(override_path_prompt.content, "override file prompt");
    assert_eq!(
        Path::new(&override_path_prompt.path),
        override_file.as_path()
    );

    let override_inline = PromptOverridePayload {
        prompt: "override inline prompt".to_string(),
        is_path: false,
    };
    let override_inline_prompt = match resolve_override_prompt(&repo_str, &override_inline) {
        Ok(value) => value,
        Err(err) => panic!("override inline resolve failed: {err}"),
    };
    assert_eq!(override_inline_prompt.content, "override inline prompt");
    assert!(override_inline_prompt.path.is_empty());
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            monotonic_nanos()
        ));
        if let Err(err) = fs::create_dir_all(&path) {
            panic!("failed creating temp dir {}: {err}", path.display());
        }
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn monotonic_nanos() -> u128 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(_) => 0,
    }
}
