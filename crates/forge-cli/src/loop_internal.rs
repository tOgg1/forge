use std::io::Write;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub use crate::run::{
    InMemoryRunBackend as InMemoryLoopInternalBackend, LoopRecord, LoopState,
    RunBackend as LoopInternalBackend,
};

pub fn run_for_test(args: &[&str], backend: &mut dyn LoopInternalBackend) -> CommandOutput {
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
    backend: &mut dyn LoopInternalBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let _ = stdout;
    match execute(args, backend) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

fn execute(args: &[String], backend: &mut dyn LoopInternalBackend) -> Result<(), String> {
    let parsed = parse_args(args)?;
    if let Some(node_id) = parsed.node.as_deref() {
        let command = crate::node::build_remote_command(
            "forge loop run",
            std::slice::from_ref(&parsed.loop_ref),
            false,
            false,
        );
        let result = crate::node::route_exec(node_id, &command)?;
        if result.exit_code != 0 {
            let detail = result.stderr.trim();
            if detail.is_empty() {
                return Err(format!(
                    "remote loop run failed on node {node_id} (exit code {})",
                    result.exit_code
                ));
            }
            return Err(format!(
                "remote loop run failed on node {node_id} (exit code {}): {detail}",
                result.exit_code
            ));
        }
        return Ok(());
    }

    let loops = backend.list_loops()?;
    let entry = crate::run::resolve_loop_ref(&loops, &parsed.loop_ref)?;
    backend
        .run_loop(&entry.id)
        .map_err(|err| format!("loop run failed: {err}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    loop_ref: String,
    node: Option<String>,
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;

    if args.get(index).is_some_and(|token| token == "loop") {
        index += 1;
    }

    match args.get(index).map(|token| token.as_str()) {
        Some("run") => {
            index += 1;
        }
        Some("-h") | Some("--help") | Some("help") | None => {
            return Err("Usage: forge loop run <loop-id>".to_string());
        }
        Some(other) => {
            return Err(format!("error: unknown argument for loop: '{other}'"));
        }
    }

    let mut loop_ref: Option<String> = None;
    let mut node: Option<String> = None;
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => {
                return Err("Usage: forge loop run <loop-id>".to_string());
            }
            "--node" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "usage: --node <node-id>".to_string())?
                    .trim()
                    .to_string();
                if value.is_empty() {
                    return Err("usage: --node <node-id>".to_string());
                }
                node = Some(value);
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for loop run: '{flag}'"));
            }
            value => {
                if loop_ref.is_some() {
                    return Err("error: accepts exactly 1 argument, received multiple".to_string());
                }
                loop_ref = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(ParsedArgs {
        loop_ref: loop_ref
            .ok_or_else(|| "error: requires exactly 1 argument: <loop-id>".to_string())?,
        node,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_args, run_for_test, InMemoryLoopInternalBackend, LoopRecord, LoopState};

    #[test]
    fn requires_run_subcommand() {
        let mut backend = seeded();
        let out = run_for_test(&["loop"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "Usage: forge loop run <loop-id>\n");
    }

    #[test]
    fn loop_run_executes_for_resolved_loop() {
        let mut backend = seeded();
        let out = run_for_test(&["loop", "run", "alpha"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
        assert!(out.stderr.is_empty());
        assert_eq!(backend.ran, vec!["loop-001"]);
    }

    #[test]
    fn backend_error_is_wrapped() {
        let mut backend = FailingBackend;
        let out = run_for_test(&["loop", "run", "alpha"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "loop run failed: runner crashed\n");
    }

    #[test]
    fn parse_accepts_node_flag() {
        let parsed = match parse_args(&[
            "loop".to_string(),
            "run".to_string(),
            "alpha".to_string(),
            "--node".to_string(),
            "node-a".to_string(),
        ]) {
            Ok(parsed) => parsed,
            Err(err) => panic!("expected parse success, got error: {err}"),
        };
        assert_eq!(parsed.loop_ref, "alpha");
        assert_eq!(parsed.node.as_deref(), Some("node-a"));
    }

    #[test]
    fn parse_rejects_node_flag_without_value() {
        let err = match parse_args(&[
            "loop".to_string(),
            "run".to_string(),
            "alpha".to_string(),
            "--node".to_string(),
        ]) {
            Ok(_) => panic!("expected parse failure"),
            Err(err) => err,
        };
        assert_eq!(err, "usage: --node <node-id>");
    }

    fn seeded() -> InMemoryLoopInternalBackend {
        InMemoryLoopInternalBackend::with_loops(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            state: LoopState::Stopped,
        }])
    }

    struct FailingBackend;

    impl super::LoopInternalBackend for FailingBackend {
        fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
            Ok(vec![LoopRecord {
                id: "loop-001".to_string(),
                short_id: "abc001".to_string(),
                name: "alpha".to_string(),
                state: LoopState::Running,
            }])
        }

        fn run_once(&mut self, _loop_id: &str) -> Result<(), String> {
            Err("runner crashed".to_string())
        }
    }
}
