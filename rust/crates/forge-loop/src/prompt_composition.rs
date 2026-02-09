use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorMessage {
    pub timestamp_rfc3339: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSpec {
    pub path: String,
    pub content: String,
    pub source: String,
    pub is_override: bool,
    pub from_file: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopPromptConfig {
    pub repo_path: String,
    pub base_prompt_msg: String,
    pub base_prompt_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptOverridePayload {
    pub prompt: String,
    pub is_path: bool,
}

pub fn resolve_base_prompt(loop_cfg: &LoopPromptConfig) -> Result<PromptSpec, String> {
    if !loop_cfg.base_prompt_msg.trim().is_empty() {
        return Ok(PromptSpec {
            path: String::new(),
            content: loop_cfg.base_prompt_msg.clone(),
            source: "base".to_string(),
            is_override: false,
            from_file: false,
        });
    }

    if !loop_cfg.base_prompt_path.trim().is_empty() {
        let path = resolve_repo_path(&loop_cfg.repo_path, &loop_cfg.base_prompt_path);
        let content = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;
        return Ok(PromptSpec {
            path: path.display().to_string(),
            content,
            source: "base".to_string(),
            is_override: false,
            from_file: true,
        });
    }

    let prompt_path = Path::new(&loop_cfg.repo_path).join("PROMPT.md");
    if prompt_path.exists() {
        let content = std::fs::read_to_string(&prompt_path).map_err(|err| err.to_string())?;
        return Ok(PromptSpec {
            path: prompt_path.display().to_string(),
            content,
            source: "base".to_string(),
            is_override: false,
            from_file: true,
        });
    }

    let fallback = Path::new(&loop_cfg.repo_path)
        .join(".forge")
        .join("prompts")
        .join("default.md");
    let content = std::fs::read_to_string(&fallback).map_err(|err| err.to_string())?;
    Ok(PromptSpec {
        path: fallback.display().to_string(),
        content,
        source: "base".to_string(),
        is_override: false,
        from_file: true,
    })
}

pub fn resolve_override_prompt(
    repo_path: &str,
    payload: &PromptOverridePayload,
) -> Result<PromptSpec, String> {
    if payload.prompt.trim().is_empty() {
        return Err("override prompt is empty".to_string());
    }

    if payload.is_path {
        let path = resolve_repo_path(repo_path, &payload.prompt);
        let content = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;
        return Ok(PromptSpec {
            path: path.display().to_string(),
            content,
            source: "override".to_string(),
            is_override: true,
            from_file: true,
        });
    }

    Ok(PromptSpec {
        path: String::new(),
        content: payload.prompt.clone(),
        source: "override".to_string(),
        is_override: true,
        from_file: false,
    })
}

pub fn resolve_repo_path(repo_root: &str, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return candidate.to_path_buf();
    }
    Path::new(repo_root).join(path)
}

pub fn inject_loop_memory(base_prompt: &str, loop_memory: &str) -> String {
    if loop_memory.trim().is_empty() {
        return base_prompt.to_string();
    }
    format!("{}{}", base_prompt.trim_end_matches('\n'), loop_memory)
}

pub fn append_operator_messages(base_prompt: &str, messages: &[OperatorMessage]) -> String {
    if messages.is_empty() {
        return base_prompt.to_string();
    }

    let mut out = String::from(base_prompt.trim_end_matches('\n'));
    for entry in messages {
        out.push_str("\n\n## Operator Message (");
        out.push_str(entry.timestamp_rfc3339.as_str());
        out.push_str(")\n\n");
        out.push_str(entry.text.trim());
    }
    out
}

pub fn compose_prompt(
    base_prompt: &str,
    loop_memory: &str,
    messages: &[OperatorMessage],
) -> String {
    let with_memory = inject_loop_memory(base_prompt, loop_memory);
    append_operator_messages(&with_memory, messages)
}

#[cfg(test)]
mod tests {
    use super::{
        append_operator_messages, compose_prompt, inject_loop_memory, resolve_base_prompt,
        resolve_override_prompt, resolve_repo_path, LoopPromptConfig, OperatorMessage,
        PromptOverridePayload,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn inject_loop_memory_skips_blank_memory() {
        assert_eq!(inject_loop_memory("base\n", " \n\t "), "base\n");
    }

    #[test]
    fn inject_loop_memory_trims_base_newlines_before_append() {
        let got = inject_loop_memory("base\n\n", "\n\n## Loop Context (persistent)\n");
        assert_eq!(got, "base\n\n## Loop Context (persistent)\n");
    }

    #[test]
    fn append_operator_messages_keeps_base_when_empty() {
        assert_eq!(append_operator_messages("base", &[]), "base");
    }

    #[test]
    fn append_operator_messages_appends_in_order_and_trims_message_text() {
        let got = append_operator_messages(
            "base\n",
            &[
                OperatorMessage {
                    timestamp_rfc3339: "2026-02-09T17:00:00Z".to_string(),
                    text: "  first  ".to_string(),
                },
                OperatorMessage {
                    timestamp_rfc3339: "2026-02-09T17:01:00Z".to_string(),
                    text: "\nsecond\n".to_string(),
                },
            ],
        );
        assert_eq!(
            got,
            "base\n\n## Operator Message (2026-02-09T17:00:00Z)\n\nfirst\n\n## Operator Message (2026-02-09T17:01:00Z)\n\nsecond"
        );
    }

    #[test]
    fn append_operator_messages_with_empty_base_matches_go_shape() {
        let got = append_operator_messages(
            "",
            &[OperatorMessage {
                timestamp_rfc3339: "2026-02-09T17:00:00Z".to_string(),
                text: "msg".to_string(),
            }],
        );
        assert_eq!(got, "\n\n## Operator Message (2026-02-09T17:00:00Z)\n\nmsg");
    }

    #[test]
    fn compose_prompt_injects_memory_before_operator_messages() {
        let got = compose_prompt(
            "base\n",
            "\n\n## Loop Context (persistent)\n\nCurrent:\n- task-1 [in_progress]\n",
            &[OperatorMessage {
                timestamp_rfc3339: "2026-02-09T17:02:00Z".to_string(),
                text: "blocked on schema diff".to_string(),
            }],
        );
        assert_eq!(
            got,
            "base\n\n## Loop Context (persistent)\n\nCurrent:\n- task-1 [in_progress]\n\n## Operator Message (2026-02-09T17:02:00Z)\n\nblocked on schema diff"
        );
    }

    #[test]
    fn resolve_base_prompt_precedence_matches_go() {
        let temp = TempDir::new("forge-loop-prompt");
        let repo = temp.path();
        let forge_prompts = repo.join(".forge").join("prompts");
        if let Err(err) = fs::create_dir_all(&forge_prompts) {
            panic!("mkdir prompts: {err}");
        }
        let fallback = forge_prompts.join("default.md");
        if let Err(err) = fs::write(&fallback, "default") {
            panic!("write fallback: {err}");
        }

        let prompt_md = repo.join("PROMPT.md");
        if let Err(err) = fs::write(&prompt_md, "prompt") {
            panic!("write prompt.md: {err}");
        }

        let inline = LoopPromptConfig {
            repo_path: repo.display().to_string(),
            base_prompt_msg: "inline".to_string(),
            base_prompt_path: String::new(),
        };
        let inline_res = match resolve_base_prompt(&inline) {
            Ok(value) => value,
            Err(err) => panic!("inline resolve failed: {err}"),
        };
        assert_eq!(inline_res.content, "inline");
        assert!(!inline_res.from_file);

        let custom = repo.join("custom.md");
        if let Err(err) = fs::write(&custom, "custom") {
            panic!("write custom: {err}");
        }
        let base_path = LoopPromptConfig {
            repo_path: repo.display().to_string(),
            base_prompt_msg: String::new(),
            base_prompt_path: "custom.md".to_string(),
        };
        let base_path_res = match resolve_base_prompt(&base_path) {
            Ok(value) => value,
            Err(err) => panic!("base path resolve failed: {err}"),
        };
        assert_eq!(base_path_res.content, "custom");
        assert_eq!(Path::new(&base_path_res.path), custom.as_path());

        let prompt_default = LoopPromptConfig {
            repo_path: repo.display().to_string(),
            base_prompt_msg: String::new(),
            base_prompt_path: String::new(),
        };
        let prompt_default_res = match resolve_base_prompt(&prompt_default) {
            Ok(value) => value,
            Err(err) => panic!("prompt.md resolve failed: {err}"),
        };
        assert_eq!(Path::new(&prompt_default_res.path), prompt_md.as_path());

        if let Err(err) = fs::remove_file(&prompt_md) {
            panic!("remove prompt.md: {err}");
        }
        let fallback_res = match resolve_base_prompt(&prompt_default) {
            Ok(value) => value,
            Err(err) => panic!("fallback resolve failed: {err}"),
        };
        assert_eq!(Path::new(&fallback_res.path), fallback.as_path());
    }

    #[test]
    fn resolve_override_prompt_path_and_inline() {
        let temp = TempDir::new("forge-loop-override");
        let repo = temp.path();
        let override_path = repo.join("override.md");
        if let Err(err) = fs::write(&override_path, "override file") {
            panic!("write override: {err}");
        }

        let from_path = PromptOverridePayload {
            prompt: "override.md".to_string(),
            is_path: true,
        };
        let path_res = match resolve_override_prompt(&repo.display().to_string(), &from_path) {
            Ok(value) => value,
            Err(err) => panic!("path override resolve failed: {err}"),
        };
        assert_eq!(Path::new(&path_res.path), override_path.as_path());
        assert_eq!(path_res.content, "override file");
        assert!(path_res.from_file);
        assert!(path_res.is_override);

        let inline = PromptOverridePayload {
            prompt: "override inline".to_string(),
            is_path: false,
        };
        let inline_res = match resolve_override_prompt(&repo.display().to_string(), &inline) {
            Ok(value) => value,
            Err(err) => panic!("inline override resolve failed: {err}"),
        };
        assert_eq!(inline_res.content, "override inline");
        assert!(inline_res.path.is_empty());
        assert!(!inline_res.from_file);
    }

    #[test]
    fn resolve_override_prompt_rejects_empty() {
        let payload = PromptOverridePayload {
            prompt: "   ".to_string(),
            is_path: false,
        };
        let err = match resolve_override_prompt("/repo", &payload) {
            Ok(value) => panic!("expected error, got {:?}", value),
            Err(err) => err,
        };
        assert_eq!(err, "override prompt is empty");
    }

    #[test]
    fn resolve_repo_path_handles_absolute_and_relative() {
        let rel = resolve_repo_path("/repo/root", "PROMPT.md");
        assert_eq!(rel, PathBuf::from("/repo/root/PROMPT.md"));
        let abs = resolve_repo_path("/repo/root", "/tmp/abs.md");
        assert_eq!(abs, PathBuf::from("/tmp/abs.md"));
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
}
