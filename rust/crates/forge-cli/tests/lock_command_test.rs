#![allow(clippy::unwrap_used)]

use forge_cli::lock::{
    run_for_test, CommandOutput, FileReservation, FileReservationGrant, InMemoryLockBackend,
    LockBackend, LockClaimResponse, LockReleaseResponse,
};

fn test_backend() -> InMemoryLockBackend {
    InMemoryLockBackend::with_project_and_agent("test-project", "test-agent")
}

fn run(args: &[&str], backend: &dyn LockBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(out: &CommandOutput) {
    assert_eq!(out.exit_code, 0, "expected exit 0, stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
}

fn assert_failure(out: &CommandOutput) {
    assert_ne!(
        out.exit_code, 0,
        "expected non-zero exit, stdout: {}",
        out.stdout
    );
}

// ---------------------------------------------------------------------------
// Help
// ---------------------------------------------------------------------------

#[test]
fn lock_help_matches_golden() {
    let backend = test_backend();
    let out = run(&["lock"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/lock/help.txt"));
}

#[test]
fn lock_help_flag_matches_golden() {
    let backend = test_backend();
    let out = run(&["lock", "--help"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/lock/help.txt"));
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[test]
fn lock_status_empty_matches_golden() {
    let backend = test_backend();
    let out = run(&["lock", "status"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/lock/status_empty.txt"));
}

#[test]
fn lock_status_json_empty() {
    let backend = test_backend();
    let out = run(&["lock", "--json", "status"], &backend);
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert!(parsed.is_array());
    assert!(parsed.as_array().unwrap().is_empty());
}

#[test]
fn lock_status_with_locks_renders_table() {
    let mut backend = test_backend();
    backend.reservations = vec![FileReservation {
        id: 1,
        agent: "bot-1".to_string(),
        path_pattern: "src/*.rs".to_string(),
        exclusive: true,
        reason: "editing".to_string(),
        created_ts: "2026-01-01T00:00:00Z".to_string(),
        expires_ts: "2099-01-01T00:00:00Z".to_string(),
        released_ts: String::new(),
    }];

    let out = run(&["lock", "status"], &backend);
    assert_success(&out);
    assert!(out.stdout.contains("LOCK-ID"));
    assert!(out.stdout.contains("AGENT"));
    assert!(out.stdout.contains("bot-1"));
    assert!(out.stdout.contains("src/*.rs"));
    assert!(out.stdout.contains("yes"));
}

#[test]
fn lock_status_filtered_by_agent() {
    let mut backend = test_backend();
    backend.reservations = vec![
        FileReservation {
            id: 1,
            agent: "bot-1".to_string(),
            path_pattern: "src/*.rs".to_string(),
            exclusive: true,
            reason: String::new(),
            created_ts: "2026-01-01T00:00:00Z".to_string(),
            expires_ts: "2099-01-01T00:00:00Z".to_string(),
            released_ts: String::new(),
        },
        FileReservation {
            id: 2,
            agent: "bot-2".to_string(),
            path_pattern: "docs/*.md".to_string(),
            exclusive: false,
            reason: String::new(),
            created_ts: "2026-01-01T00:00:00Z".to_string(),
            expires_ts: "2099-01-01T00:00:00Z".to_string(),
            released_ts: String::new(),
        },
    ];

    let out = run(&["lock", "status", "--agent", "bot-1"], &backend);
    assert_success(&out);
    assert!(out.stdout.contains("bot-1"));
    assert!(!out.stdout.contains("bot-2"));
}

#[test]
fn lock_status_jsonl_output() {
    let mut backend = test_backend();
    backend.reservations = vec![
        FileReservation {
            id: 1,
            agent: "bot-1".to_string(),
            path_pattern: "src/*.rs".to_string(),
            exclusive: true,
            reason: String::new(),
            created_ts: "2026-01-01T00:00:00Z".to_string(),
            expires_ts: "2099-01-01T00:00:00Z".to_string(),
            released_ts: String::new(),
        },
        FileReservation {
            id: 2,
            agent: "bot-2".to_string(),
            path_pattern: "docs/*.md".to_string(),
            exclusive: false,
            reason: String::new(),
            created_ts: "2026-01-01T00:00:00Z".to_string(),
            expires_ts: "2099-01-01T00:00:00Z".to_string(),
            released_ts: String::new(),
        },
    ];

    let out = run(&["lock", "--jsonl", "status"], &backend);
    assert_success(&out);
    let lines: Vec<&str> = out.stdout.trim().lines().collect();
    assert_eq!(lines.len(), 2);
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["agent"], "bot-1");
    let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(second["agent"], "bot-2");
}

// ---------------------------------------------------------------------------
// Check
// ---------------------------------------------------------------------------

#[test]
fn lock_check_clear_matches_golden() {
    let backend = test_backend();
    let out = run(&["lock", "check", "--path", "src/main.rs"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/lock/check_clear.txt"));
}

#[test]
fn lock_check_locked_path() {
    let mut backend = test_backend();
    backend.reservations = vec![FileReservation {
        id: 1,
        agent: "bot-1".to_string(),
        path_pattern: "src/*.rs".to_string(),
        exclusive: true,
        reason: "editing".to_string(),
        created_ts: "2026-01-01T00:00:00Z".to_string(),
        expires_ts: "2099-01-01T00:00:00Z".to_string(),
        released_ts: String::new(),
    }];

    let out = run(&["lock", "check", "--path", "src/main.rs"], &backend);
    assert_success(&out);
    assert!(out.stdout.contains("Path is locked: src/main.rs"));
    assert!(out.stdout.contains("Holder: bot-1"));
    assert!(out.stdout.contains("Pattern: src/*.rs"));
}

#[test]
fn lock_check_json_output() {
    let mut backend = test_backend();
    backend.reservations = vec![FileReservation {
        id: 1,
        agent: "bot-1".to_string(),
        path_pattern: "src/*.rs".to_string(),
        exclusive: true,
        reason: String::new(),
        created_ts: "2026-01-01T00:00:00Z".to_string(),
        expires_ts: "2099-01-01T00:00:00Z".to_string(),
        released_ts: String::new(),
    }];

    let out = run(
        &["lock", "--json", "check", "--path", "src/main.rs"],
        &backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed[0]["path"], "src/main.rs");
    assert!(!parsed[0]["claims"].as_array().unwrap().is_empty());
}

#[test]
fn lock_check_requires_path() {
    let backend = test_backend();
    let out = run(&["lock", "check"], &backend);
    assert_failure(&out);
    assert!(out.stderr.contains("--path is required"));
}

// ---------------------------------------------------------------------------
// Claim
// ---------------------------------------------------------------------------

#[test]
fn lock_claim_success_text() {
    let mut backend = test_backend();
    backend.set_claim_response(LockClaimResponse {
        granted: vec![FileReservationGrant {
            id: 42,
            path_pattern: "src/*.rs".to_string(),
            exclusive: true,
            reason: "editing".to_string(),
            expires_ts: "2099-01-01T00:00:00Z".to_string(),
        }],
        conflicts: Vec::new(),
    });

    let out = run(
        &[
            "lock", "claim", "--agent", "bot", "--path", "src/*.rs", "--reason", "editing",
        ],
        &backend,
    );
    assert_success(&out);
    assert!(out.stdout.contains("Lock claimed:"));
    assert!(out.stdout.contains("Agent:   bot"));
    assert!(out.stdout.contains("src/*.rs"));
    assert!(out.stdout.contains("id 42"));
    assert!(out.stdout.contains("1h0m0s"));
}

#[test]
fn lock_claim_json_output() {
    let mut backend = test_backend();
    backend.set_claim_response(LockClaimResponse {
        granted: vec![FileReservationGrant {
            id: 42,
            path_pattern: "src/*.rs".to_string(),
            exclusive: true,
            reason: "editing".to_string(),
            expires_ts: "2099-01-01T00:00:00Z".to_string(),
        }],
        conflicts: Vec::new(),
    });

    let out = run(
        &[
            "lock", "--json", "claim", "--agent", "bot", "--path", "src/*.rs",
        ],
        &backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["granted"][0]["id"], 42);
    assert_eq!(parsed["granted"][0]["path_pattern"], "src/*.rs");
}

#[test]
fn lock_claim_requires_path() {
    let backend = test_backend();
    let out = run(&["lock", "claim", "--agent", "bot"], &backend);
    assert_failure(&out);
    assert!(out.stderr.contains("at least one --path is required"));
}

#[test]
fn lock_claim_rejects_short_ttl() {
    let backend = test_backend();
    let out = run(
        &[
            "lock",
            "claim",
            "--agent",
            "bot",
            "--path",
            "src/main.rs",
            "--ttl",
            "30s",
        ],
        &backend,
    );
    assert_failure(&out);
    assert!(out.stderr.contains("ttl must be at least 1m"));
}

// ---------------------------------------------------------------------------
// Release
// ---------------------------------------------------------------------------

#[test]
fn lock_release_text() {
    let mut backend = test_backend();
    backend.set_release_response(LockReleaseResponse {
        released: 2,
        released_at: "2026-01-01T00:00:00Z".to_string(),
    });

    let out = run(
        &["lock", "release", "--agent", "bot", "--path", "src/*.rs"],
        &backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, "Released 2 lock(s)\n");
}

#[test]
fn lock_release_json() {
    let mut backend = test_backend();
    backend.set_release_response(LockReleaseResponse {
        released: 1,
        released_at: "2026-01-01T00:00:00Z".to_string(),
    });

    let out = run(
        &[
            "lock",
            "--json",
            "release",
            "--agent",
            "bot",
            "--lock-id",
            "42",
        ],
        &backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["released"], 1);
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn lock_unknown_subcommand() {
    let backend = test_backend();
    let out = run(&["lock", "foobar"], &backend);
    assert_failure(&out);
    assert!(out.stderr.contains("unknown lock subcommand: foobar"));
}

#[test]
fn lock_claim_unknown_flag() {
    let backend = test_backend();
    let out = run(&["lock", "claim", "--bogus"], &backend);
    assert_failure(&out);
    assert!(out.stderr.contains("unknown flag for lock claim"));
}

#[test]
fn lock_json_jsonl_mutually_exclusive() {
    let backend = test_backend();
    let out = run(&["lock", "--json", "--jsonl", "status"], &backend);
    assert_failure(&out);
    assert!(out.stderr.contains("mutually exclusive"));
}
