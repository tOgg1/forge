use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnResult {
    pub owner: String,
    pub instance_id: String,
    pub pid: Option<i64>,
}

pub fn start_loop_runner(loop_id: &str, requested_owner: &str) -> Result<SpawnResult, String> {
    match requested_owner {
        "local" => spawn_local(loop_id, "local"),
        "daemon" => Ok(SpawnResult {
            owner: "daemon".to_string(),
            instance_id: format!("resume-{}", uuid::Uuid::new_v4().simple()),
            pid: None,
        }),
        "auto" => spawn_local(loop_id, "auto"),
        other => Err(format!(
            "invalid --spawn-owner \"{other}\" (valid: local|daemon|auto)"
        )),
    }
}

fn spawn_local(loop_id: &str, owner_label: &str) -> Result<SpawnResult, String> {
    if skip_spawn_for_test_harness() {
        return Ok(SpawnResult {
            owner: owner_label.to_string(),
            instance_id: format!("resume-{}", uuid::Uuid::new_v4().simple()),
            pid: None,
        });
    }

    let exe =
        std::env::current_exe().map_err(|err| format!("resolve current executable: {err}"))?;
    let mut cmd = Command::new(exe);
    cmd.arg("loop").arg("run").arg(loop_id);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());

    let child = cmd
        .spawn()
        .map_err(|err| format!("failed to start local loop process: {err}"))?;
    let pid = child.id();
    drop(child);

    Ok(SpawnResult {
        owner: owner_label.to_string(),
        instance_id: format!("resume-{}", uuid::Uuid::new_v4().simple()),
        pid: Some(i64::from(pid)),
    })
}

fn skip_spawn_for_test_harness() -> bool {
    if let Ok(exe) = std::env::current_exe() {
        let path = exe.to_string_lossy();
        if path.contains("/target/debug/deps/") || path.contains("\\target\\debug\\deps\\") {
            return true;
        }
    }
    std::env::var_os("RUST_TEST_THREADS").is_some()
        || std::env::var_os("FORGE_TEST_MODE").is_some()
        || std::env::var("CI")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}
