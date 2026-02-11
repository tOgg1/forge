use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::Command;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopLedgerRecord {
    pub id: String,
    pub name: String,
    pub repo_path: String,
    pub ledger_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRunRecord {
    pub id: String,
    pub status: String,
    pub prompt_source: String,
    pub prompt_path: String,
    pub prompt_override: bool,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRecord {
    pub name: String,
    pub harness: String,
    pub auth_kind: String,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct LedgerConfig {
    #[serde(default)]
    pub git_status: bool,
    #[serde(default)]
    pub git_diff_stat: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RepoConfig {
    #[serde(default)]
    ledger: LedgerConfig,
}

pub fn ensure_ledger_file(loop_record: &LoopLedgerRecord) -> Result<(), String> {
    ensure_ledger_file_with_now(loop_record, Utc::now())
}

fn ensure_ledger_file_with_now(
    loop_record: &LoopLedgerRecord,
    now: DateTime<Utc>,
) -> Result<(), String> {
    if loop_record.ledger_path.is_empty() {
        return Ok(());
    }
    let ledger_path = Path::new(&loop_record.ledger_path);
    if ledger_path.exists() {
        return Ok(());
    }
    if let Some(parent) = ledger_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let mut content = String::new();
    content.push_str("---\n");
    content.push_str(&format!("loop_id: {}\n", loop_record.id));
    content.push_str(&format!("loop_name: {}\n", loop_record.name));
    content.push_str(&format!("repo_path: {}\n", loop_record.repo_path));
    content.push_str(&format!(
        "created_at: {}\n",
        now.to_rfc3339_opts(SecondsFormat::Secs, true)
    ));
    content.push_str("---\n\n");
    content.push_str(&format!("# Loop Ledger: {}\n\n", loop_record.name));

    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }
    let mut file = options.open(ledger_path).map_err(|err| err.to_string())?;
    file.write_all(content.as_bytes())
        .map_err(|err| err.to_string())
}

pub fn append_ledger_entry(
    loop_record: &LoopLedgerRecord,
    run_record: &LoopRunRecord,
    profile: &ProfileRecord,
    output_tail: &str,
    tail_lines: usize,
) -> Result<(), String> {
    append_ledger_entry_with_now(
        loop_record,
        run_record,
        profile,
        output_tail,
        tail_lines,
        Utc::now(),
    )
}

fn append_ledger_entry_with_now(
    loop_record: &LoopLedgerRecord,
    run_record: &LoopRunRecord,
    profile: &ProfileRecord,
    output_tail: &str,
    tail_lines: usize,
    now: DateTime<Utc>,
) -> Result<(), String> {
    if loop_record.ledger_path.is_empty() {
        return Ok(());
    }

    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }
    let mut file = options
        .open(&loop_record.ledger_path)
        .map_err(|err| err.to_string())?;

    let mut entry = String::new();
    entry.push_str(&format!(
        "## {}\n\n",
        now.to_rfc3339_opts(SecondsFormat::Secs, true)
    ));
    entry.push_str(&format!("- run_id: {}\n", run_record.id));
    entry.push_str(&format!("- loop_name: {}\n", loop_record.name));
    entry.push_str(&format!("- status: {}\n", run_record.status));
    entry.push_str(&format!("- profile: {}\n", profile.name));
    if !profile.harness.is_empty() {
        entry.push_str(&format!("- harness: {}\n", profile.harness));
    }
    if !profile.auth_kind.is_empty() {
        entry.push_str(&format!("- auth_kind: {}\n", profile.auth_kind));
    }
    entry.push_str(&format!("- prompt_source: {}\n", run_record.prompt_source));
    if !run_record.prompt_path.is_empty() {
        entry.push_str(&format!("- prompt_path: {}\n", run_record.prompt_path));
    }
    entry.push_str(&format!(
        "- prompt_override: {}\n",
        run_record.prompt_override
    ));
    entry.push_str(&format!(
        "- started_at: {}\n",
        run_record
            .started_at
            .to_rfc3339_opts(SecondsFormat::Secs, true)
    ));
    if let Some(finished_at) = run_record.finished_at {
        entry.push_str(&format!(
            "- finished_at: {}\n",
            finished_at.to_rfc3339_opts(SecondsFormat::Secs, true)
        ));
    }
    if let Some(exit_code) = run_record.exit_code {
        entry.push_str(&format!("- exit_code: {}\n", exit_code));
    }
    entry.push('\n');

    let trimmed_tail = limit_output_lines(output_tail, tail_lines);
    if !trimmed_tail.trim().is_empty() {
        entry.push_str("```\n");
        entry.push_str(trimmed_tail.trim());
        entry.push_str("\n```\n");
    }

    let config = load_ledger_config(&loop_record.repo_path);
    let git_summary = build_git_summary(&loop_record.repo_path, &config);
    if !git_summary.trim().is_empty() {
        entry.push_str("\n### Git Summary\n\n```\n");
        entry.push_str(git_summary.trim());
        entry.push_str("\n```\n");
    }
    entry.push('\n');

    file.write_all(entry.as_bytes())
        .map_err(|err| err.to_string())
}

pub fn limit_output_lines(text: &str, max_lines: usize) -> String {
    if max_lines == 0 {
        return text.to_string();
    }
    let lines = text.split('\n').collect::<Vec<&str>>();
    if lines.len() <= max_lines {
        return text.to_string();
    }
    lines[lines.len() - max_lines..].join("\n")
}

pub fn load_ledger_config(repo_path: &str) -> LedgerConfig {
    let path = Path::new(repo_path).join(".forge").join("forge.yaml");
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return LedgerConfig::default(),
    };
    let parsed: RepoConfig = match serde_yaml::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(_) => return LedgerConfig::default(),
    };
    parsed.ledger
}

pub fn build_git_summary(repo_path: &str, config: &LedgerConfig) -> String {
    if !config.git_status && !config.git_diff_stat {
        return String::new();
    }
    if !is_git_repo(repo_path) {
        return String::new();
    }

    let mut lines: Vec<String> = Vec::new();
    if config.git_status {
        if let Ok(status) = run_git(repo_path, &["status", "--porcelain"]) {
            lines.push("status --porcelain:".to_string());
            if status.trim().is_empty() {
                lines.push("  (clean)".to_string());
            } else {
                lines.push(status.trim().to_string());
            }
        }
    }
    if config.git_diff_stat {
        if let Ok(diff) = run_git(repo_path, &["diff", "--stat"]) {
            lines.push("diff --stat:".to_string());
            if diff.trim().is_empty() {
                lines.push("  (clean)".to_string());
            } else {
                lines.push(diff.trim().to_string());
            }
        }
    }
    lines.join("\n")
}

fn is_git_repo(repo_path: &str) -> bool {
    match run_git(repo_path, &["rev-parse", "--is-inside-work-tree"]) {
        Ok(output) => output.trim() == "true",
        Err(_) => false,
    }
}

fn run_git(repo_path: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        append_ledger_entry_with_now, build_git_summary, ensure_ledger_file_with_now,
        limit_output_lines, LedgerConfig, LoopLedgerRecord, LoopRunRecord, ProfileRecord,
    };
    use chrono::{TimeZone, Utc};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn ensure_ledger_file_writes_header_once() {
        let temp = TempDir::new("forge-loop-ledger");
        let ledger = temp.path().join(".forge").join("ledgers").join("alpha.md");
        let loop_record = LoopLedgerRecord {
            id: "loop-1".to_string(),
            name: "alpha".to_string(),
            repo_path: temp.path().display().to_string(),
            ledger_path: ledger.display().to_string(),
        };
        let now = Utc.with_ymd_and_hms(2026, 2, 9, 17, 0, 0).unwrap();
        if let Err(err) = ensure_ledger_file_with_now(&loop_record, now) {
            panic!("ensure ledger failed: {err}");
        }
        let first = match fs::read_to_string(&ledger) {
            Ok(text) => text,
            Err(err) => panic!("read ledger failed: {err}"),
        };
        assert!(first.contains("loop_id: loop-1"));
        assert!(first.contains("# Loop Ledger: alpha"));

        if let Err(err) = ensure_ledger_file_with_now(&loop_record, now) {
            panic!("ensure ledger second failed: {err}");
        }
        let second = match fs::read_to_string(&ledger) {
            Ok(text) => text,
            Err(err) => panic!("read ledger second failed: {err}"),
        };
        assert_eq!(first, second);
    }

    #[test]
    fn append_ledger_entry_writes_expected_fields_and_tail() {
        let temp = TempDir::new("forge-loop-ledger-append");
        let ledger = temp.path().join(".forge").join("ledgers").join("beta.md");
        let loop_record = LoopLedgerRecord {
            id: "loop-2".to_string(),
            name: "beta".to_string(),
            repo_path: temp.path().display().to_string(),
            ledger_path: ledger.display().to_string(),
        };
        let run_record = LoopRunRecord {
            id: "run-1".to_string(),
            status: "completed".to_string(),
            prompt_source: "base".to_string(),
            prompt_path: "PROMPT.md".to_string(),
            prompt_override: false,
            started_at: Utc.with_ymd_and_hms(2026, 2, 9, 17, 0, 0).unwrap(),
            finished_at: Some(Utc.with_ymd_and_hms(2026, 2, 9, 17, 1, 0).unwrap()),
            exit_code: Some(0),
        };
        let profile = ProfileRecord {
            name: "default".to_string(),
            harness: "codex".to_string(),
            auth_kind: "local".to_string(),
        };

        if let Err(err) = ensure_ledger_file_with_now(
            &loop_record,
            Utc.with_ymd_and_hms(2026, 2, 9, 17, 0, 0).unwrap(),
        ) {
            panic!("ensure ledger failed: {err}");
        }
        if let Err(err) = append_ledger_entry_with_now(
            &loop_record,
            &run_record,
            &profile,
            "line1\nline2\nline3",
            2,
            Utc.with_ymd_and_hms(2026, 2, 9, 17, 2, 0).unwrap(),
        ) {
            panic!("append entry failed: {err}");
        }

        let text = match fs::read_to_string(&ledger) {
            Ok(text) => text,
            Err(err) => panic!("read ledger failed: {err}"),
        };
        assert!(text.contains("- run_id: run-1"));
        assert!(text.contains("- profile: default"));
        assert!(text.contains("- prompt_path: PROMPT.md"));
        assert!(text.contains("```\nline2\nline3\n```"));
    }

    #[test]
    fn limit_output_lines_matches_go_behavior() {
        assert_eq!(limit_output_lines("a\nb\nc", 0), "a\nb\nc");
        assert_eq!(limit_output_lines("a\nb\nc", 2), "b\nc");
        assert_eq!(limit_output_lines("a\nb", 5), "a\nb");
    }

    #[test]
    fn build_git_summary_returns_empty_when_not_git() {
        let temp = TempDir::new("forge-loop-not-git");
        let cfg = LedgerConfig {
            git_status: true,
            git_diff_stat: true,
        };
        assert!(build_git_summary(&temp.path().display().to_string(), &cfg).is_empty());
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
