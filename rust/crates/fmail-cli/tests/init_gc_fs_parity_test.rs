#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

const FMAIL_EXE: &str = env!("CARGO_BIN_EXE_fmail-cli");

fn run_fmail(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(FMAIL_EXE)
        .args(args)
        .env("FMAIL_ROOT", root)
        .current_dir(root)
        .output()
        .expect("run fmail-cli")
}

fn project_file(root: &Path) -> PathBuf {
    root.join(".fmail").join("project.json")
}

fn read_project_json(path: &Path) -> Value {
    let data = std::fs::read_to_string(path).expect("read project.json");
    serde_json::from_str::<Value>(&data).expect("parse project.json")
}

#[test]
fn init_creates_project_when_missing() {
    let tmp = TempDir::new().expect("tempdir");
    let out = run_fmail(tmp.path(), &["init"]);
    assert!(
        out.status.success(),
        "status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let project = read_project_json(&project_file(tmp.path()));
    let expected_id = fmail_core::project::derive_project_id(tmp.path()).expect("derive project");
    assert_eq!(project["id"], Value::String(expected_id));
    assert!(
        project["created"].as_str().is_some(),
        "missing created timestamp"
    );
}

#[test]
fn init_without_project_flag_does_not_rewrite_existing_project() {
    let tmp = TempDir::new().expect("tempdir");
    let project_path = project_file(tmp.path());
    std::fs::create_dir_all(project_path.parent().expect("project parent")).expect("mkdir .fmail");
    let original = "{\n  \"id\": \"existing-id\",\n  \"created\": \"2026-02-08T10:11:12Z\",\n  \"extra\": \"keep\"\n}\n";
    std::fs::write(&project_path, original).expect("seed project.json");

    let out = run_fmail(tmp.path(), &["init"]);
    assert!(
        out.status.success(),
        "status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let after = std::fs::read_to_string(&project_path).expect("read project after init");
    assert_eq!(after, original);
}

#[test]
fn init_project_override_preserves_created_timestamp() {
    let tmp = TempDir::new().expect("tempdir");
    let project_path = project_file(tmp.path());
    std::fs::create_dir_all(project_path.parent().expect("project parent")).expect("mkdir .fmail");
    std::fs::write(
        &project_path,
        "{\n  \"id\": \"old-id\",\n  \"created\": \"2026-02-08T10:11:12Z\"\n}\n",
    )
    .expect("seed project.json");

    let out = run_fmail(tmp.path(), &["init", "--project", "new-id"]);
    assert!(
        out.status.success(),
        "status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let project = read_project_json(&project_path);
    assert_eq!(project["id"], Value::String("new-id".to_string()));
    assert_eq!(
        project["created"],
        Value::String("2026-02-08T10:11:12Z".to_string())
    );
}

#[test]
fn gc_dry_run_lists_relative_old_files_without_deleting() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().join(".fmail");
    std::fs::create_dir_all(root.join("topics/task")).expect("mkdir task");
    std::fs::create_dir_all(root.join("dm/alice")).expect("mkdir alice");
    std::fs::create_dir_all(root.join("topics/Bad Topic!")).expect("mkdir bad topic");
    std::fs::create_dir_all(root.join("dm/Bad Agent!")).expect("mkdir bad agent");

    let old_topic = root.join("topics/task/20200101-010101-0001.json");
    let new_topic = root.join("topics/task/29990101-010101-0001.json");
    let old_dm = root.join("dm/alice/20200101-020202-0001.json");
    let bad_topic = root.join("topics/Bad Topic!/20200101-030303-0001.json");
    let bad_dm = root.join("dm/Bad Agent!/20200101-040404-0001.json");

    for path in [&old_topic, &new_topic, &old_dm, &bad_topic, &bad_dm] {
        std::fs::write(path, "{}").expect("write fixture");
    }

    let out = run_fmail(tmp.path(), &["gc", "--days", "1", "--dry-run"]);
    assert!(
        out.status.success(),
        "status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout,
        "topics/task/20200101-010101-0001.json\ndm/alice/20200101-020202-0001.json\n"
    );

    assert!(old_topic.exists(), "dry-run must not delete old topic");
    assert!(old_dm.exists(), "dry-run must not delete old dm");
    assert!(new_topic.exists(), "dry-run must not delete new topic");
    assert!(
        bad_topic.exists(),
        "dry-run must not touch invalid topic dir"
    );
    assert!(bad_dm.exists(), "dry-run must not touch invalid dm dir");
}

#[test]
fn gc_delete_removes_only_old_valid_files() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().join(".fmail");
    std::fs::create_dir_all(root.join("topics/task")).expect("mkdir task");
    std::fs::create_dir_all(root.join("dm/alice")).expect("mkdir alice");
    std::fs::create_dir_all(root.join("topics/Bad Topic!")).expect("mkdir bad topic");
    std::fs::create_dir_all(root.join("dm/Bad Agent!")).expect("mkdir bad agent");

    let old_topic = root.join("topics/task/20200101-010101-0001.json");
    let new_topic = root.join("topics/task/29990101-010101-0001.json");
    let old_dm = root.join("dm/alice/20200101-020202-0001.json");
    let bad_topic = root.join("topics/Bad Topic!/20200101-030303-0001.json");
    let bad_dm = root.join("dm/Bad Agent!/20200101-040404-0001.json");

    for path in [&old_topic, &new_topic, &old_dm, &bad_topic, &bad_dm] {
        std::fs::write(path, "{}").expect("write fixture");
    }

    let out = run_fmail(tmp.path(), &["gc", "--days", "1"]);
    assert!(
        out.status.success(),
        "status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stdout.is_empty(),
        "gc delete mode should not print output, got:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );

    assert!(!old_topic.exists(), "old topic should be deleted");
    assert!(!old_dm.exists(), "old dm should be deleted");
    assert!(new_topic.exists(), "new topic should remain");
    assert!(bad_topic.exists(), "invalid topic dir should be ignored");
    assert!(bad_dm.exists(), "invalid dm dir should be ignored");
}
