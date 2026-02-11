use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn workflow_dispatch_uses_filesystem_backend_for_ls_show_validate() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let repo = TempDir::new("workflow-dispatch-fs-backend");
    seed_workflows(&repo.path);

    with_working_dir(&repo.path, || {
        let (code, stdout, stderr) = run(&["--json", "workflow", "ls"]);
        assert_eq!(code, 0, "stderr: {stderr}");
        assert!(stderr.is_empty());
        let list: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
        let items = list
            .as_array()
            .unwrap_or_else(|| panic!("expected list array: {list}"));
        assert_eq!(items.len(), 2);
        let names: Vec<&str> = items
            .iter()
            .map(|item| item["name"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(names, vec!["bad-dep", "basic"]);

        let (code, stdout, stderr) = run(&["workflow", "show", "basic", "--json"]);
        assert_eq!(code, 0, "stderr: {stderr}");
        assert!(stderr.is_empty());
        let show: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
        assert_eq!(show["name"], "basic");
        assert_eq!(show["description"], "Basic workflow");
        let steps = show["steps"]
            .as_array()
            .unwrap_or_else(|| panic!("expected steps array: {show}"));
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0]["id"], "plan");
        assert_eq!(steps[0]["type"], "agent");

        let (code, stdout, stderr) = run(&["workflow", "validate", "bad-dep", "--json"]);
        assert_eq!(code, 1, "stderr: {stderr}");
        assert_eq!(stderr, "\n");
        let validate: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
        assert_eq!(validate["name"], "bad-dep");
        assert_eq!(validate["valid"], false);
        assert!(validate["path"]
            .as_str()
            .is_some_and(|path| path.ends_with(".forge/workflows/bad-dep.toml")));
        let errors = validate["errors"]
            .as_array()
            .unwrap_or_else(|| panic!("expected errors array: {validate}"));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0]["code"], "ERR_MISSING_STEP");
    });
}

fn run(args: &[&str]) -> (i32, String, String) {
    let argv: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = forge_cli::run_with_args(&argv, &mut stdout, &mut stderr);
    (
        code,
        String::from_utf8(stdout).unwrap_or_default(),
        String::from_utf8(stderr).unwrap_or_default(),
    )
}

fn seed_workflows(repo: &Path) {
    let workflows_dir = repo.join(".forge").join("workflows");
    std::fs::create_dir_all(&workflows_dir)
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", workflows_dir.display()));

    let basic = r#"
name = "basic"
description = "Basic workflow"

[[steps]]
id = "plan"
type = "agent"
prompt = "Plan work"
"#;
    let bad_dep = r#"
name = "bad-dep"

[[steps]]
id = "build"
type = "bash"
cmd = "make test"
depends_on = ["missing"]
"#;

    std::fs::write(workflows_dir.join("basic.toml"), basic.trim_start())
        .unwrap_or_else(|e| panic!("write basic workflow fixture: {e}"));
    std::fs::write(workflows_dir.join("bad-dep.toml"), bad_dep.trim_start())
        .unwrap_or_else(|e| panic!("write bad-dep workflow fixture: {e}"));
}

fn with_working_dir<F>(dir: &Path, run: F)
where
    F: FnOnce(),
{
    let original = std::env::current_dir()
        .unwrap_or_else(|e| panic!("failed to capture current working directory: {e}"));
    std::env::set_current_dir(dir)
        .unwrap_or_else(|e| panic!("failed to set current working directory: {e}"));
    run();
    std::env::set_current_dir(&original)
        .unwrap_or_else(|e| panic!("failed to restore current working directory: {e}"));
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let mut path = std::env::temp_dir();
        let unique = format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        path.push(unique);
        std::fs::create_dir_all(&path).unwrap_or_else(|e| panic!("mkdir {}: {e}", path.display()));
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
