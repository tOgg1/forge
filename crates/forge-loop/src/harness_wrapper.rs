use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessKind {
    Pi,
    Claude,
    Codex,
    OpenCode,
    Droid,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptMode {
    Env,
    Stdin,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSpec {
    pub harness: HarnessKind,
    pub prompt_mode: Option<PromptMode>,
    pub command_template: String,
    pub extra_args: Vec<String>,
    pub auth_home: String,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub command: String,
    pub env: Vec<String>,
    pub stdin: Option<String>,
}

pub fn build_execution_plan(
    profile: &ProfileSpec,
    prompt_path: &str,
    prompt_content: &str,
    base_env: &[String],
) -> Result<ExecutionPlan, String> {
    let mut command = profile.command_template.trim().to_string();
    if command.is_empty() {
        return Err("command template is required".to_string());
    }
    if !profile.extra_args.is_empty() {
        command = format!("{command} {}", profile.extra_args.join(" "));
    }

    let prompt_mode = profile.prompt_mode.clone().unwrap_or(PromptMode::Env);
    match prompt_mode {
        PromptMode::Path => {
            if prompt_path.is_empty() {
                return Err("prompt path is required for path mode".to_string());
            }
            command = command.replace("{prompt}", prompt_path);
        }
        PromptMode::Env | PromptMode::Stdin => {}
    }

    let mut env = build_env(profile, &prompt_mode, prompt_content, base_env);
    for (key, value) in &profile.env {
        env.push(format!("{key}={value}"));
    }

    let stdin = match prompt_mode {
        PromptMode::Stdin => Some(prompt_content.to_string()),
        PromptMode::Env | PromptMode::Path => None,
    };

    Ok(ExecutionPlan {
        command,
        env,
        stdin,
    })
}

fn build_env(
    profile: &ProfileSpec,
    mode: &PromptMode,
    prompt_content: &str,
    base_env: &[String],
) -> Vec<String> {
    let mut env = base_env.to_vec();

    if !profile.auth_home.is_empty() {
        if !matches!(
            profile.harness,
            HarnessKind::Claude | HarnessKind::Codex | HarnessKind::OpenCode
        ) {
            env.push(format!("HOME={}", profile.auth_home));
        }
        match profile.harness {
            HarnessKind::Codex => env.push(format!("CODEX_HOME={}", profile.auth_home)),
            HarnessKind::OpenCode => {
                env.push(format!("OPENCODE_CONFIG_DIR={}", profile.auth_home));
                env.push(format!("XDG_DATA_HOME={}", profile.auth_home));
            }
            HarnessKind::Pi => env.push(format!("PI_CODING_AGENT_DIR={}", profile.auth_home)),
            HarnessKind::Claude => env.push(format!("CLAUDE_CONFIG_DIR={}", profile.auth_home)),
            HarnessKind::Droid | HarnessKind::Other(_) => {}
        }
    }

    if matches!(mode, PromptMode::Env) {
        env.push(format!("FORGE_PROMPT_CONTENT={prompt_content}"));
    }

    env
}

#[cfg(test)]
mod tests {
    use super::{build_execution_plan, HarnessKind, ProfileSpec, PromptMode};
    use std::collections::BTreeMap;

    #[test]
    fn env_mode_sets_prompt_content_env() {
        let profile = ProfileSpec {
            harness: HarnessKind::Claude,
            prompt_mode: Some(PromptMode::Env),
            command_template: "claude -p \"$FORGE_PROMPT_CONTENT\"".to_string(),
            extra_args: Vec::new(),
            auth_home: String::new(),
            env: BTreeMap::new(),
        };
        let plan = match build_execution_plan(&profile, "", "hello", &[]) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan
            .env
            .iter()
            .any(|entry| entry == "FORGE_PROMPT_CONTENT=hello"));
        assert!(plan.stdin.is_none());
    }

    #[test]
    fn path_mode_requires_prompt_path_and_replaces_placeholder() {
        let profile = ProfileSpec {
            harness: HarnessKind::Pi,
            prompt_mode: Some(PromptMode::Path),
            command_template: "pi -p \"{prompt}\"".to_string(),
            extra_args: Vec::new(),
            auth_home: "/tmp/pi".to_string(),
            env: BTreeMap::new(),
        };
        let plan = match build_execution_plan(&profile, "/repo/PROMPT.md", "", &[]) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan.command.contains("/repo/PROMPT.md"));
        assert!(plan
            .env
            .iter()
            .any(|entry| entry == "PI_CODING_AGENT_DIR=/tmp/pi"));
    }

    #[test]
    fn path_mode_without_path_is_error() {
        let profile = ProfileSpec {
            harness: HarnessKind::Pi,
            prompt_mode: Some(PromptMode::Path),
            command_template: "pi -p \"{prompt}\"".to_string(),
            extra_args: Vec::new(),
            auth_home: String::new(),
            env: BTreeMap::new(),
        };
        let err = match build_execution_plan(&profile, "", "", &[]) {
            Ok(_) => panic!("expected error"),
            Err(err) => err,
        };
        assert_eq!(err, "prompt path is required for path mode");
    }

    #[test]
    fn stdin_mode_sets_stdin_and_not_prompt_env() {
        let profile = ProfileSpec {
            harness: HarnessKind::Codex,
            prompt_mode: Some(PromptMode::Stdin),
            command_template: "codex exec -".to_string(),
            extra_args: Vec::new(),
            auth_home: String::new(),
            env: BTreeMap::new(),
        };
        let plan = match build_execution_plan(&profile, "", "prompt", &[]) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert_eq!(plan.stdin.as_deref(), Some("prompt"));
        assert!(!plan
            .env
            .iter()
            .any(|entry| entry.starts_with("FORGE_PROMPT_CONTENT=")));
    }

    #[test]
    fn claude_auth_home_sets_claude_config_dir_but_not_home() {
        let profile = ProfileSpec {
            harness: HarnessKind::Claude,
            prompt_mode: Some(PromptMode::Env),
            command_template: "claude -p \"$FORGE_PROMPT_CONTENT\"".to_string(),
            extra_args: Vec::new(),
            auth_home: "/tmp/claude-1".to_string(),
            env: BTreeMap::new(),
        };
        let plan = match build_execution_plan(&profile, "", "hello", &[]) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan
            .env
            .iter()
            .any(|entry| entry == "CLAUDE_CONFIG_DIR=/tmp/claude-1"));
        assert!(!plan.env.iter().any(|entry| entry == "HOME=/tmp/claude-1"));
    }

    #[test]
    fn extra_args_are_appended_to_command_template() {
        let profile = ProfileSpec {
            harness: HarnessKind::Claude,
            prompt_mode: Some(PromptMode::Env),
            command_template: "claude -p \"$FORGE_PROMPT_CONTENT\"".to_string(),
            extra_args: vec![
                "--dangerously-skip-permissions".to_string(),
                "--verbose".to_string(),
            ],
            auth_home: String::new(),
            env: BTreeMap::new(),
        };
        let plan = match build_execution_plan(&profile, "", "hello", &[]) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(plan.command.contains("--dangerously-skip-permissions"));
        assert!(plan.command.contains("--verbose"));
    }

    #[test]
    fn profile_env_is_appended_last() {
        let mut profile_env = BTreeMap::new();
        profile_env.insert("FORGE_PROMPT_CONTENT".to_string(), "override".to_string());
        let profile = ProfileSpec {
            harness: HarnessKind::Pi,
            prompt_mode: Some(PromptMode::Env),
            command_template: "pi -p \"$FORGE_PROMPT_CONTENT\"".to_string(),
            extra_args: Vec::new(),
            auth_home: String::new(),
            env: profile_env,
        };
        let plan = match build_execution_plan(&profile, "", "hello", &[]) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert_eq!(
            plan.env.last().map(String::as_str),
            Some("FORGE_PROMPT_CONTENT=override")
        );
    }
}
