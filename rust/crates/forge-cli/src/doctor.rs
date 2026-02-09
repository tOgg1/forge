use std::io::Write;

use serde::Serialize;
use tabwriter::TabWriter;

/// Status of a single diagnostic check, matching Go's `DoctorCheckStatus`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

impl CheckStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::Skip => "skip",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            Self::Pass => "\u{2713}", // ✓
            Self::Warn => "!",
            Self::Fail => "\u{2717}", // ✗
            Self::Skip => "-",
        }
    }
}

/// A single diagnostic check result, matching Go's `DoctorCheck`.
#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub category: String,
    pub name: String,
    pub status: CheckStatus,
    pub details: Option<String>,
    pub error: Option<String>,
}

/// Summary of all diagnostic results, matching Go's `DoctorSummary`.
#[derive(Debug, Clone)]
pub struct DoctorSummary {
    pub total: usize,
    pub passed: usize,
    pub warnings: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Full diagnostic report, matching Go's `DoctorReport`.
#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
    pub summary: DoctorSummary,
    pub checked_at: String,
}

/// Backend trait abstracting environment checks for testability.
pub trait DoctorBackend {
    /// Run all diagnostic checks and return the results.
    fn run_checks(&self) -> Vec<DoctorCheck>;
    /// Return the current UTC timestamp as an ISO-8601 string.
    fn now_utc(&self) -> String;
}

/// In-memory backend for testing.
#[derive(Debug, Default)]
pub struct InMemoryDoctorBackend {
    pub checks: Vec<DoctorCheck>,
    pub timestamp: String,
}

impl InMemoryDoctorBackend {
    pub fn with_checks(mut self, checks: Vec<DoctorCheck>) -> Self {
        self.checks = checks;
        self
    }

    pub fn with_timestamp(mut self, ts: &str) -> Self {
        self.timestamp = ts.to_string();
        self
    }
}

impl DoctorBackend for InMemoryDoctorBackend {
    fn run_checks(&self) -> Vec<DoctorCheck> {
        self.checks.clone()
    }

    fn now_utc(&self) -> String {
        if self.timestamp.is_empty() {
            "2026-01-01T00:00:00Z".to_string()
        } else {
            self.timestamp.clone()
        }
    }
}

/// Test-only command output.
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn run_for_test(args: &[&str], backend: &dyn DoctorBackend) -> CommandOutput {
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
    backend: &dyn DoctorBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout) {
        Ok(has_failures) => {
            if has_failures {
                1
            } else {
                0
            }
        }
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

fn execute(
    args: &[String],
    backend: &dyn DoctorBackend,
    stdout: &mut dyn Write,
) -> Result<bool, String> {
    let parsed = parse_args(args)?;

    let checks = backend.run_checks();
    let summary = build_summary(&checks);
    let has_failures = summary.failed > 0;

    let report = DoctorReport {
        checks,
        summary,
        checked_at: backend.now_utc(),
    };

    if parsed.json || parsed.jsonl {
        let json_report = build_json_report(&report);
        if parsed.jsonl {
            serde_json::to_writer(&mut *stdout, &json_report).map_err(|e| e.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &json_report).map_err(|e| e.to_string())?;
        }
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(has_failures);
    }

    write_human(&report, stdout)?;
    Ok(has_failures)
}

fn build_summary(checks: &[DoctorCheck]) -> DoctorSummary {
    let mut summary = DoctorSummary {
        total: checks.len(),
        passed: 0,
        warnings: 0,
        failed: 0,
        skipped: 0,
    };
    for c in checks {
        match c.status {
            CheckStatus::Pass => summary.passed += 1,
            CheckStatus::Warn => summary.warnings += 1,
            CheckStatus::Fail => summary.failed += 1,
            CheckStatus::Skip => summary.skipped += 1,
        }
    }
    summary
}

fn write_human(report: &DoctorReport, stdout: &mut dyn Write) -> Result<(), String> {
    writeln!(stdout, "Forge Doctor").map_err(|e| e.to_string())?;
    writeln!(stdout, "============").map_err(|e| e.to_string())?;
    writeln!(stdout).map_err(|e| e.to_string())?;

    // Group by category, in fixed order matching Go
    let categories = ["dependencies", "config", "database", "nodes"];
    let mut tw = TabWriter::new(&mut *stdout).padding(2);

    for cat in &categories {
        let cat_checks: Vec<&DoctorCheck> = report
            .checks
            .iter()
            .filter(|c| c.category == *cat)
            .collect();
        if cat_checks.is_empty() {
            continue;
        }

        writeln!(tw, "\n{}:", cat.to_uppercase()).map_err(|e| e.to_string())?;
        for c in &cat_checks {
            let detail = if let Some(ref err) = c.error {
                err.as_str()
            } else if let Some(ref d) = c.details {
                d.as_str()
            } else {
                ""
            };
            writeln!(tw, "  [{}] {}\t{}", c.status.icon(), c.name, detail)
                .map_err(|e| e.to_string())?;
        }
    }
    tw.flush().map_err(|e| e.to_string())?;

    writeln!(stdout).map_err(|e| e.to_string())?;
    writeln!(
        stdout,
        "Summary: {} passed, {} warnings, {} failed",
        report.summary.passed, report.summary.warnings, report.summary.failed
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

// --- JSON serialization types matching Go struct tags ---

#[derive(Debug, Serialize)]
struct DoctorReportJson<'a> {
    checks: Vec<DoctorCheckJson<'a>>,
    summary: DoctorSummaryJson,
    checked_at: &'a str,
}

#[derive(Debug, Serialize)]
struct DoctorCheckJson<'a> {
    category: &'a str,
    name: &'a str,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct DoctorSummaryJson {
    total: usize,
    passed: usize,
    warnings: usize,
    failed: usize,
    skipped: usize,
}

fn build_json_report(report: &DoctorReport) -> DoctorReportJson<'_> {
    DoctorReportJson {
        checks: report
            .checks
            .iter()
            .map(|c| DoctorCheckJson {
                category: &c.category,
                name: &c.name,
                status: c.status.as_str(),
                details: c.details.as_deref(),
                error: c.error.as_deref(),
            })
            .collect(),
        summary: DoctorSummaryJson {
            total: report.summary.total,
            passed: report.summary.passed,
            warnings: report.summary.warnings,
            failed: report.summary.failed,
            skipped: report.summary.skipped,
        },
        checked_at: &report.checked_at,
    }
}

// --- Argument parsing ---

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "doctor") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => {
                return Err(HELP_TEXT.to_string());
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for doctor: '{flag}'"));
            }
            other => {
                return Err(format!(
                    "error: doctor takes no positional arguments, got '{other}'"
                ));
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    Ok(ParsedArgs { json, jsonl })
}

const HELP_TEXT: &str = "\
Run comprehensive diagnostics on your Forge environment.

Checks include:
- Dependencies: tmux, opencode, ssh, git
- Configuration: config file, database, migrations
- Nodes: connectivity and health
- Accounts: vault access and profiles

Usage:
  forge doctor [flags]

Examples:
  forge doctor
  forge doctor --json

Flags:
  -h, --help   help for doctor";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn default_backend() -> InMemoryDoctorBackend {
        InMemoryDoctorBackend::default()
    }

    fn sample_checks() -> Vec<DoctorCheck> {
        vec![
            DoctorCheck {
                category: "dependencies".to_string(),
                name: "tmux".to_string(),
                status: CheckStatus::Pass,
                details: Some("3.4".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "dependencies".to_string(),
                name: "opencode".to_string(),
                status: CheckStatus::Pass,
                details: Some("installed".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "dependencies".to_string(),
                name: "git".to_string(),
                status: CheckStatus::Pass,
                details: Some("2.44.0".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "dependencies".to_string(),
                name: "ssh".to_string(),
                status: CheckStatus::Pass,
                details: Some("OpenSSH_9.7".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "config".to_string(),
                name: "config_file".to_string(),
                status: CheckStatus::Pass,
                details: Some("/home/user/.config/forge/config.yaml".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "config".to_string(),
                name: "data_directory".to_string(),
                status: CheckStatus::Pass,
                details: Some("/home/user/.local/share/forge".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "database".to_string(),
                name: "connection".to_string(),
                status: CheckStatus::Pass,
                details: Some("/home/user/.local/share/forge/forge.db".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "database".to_string(),
                name: "migrations".to_string(),
                status: CheckStatus::Pass,
                details: Some("12 applied".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "nodes".to_string(),
                name: "count".to_string(),
                status: CheckStatus::Pass,
                details: Some("2 node(s)".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "nodes".to_string(),
                name: "node:local".to_string(),
                status: CheckStatus::Pass,
                details: Some("all checks passed".to_string()),
                error: None,
            },
        ]
    }

    fn failing_checks() -> Vec<DoctorCheck> {
        vec![
            DoctorCheck {
                category: "dependencies".to_string(),
                name: "tmux".to_string(),
                status: CheckStatus::Fail,
                details: None,
                error: Some("not found in PATH".to_string()),
            },
            DoctorCheck {
                category: "dependencies".to_string(),
                name: "git".to_string(),
                status: CheckStatus::Pass,
                details: Some("2.44.0".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "config".to_string(),
                name: "config_file".to_string(),
                status: CheckStatus::Warn,
                details: Some("not found (using defaults)".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "database".to_string(),
                name: "connection".to_string(),
                status: CheckStatus::Fail,
                details: None,
                error: Some("unable to open database".to_string()),
            },
        ]
    }

    fn run(args: &[&str], backend: &dyn DoctorBackend) -> CommandOutput {
        run_for_test(args, backend)
    }

    fn assert_success(out: &CommandOutput) {
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
    }

    // --- parse_args tests ---

    fn to_args(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parse_accepts_no_args() {
        let args = to_args(&["doctor"]);
        let parsed = parse_args(&args).unwrap();
        assert!(!parsed.json);
        assert!(!parsed.jsonl);
    }

    #[test]
    fn parse_accepts_json_flag() {
        let args = to_args(&["doctor", "--json"]);
        let parsed = parse_args(&args).unwrap();
        assert!(parsed.json);
    }

    #[test]
    fn parse_accepts_jsonl_flag() {
        let args = to_args(&["doctor", "--jsonl"]);
        let parsed = parse_args(&args).unwrap();
        assert!(parsed.jsonl);
    }

    #[test]
    fn parse_rejects_json_and_jsonl_together() {
        let args = to_args(&["doctor", "--json", "--jsonl"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn parse_rejects_unknown_flags() {
        let args = to_args(&["doctor", "--bogus"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("unknown argument for doctor"));
    }

    #[test]
    fn parse_rejects_positional_args() {
        let args = to_args(&["doctor", "extra"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("no positional arguments"));
    }

    #[test]
    fn parse_help_returns_usage() {
        let args = to_args(&["doctor", "--help"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("Run comprehensive diagnostics"));
        assert!(err.contains("forge doctor"));
    }

    #[test]
    fn parse_short_help_returns_usage() {
        let args = to_args(&["doctor", "-h"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("Run comprehensive diagnostics"));
    }

    #[test]
    fn parse_help_subcommand_returns_usage() {
        let args = to_args(&["doctor", "help"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("Run comprehensive diagnostics"));
    }

    // --- command output tests ---

    #[test]
    fn doctor_no_checks_returns_success() {
        let backend = default_backend();
        let out = run(&["doctor"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Forge Doctor"));
        assert!(out
            .stdout
            .contains("Summary: 0 passed, 0 warnings, 0 failed"));
    }

    #[test]
    fn doctor_all_pass_returns_success() {
        let backend = default_backend()
            .with_checks(sample_checks())
            .with_timestamp("2026-02-01T10:00:00Z");
        let out = run(&["doctor"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Forge Doctor"));
        assert!(out.stdout.contains("============"));
        assert!(out.stdout.contains("DEPENDENCIES:"));
        assert!(out.stdout.contains("[\u{2713}] tmux"));
        assert!(out.stdout.contains("3.4"));
        assert!(out.stdout.contains("[\u{2713}] git"));
        assert!(out.stdout.contains("CONFIG:"));
        assert!(out.stdout.contains("[\u{2713}] config_file"));
        assert!(out.stdout.contains("DATABASE:"));
        assert!(out.stdout.contains("[\u{2713}] connection"));
        assert!(out.stdout.contains("NODES:"));
        assert!(out.stdout.contains("[\u{2713}] count"));
        assert!(out
            .stdout
            .contains("Summary: 10 passed, 0 warnings, 0 failed"));
    }

    #[test]
    fn doctor_with_failures_returns_exit_1() {
        let backend = default_backend().with_checks(failing_checks());
        let out = run(&["doctor"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("[\u{2717}] tmux"));
        assert!(out.stdout.contains("not found in PATH"));
        assert!(out.stdout.contains("[!] config_file"));
        assert!(out.stdout.contains("not found (using defaults)"));
        assert!(out.stdout.contains("[\u{2717}] connection"));
        assert!(out.stdout.contains("unable to open database"));
        assert!(out
            .stdout
            .contains("Summary: 1 passed, 1 warnings, 2 failed"));
    }

    #[test]
    fn doctor_skip_check_icon() {
        let checks = vec![DoctorCheck {
            category: "nodes".to_string(),
            name: "truncated".to_string(),
            status: CheckStatus::Skip,
            details: Some("too many nodes, remaining skipped".to_string()),
            error: None,
        }];
        let backend = default_backend().with_checks(checks);
        let out = run(&["doctor"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("[-] truncated"));
        assert!(out.stdout.contains("too many nodes, remaining skipped"));
    }

    #[test]
    fn doctor_help_returns_usage_on_stderr() {
        let backend = default_backend();
        let out = run(&["doctor", "--help"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("Run comprehensive diagnostics"));
        assert!(out.stderr.contains("forge doctor"));
    }

    // --- JSON output tests ---

    #[test]
    fn doctor_json_output_all_pass() {
        let backend = default_backend()
            .with_checks(sample_checks())
            .with_timestamp("2026-02-01T10:00:00Z");
        let out = run(&["doctor", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();

        assert_eq!(parsed["checked_at"], "2026-02-01T10:00:00Z");
        assert_eq!(parsed["summary"]["total"], 10);
        assert_eq!(parsed["summary"]["passed"], 10);
        assert_eq!(parsed["summary"]["warnings"], 0);
        assert_eq!(parsed["summary"]["failed"], 0);
        assert_eq!(parsed["summary"]["skipped"], 0);

        let checks = parsed["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 10);

        // First check: tmux
        assert_eq!(checks[0]["category"], "dependencies");
        assert_eq!(checks[0]["name"], "tmux");
        assert_eq!(checks[0]["status"], "pass");
        assert_eq!(checks[0]["details"], "3.4");
        assert!(checks[0].get("error").is_none());
    }

    #[test]
    fn doctor_json_output_with_failures() {
        let backend = default_backend()
            .with_checks(failing_checks())
            .with_timestamp("2026-02-01T10:00:00Z");
        let out = run(&["doctor", "--json"], &backend);
        // JSON output still returns exit 1 on failures
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.is_empty());

        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["summary"]["total"], 4);
        assert_eq!(parsed["summary"]["passed"], 1);
        assert_eq!(parsed["summary"]["warnings"], 1);
        assert_eq!(parsed["summary"]["failed"], 2);

        let checks = parsed["checks"].as_array().unwrap();
        // tmux is fail with error field
        assert_eq!(checks[0]["status"], "fail");
        assert_eq!(checks[0]["error"], "not found in PATH");
        assert!(checks[0].get("details").is_none());
    }

    #[test]
    fn doctor_json_omits_null_details_and_error() {
        let checks = vec![DoctorCheck {
            category: "dependencies".to_string(),
            name: "tmux".to_string(),
            status: CheckStatus::Pass,
            details: Some("3.4".to_string()),
            error: None,
        }];
        let backend = default_backend().with_checks(checks);
        let out = run(&["doctor", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let check = &parsed["checks"][0];
        assert_eq!(check["details"], "3.4");
        // error should be absent (None -> skip_serializing_if)
        assert!(check.get("error").is_none());
    }

    #[test]
    fn doctor_json_empty_checks() {
        let backend = default_backend().with_timestamp("2026-01-01T00:00:00Z");
        let out = run(&["doctor", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["summary"]["total"], 0);
        let checks = parsed["checks"].as_array().unwrap();
        assert!(checks.is_empty());
    }

    // --- JSONL output tests ---

    #[test]
    fn doctor_jsonl_output() {
        let backend = default_backend()
            .with_checks(sample_checks())
            .with_timestamp("2026-02-01T10:00:00Z");
        let out = run(&["doctor", "--jsonl"], &backend);
        assert_success(&out);
        let lines: Vec<&str> = out.stdout.trim().split('\n').collect();
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["checked_at"], "2026-02-01T10:00:00Z");
        assert_eq!(parsed["summary"]["total"], 10);
    }

    #[test]
    fn doctor_jsonl_with_failures_returns_exit_1() {
        let backend = default_backend().with_checks(failing_checks());
        let out = run(&["doctor", "--jsonl"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.is_empty());
        let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
        assert_eq!(parsed["summary"]["failed"], 2);
    }

    // --- build_summary tests ---

    #[test]
    fn build_summary_empty() {
        let s = build_summary(&[]);
        assert_eq!(s.total, 0);
        assert_eq!(s.passed, 0);
        assert_eq!(s.warnings, 0);
        assert_eq!(s.failed, 0);
        assert_eq!(s.skipped, 0);
    }

    #[test]
    fn build_summary_counts_correctly() {
        let checks = vec![
            DoctorCheck {
                category: "a".to_string(),
                name: "1".to_string(),
                status: CheckStatus::Pass,
                details: None,
                error: None,
            },
            DoctorCheck {
                category: "a".to_string(),
                name: "2".to_string(),
                status: CheckStatus::Pass,
                details: None,
                error: None,
            },
            DoctorCheck {
                category: "b".to_string(),
                name: "3".to_string(),
                status: CheckStatus::Warn,
                details: None,
                error: None,
            },
            DoctorCheck {
                category: "c".to_string(),
                name: "4".to_string(),
                status: CheckStatus::Fail,
                details: None,
                error: None,
            },
            DoctorCheck {
                category: "d".to_string(),
                name: "5".to_string(),
                status: CheckStatus::Skip,
                details: None,
                error: None,
            },
        ];
        let s = build_summary(&checks);
        assert_eq!(s.total, 5);
        assert_eq!(s.passed, 2);
        assert_eq!(s.warnings, 1);
        assert_eq!(s.failed, 1);
        assert_eq!(s.skipped, 1);
    }

    // --- human output category ordering ---

    #[test]
    fn human_output_shows_categories_in_fixed_order() {
        // Add checks in reverse order — output should still be dependencies, config, database, nodes
        let checks = vec![
            DoctorCheck {
                category: "nodes".to_string(),
                name: "count".to_string(),
                status: CheckStatus::Pass,
                details: Some("1 node(s)".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "database".to_string(),
                name: "connection".to_string(),
                status: CheckStatus::Pass,
                details: Some("ok".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "config".to_string(),
                name: "config_file".to_string(),
                status: CheckStatus::Warn,
                details: Some("not found".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "dependencies".to_string(),
                name: "tmux".to_string(),
                status: CheckStatus::Pass,
                details: Some("3.4".to_string()),
                error: None,
            },
        ];
        let backend = default_backend().with_checks(checks);
        let out = run(&["doctor"], &backend);
        assert_eq!(out.exit_code, 0);

        let dep_pos = out.stdout.find("DEPENDENCIES:").unwrap();
        let cfg_pos = out.stdout.find("CONFIG:").unwrap();
        let db_pos = out.stdout.find("DATABASE:").unwrap();
        let node_pos = out.stdout.find("NODES:").unwrap();
        assert!(dep_pos < cfg_pos);
        assert!(cfg_pos < db_pos);
        assert!(db_pos < node_pos);
    }

    // --- error display preference ---

    #[test]
    fn human_output_prefers_error_over_details() {
        let checks = vec![DoctorCheck {
            category: "database".to_string(),
            name: "connection".to_string(),
            status: CheckStatus::Fail,
            details: Some("some detail".to_string()),
            error: Some("connection refused".to_string()),
        }];
        let backend = default_backend().with_checks(checks);
        let out = run(&["doctor"], &backend);
        assert!(out.stdout.contains("connection refused"));
        // When error is present, it takes priority over details in display
    }

    // --- JSON golden structure ---

    #[test]
    fn doctor_json_golden_structure() {
        let checks = vec![
            DoctorCheck {
                category: "dependencies".to_string(),
                name: "tmux".to_string(),
                status: CheckStatus::Pass,
                details: Some("3.4".to_string()),
                error: None,
            },
            DoctorCheck {
                category: "database".to_string(),
                name: "connection".to_string(),
                status: CheckStatus::Fail,
                details: None,
                error: Some("unable to open".to_string()),
            },
        ];
        let backend = default_backend()
            .with_checks(checks)
            .with_timestamp("2026-02-09T12:00:00Z");
        let out = run(&["doctor", "--json"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.is_empty());
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();

        // Top-level keys
        assert!(parsed.get("checks").is_some());
        assert!(parsed.get("summary").is_some());
        assert!(parsed.get("checked_at").is_some());

        // Summary structure
        assert!(parsed["summary"].get("total").is_some());
        assert!(parsed["summary"].get("passed").is_some());
        assert!(parsed["summary"].get("warnings").is_some());
        assert!(parsed["summary"].get("failed").is_some());
        assert!(parsed["summary"].get("skipped").is_some());

        // Check structure
        let check = &parsed["checks"][0];
        assert!(check.get("category").is_some());
        assert!(check.get("name").is_some());
        assert!(check.get("status").is_some());
    }
}
