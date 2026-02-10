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
    let loop_ref = parse_args(args)?;
    let loops = backend.list_loops()?;
    let entry = crate::run::resolve_loop_ref(&loops, &loop_ref)?;
    backend
        .run_once(&entry.id)
        .map_err(|err| format!("loop run failed: {err}"))
}

fn parse_args(args: &[String]) -> Result<String, String> {
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
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => {
                return Err("Usage: forge loop run <loop-id>".to_string());
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

    loop_ref.ok_or_else(|| "error: requires exactly 1 argument: <loop-id>".to_string())
}

#[cfg(test)]
mod tests {
    use super::{run_for_test, InMemoryLoopInternalBackend, LoopRecord, LoopState};

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
