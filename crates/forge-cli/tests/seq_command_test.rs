#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::field_reassign_with_default
)]

use forge_cli::seq::{run_for_test, InMemorySeqBackend, Sequence, SequenceStep, SequenceVar};

#[test]
fn seq_help_matches_golden() {
    let mut backend = InMemorySeqBackend::default();
    let out = run(&["seq", "--help"], &mut backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/seq/help.txt"));
}

#[test]
fn seq_ls_empty_matches_golden() {
    let mut backend = InMemorySeqBackend {
        user_dir: Some("/home/user/.config/forge/sequences".into()),
        project_dir: Some("/project/.forge/sequences".into()),
        ..Default::default()
    };

    let out = run(&["seq", "ls"], &mut backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/seq/empty_list.txt"));
}

#[test]
fn seq_ls_table_matches_golden() {
    let mut backend = seeded();
    let out = run(&["seq", "ls"], &mut backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/seq/list.txt"));
}

#[test]
fn seq_ls_filters_by_tags() {
    let mut backend = seeded();
    let out = run(&["seq", "ls", "--tags", "bug"], &mut backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert!(out.stdout.contains("bugfix"));
    assert!(!out.stdout.contains("deploy"));
}

#[test]
fn seq_show_matches_golden() {
    let mut backend = seeded();
    let out = run(&["seq", "show", "bugfix"], &mut backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/seq/show_bugfix.txt"));
}

#[test]
fn seq_run_requires_required_var() {
    let mut backend = seeded();
    let out = run(
        &["seq", "run", "bugfix", "--agent", "agent_ab"],
        &mut backend,
    );
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "missing required variable \"issue_id\"\n");
}

#[test]
fn seq_run_matches_golden() {
    let mut backend = seeded();
    let out = run(
        &[
            "seq",
            "run",
            "bugfix",
            "--agent",
            "agent_ab",
            "--var",
            "issue_id=ISSUE-123",
        ],
        &mut backend,
    );
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/seq/run_bugfix.txt"));
}

fn run(args: &[&str], backend: &mut InMemorySeqBackend) -> forge_cli::seq::CommandOutput {
    run_for_test(args, backend)
}

fn seeded() -> InMemorySeqBackend {
    InMemorySeqBackend {
        sequences: vec![
            Sequence {
                name: "bugfix".to_string(),
                description: "Standard bug fix workflow".to_string(),
                steps: vec![
                    SequenceStep {
                        step_type: "message".to_string(),
                        content: "Find and fix the bug described in issue {{.issue_id}}."
                            .to_string(),
                        ..Default::default()
                    },
                    SequenceStep {
                        step_type: "pause".to_string(),
                        duration: "30s".to_string(),
                        reason: "Wait for initial analysis".to_string(),
                        ..Default::default()
                    },
                    SequenceStep {
                        step_type: "conditional".to_string(),
                        when: "idle".to_string(),
                        message: "Run the tests and report results.".to_string(),
                        ..Default::default()
                    },
                    SequenceStep {
                        step_type: "pause".to_string(),
                        duration: "30s".to_string(),
                        reason: "Wait for tests to finish".to_string(),
                        ..Default::default()
                    },
                    SequenceStep {
                        step_type: "message".to_string(),
                        content: "Commit the fix with a clear summary.".to_string(),
                        ..Default::default()
                    },
                ],
                variables: vec![SequenceVar {
                    name: "issue_id".to_string(),
                    description: "Issue identifier or URL".to_string(),
                    default_value: String::new(),
                    required: true,
                }],
                tags: vec!["bug".to_string()],
                source: "builtin".to_string(),
            },
            Sequence {
                name: "deploy".to_string(),
                description: "Deploy to staging".to_string(),
                steps: vec![SequenceStep {
                    step_type: "message".to_string(),
                    content: "Deploy now.".to_string(),
                    ..Default::default()
                }],
                variables: vec![],
                tags: vec!["ops".to_string()],
                source: "/home/user/.config/forge/sequences/deploy.yaml".to_string(),
            },
            Sequence {
                name: "review-loop".to_string(),
                description: "Review, address feedback, and re-review".to_string(),
                steps: vec![
                    SequenceStep {
                        step_type: "message".to_string(),
                        content: "Review the latest changes.".to_string(),
                        ..Default::default()
                    },
                    SequenceStep {
                        step_type: "pause".to_string(),
                        duration: "20s".to_string(),
                        ..Default::default()
                    },
                ],
                variables: vec![],
                tags: vec!["review".to_string()],
                source: "/project/.forge/sequences/review-loop.yaml".to_string(),
            },
            Sequence {
                name: "misc".to_string(),
                description: String::new(),
                steps: vec![SequenceStep {
                    step_type: "message".to_string(),
                    content: "Hello".to_string(),
                    ..Default::default()
                }],
                variables: vec![],
                tags: vec![],
                source: "/tmp/misc.yaml".to_string(),
            },
        ],
        user_dir: Some("/home/user/.config/forge/sequences".into()),
        project_dir: Some("/project/.forge/sequences".into()),
        agent_id: Some("agent_abcdef1234567890".to_string()),
        ..Default::default()
    }
}
