#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::process::Command;

const RFMAIL_EXE: &str = env!("CARGO_BIN_EXE_rfmail");

#[test]
fn rfmail_robot_help_is_valid_json() {
    let output = Command::new(RFMAIL_EXE)
        .arg("--robot-help")
        .output()
        .expect("run rfmail");

    assert!(
        output.status.success(),
        "rfmail --robot-help failed: status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("robot-help json");
    assert_eq!(parsed["name"], "fmail");
}

#[test]
fn rfmail_no_args_shows_usage() {
    let output = Command::new(RFMAIL_EXE).output().expect("run rfmail");

    assert!(
        output.status.success(),
        "rfmail (no args) failed: status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage:"),
        "missing Usage: in stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("fmail"),
        "missing fmail token in stdout:\n{stdout}"
    );
}
