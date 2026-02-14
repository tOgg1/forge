use std::path::Path;

/// Resolve a prompt CLI argument as:
/// 1) registered prompt name under `.forge/prompts/<name>.md`
/// 2) original value passthrough (explicit path fallback)
#[must_use]
pub fn resolve_prompt_name_or_path(repo_path: &Path, prompt: &str) -> String {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Preserve explicit path-style values.
    if trimmed.contains('/') || trimmed.contains('\\') {
        return trimmed.to_owned();
    }

    let prompt_name = trimmed.strip_suffix(".md").unwrap_or(trimmed).trim();
    if prompt_name.is_empty() {
        return trimmed.to_owned();
    }

    let candidate = repo_path
        .join(".forge")
        .join("prompts")
        .join(format!("{prompt_name}.md"));
    if candidate.is_file() {
        format!(".forge/prompts/{prompt_name}.md")
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_prompt_name_or_path;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo_path(tag: &str) -> std::path::PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-cli-prompt-resolution-{tag}-{nanos}-{}-{suffix}",
            std::process::id(),
        ))
    }

    #[test]
    fn resolves_registered_prompt_name_to_repo_prompts_path() {
        let repo = temp_repo_path("name");
        let prompts_dir = repo.join(".forge").join("prompts");
        std::fs::create_dir_all(&prompts_dir)
            .unwrap_or_else(|err| panic!("create prompts dir: {err}"));
        std::fs::write(prompts_dir.join("po-design.md"), "# prompt")
            .unwrap_or_else(|err| panic!("write prompt file: {err}"));

        let resolved = resolve_prompt_name_or_path(Path::new(&repo), "po-design");
        assert_eq!(resolved, ".forge/prompts/po-design.md");

        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn keeps_explicit_path_passthrough() {
        let repo = temp_repo_path("path");
        std::fs::create_dir_all(&repo).unwrap_or_else(|err| panic!("create repo dir: {err}"));
        let resolved = resolve_prompt_name_or_path(Path::new(&repo), "./PROMPT.md");
        assert_eq!(resolved, "./PROMPT.md");
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn keeps_unknown_prompt_name_passthrough() {
        let repo = temp_repo_path("unknown");
        std::fs::create_dir_all(&repo).unwrap_or_else(|err| panic!("create repo dir: {err}"));
        let resolved = resolve_prompt_name_or_path(Path::new(&repo), "does-not-exist");
        assert_eq!(resolved, "does-not-exist");
        let _ = std::fs::remove_dir_all(repo);
    }
}
