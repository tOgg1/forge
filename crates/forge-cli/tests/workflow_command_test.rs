use forge_cli::workflow::{parse_workflow_toml, run_for_test, InMemoryWorkflowBackend, Workflow};
use std::fmt::Debug;
use std::path::PathBuf;

#[test]
fn workflow_help_matches_golden() {
    let backend = seeded_backend();
    let out = run(&["workflow", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/workflow/help.txt"));
}

#[test]
fn workflow_show_json_matches_golden() {
    let backend = seeded_backend();
    let out = run(&["workflow", "--json", "show", "basic"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(
        out.stdout,
        include_str!("golden/workflow/show_basic_json.txt")
    );
}

#[test]
fn workflow_validate_invalid_human_matches_goldens() {
    let backend = seeded_backend();
    let out = run(&["workflow", "validate", "bad-dep"], &backend);
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        out.stdout,
        include_str!("golden/workflow/validate_bad_dep_stdout.txt")
    );
    assert_eq!(
        out.stderr,
        include_str!("golden/workflow/validate_bad_dep_stderr.txt")
    );
}

#[test]
fn workflow_validate_invalid_json_matches_golden() {
    let backend = seeded_backend();
    let out = run(&["workflow", "--json", "validate", "bad-dep"], &backend);
    assert_eq!(out.exit_code, 1);
    assert_eq!(out.stderr, "\n");
    assert_eq!(
        out.stdout,
        include_str!("golden/workflow/validate_bad_dep_json.txt")
    );
}

fn run(args: &[&str], backend: &InMemoryWorkflowBackend) -> forge_cli::workflow::CommandOutput {
    run_for_test(args, backend)
}

fn seeded_backend() -> InMemoryWorkflowBackend {
    InMemoryWorkflowBackend {
        workflows: vec![basic_workflow(), invalid_workflow()],
        project_dir: Some(PathBuf::from("/project")),
    }
}

fn ok_or_panic<T, E>(result: Result<T, E>, context: &str) -> T
where
    E: Debug,
{
    match result {
        Ok(value) => value,
        Err(err) => panic!("{context}: {err:?}"),
    }
}

fn basic_workflow() -> Workflow {
    ok_or_panic(
        parse_workflow_toml(
            r#"
name = "basic"
description = "Basic workflow"

[[steps]]
id = "plan"
type = "agent"
prompt = "Plan work"
	"#,
            "/project/.forge/workflows/basic.toml",
        ),
        "basic workflow should parse",
    )
}

fn invalid_workflow() -> Workflow {
    ok_or_panic(
        parse_workflow_toml(
            r#"
name = "bad-dep"

[[steps]]
id = "build"
type = "bash"
cmd = "make test"
depends_on = ["missing"]
	"#,
            ".forge/workflows/bad-dep.toml",
        ),
        "invalid workflow fixture should parse",
    )
}
