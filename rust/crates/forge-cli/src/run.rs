use std::io::Write;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopState {
    Pending,
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub state: LoopState,
}

pub trait RunBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String>;
    fn run_once(&mut self, loop_id: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryRunBackend {
    loops: Vec<LoopRecord>,
    pub ran: Vec<String>,
}

impl InMemoryRunBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        Self {
            loops,
            ran: Vec::new(),
        }
    }
}

impl RunBackend for InMemoryRunBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(self.loops.clone())
    }

    fn run_once(&mut self, loop_id: &str) -> Result<(), String> {
        self.ran.push(loop_id.to_string());
        Ok(())
    }
}

pub fn run_for_test(args: &[&str], backend: &mut dyn RunBackend) -> CommandOutput {
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
    backend: &mut dyn RunBackend,
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

fn execute(args: &[String], backend: &mut dyn RunBackend) -> Result<(), String> {
    let loop_ref = parse_args(args)?;
    let loops = backend.list_loops()?;
    let entry = resolve_loop_ref(&loops, &loop_ref)?;
    backend
        .run_once(&entry.id)
        .map_err(|err| format!("loop run failed: {err}"))
}

fn parse_args(args: &[String]) -> Result<String, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "run") {
        index += 1;
    }

    let mut loop_ref: Option<String> = None;
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => {
                return Err("Usage: forge run <loop>".to_string());
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for run: '{flag}'"));
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

    loop_ref.ok_or_else(|| "error: requires exactly 1 argument: <loop>".to_string())
}

pub(crate) fn resolve_loop_ref(loops: &[LoopRecord], loop_ref: &str) -> Result<LoopRecord, String> {
    let trimmed = loop_ref.trim();
    if trimmed.is_empty() {
        return Err("loop name or ID required".to_string());
    }
    if loops.is_empty() {
        return Err(format!("loop '{trimmed}' not found"));
    }

    // exact short_id match
    if let Some(entry) = loops
        .iter()
        .find(|entry| short_id(entry).eq_ignore_ascii_case(trimmed))
    {
        return Ok(entry.clone());
    }

    // exact id match
    if let Some(entry) = loops.iter().find(|entry| entry.id == trimmed) {
        return Ok(entry.clone());
    }

    // exact name match
    if let Some(entry) = loops.iter().find(|entry| entry.name == trimmed) {
        return Ok(entry.clone());
    }

    // prefix match
    let normalized = trimmed.to_ascii_lowercase();
    let mut prefix_matches: Vec<LoopRecord> = loops
        .iter()
        .filter(|entry| {
            short_id(entry)
                .to_ascii_lowercase()
                .starts_with(&normalized)
                || entry.id.starts_with(trimmed)
        })
        .cloned()
        .collect();

    if prefix_matches.len() == 1 {
        return Ok(prefix_matches.remove(0));
    }

    if !prefix_matches.is_empty() {
        prefix_matches.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| short_id(left).cmp(short_id(right)))
        });
        let labels = prefix_matches
            .iter()
            .map(format_loop_match)
            .collect::<Vec<String>>()
            .join(", ");
        return Err(format!(
            "loop '{trimmed}' is ambiguous; matches: {labels} (use a longer prefix or full ID)"
        ));
    }

    let example = &loops[0];
    Err(format!(
        "loop '{}' not found. Example input: '{}' or '{}'",
        trimmed,
        example.name,
        short_id(example)
    ))
}

fn short_id(entry: &LoopRecord) -> &str {
    if entry.short_id.is_empty() {
        return &entry.id;
    }
    &entry.short_id
}

fn format_loop_match(entry: &LoopRecord) -> String {
    format!("{} ({})", entry.name, short_id(entry))
}

#[cfg(test)]
mod tests {
    use super::{run_for_test, InMemoryRunBackend, LoopRecord, LoopState};

    #[test]
    fn run_requires_loop_arg() {
        let mut backend = InMemoryRunBackend::default();
        let out = run_for_test(&["run"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "error: requires exactly 1 argument: <loop>\n");
    }

    #[test]
    fn run_rejects_extra_args() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "alpha", "beta"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "error: accepts exactly 1 argument, received multiple\n"
        );
    }

    #[test]
    fn run_rejects_unknown_flags() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "--bogus"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "error: unknown argument for run: '--bogus'\n");
    }

    #[test]
    fn run_resolves_by_name() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "alpha"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
        assert!(out.stderr.is_empty());
        assert_eq!(backend.ran, vec!["loop-001"]);
    }

    #[test]
    fn run_resolves_by_short_id() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "abc001"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
        assert!(out.stderr.is_empty());
        assert_eq!(backend.ran, vec!["loop-001"]);
    }

    #[test]
    fn run_resolves_by_full_id() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "loop-001"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
        assert!(out.stderr.is_empty());
        assert_eq!(backend.ran, vec!["loop-001"]);
    }

    #[test]
    fn run_missing_loop_returns_error() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "does-not-exist"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "loop 'does-not-exist' not found. Example input: 'alpha' or 'abc001'\n"
        );
    }

    #[test]
    fn run_backend_failure_wraps_error() {
        let mut backend = FailingRunBackend;
        let out = run_for_test(&["run", "any-loop"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "loop run failed: database error\n");
    }

    #[test]
    fn run_ambiguous_prefix_reports_matches() {
        let mut backend = InMemoryRunBackend::with_loops(vec![
            LoopRecord {
                id: "loop-abc001".to_string(),
                short_id: "abc001".to_string(),
                name: "alpha".to_string(),
                state: LoopState::Stopped,
            },
            LoopRecord {
                id: "loop-abc002".to_string(),
                short_id: "abc002".to_string(),
                name: "beta".to_string(),
                state: LoopState::Stopped,
            },
        ]);
        let out = run_for_test(&["run", "abc"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "loop 'abc' is ambiguous; matches: alpha (abc001), beta (abc002) (use a longer prefix or full ID)\n"
        );
    }

    #[test]
    fn run_produces_no_stdout_on_success() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "alpha"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(
            out.stdout.is_empty(),
            "run should produce no stdout on success"
        );
        assert!(out.stderr.is_empty());
    }

    fn seeded() -> InMemoryRunBackend {
        InMemoryRunBackend::with_loops(vec![
            LoopRecord {
                id: "loop-001".to_string(),
                short_id: "abc001".to_string(),
                name: "alpha".to_string(),
                state: LoopState::Running,
            },
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "def002".to_string(),
                name: "beta".to_string(),
                state: LoopState::Stopped,
            },
        ])
    }

    struct FailingRunBackend;

    impl super::RunBackend for FailingRunBackend {
        fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
            Ok(vec![LoopRecord {
                id: "loop-fail".to_string(),
                short_id: "fail01".to_string(),
                name: "any-loop".to_string(),
                state: LoopState::Running,
            }])
        }

        fn run_once(&mut self, _loop_id: &str) -> Result<(), String> {
            Err("database error".to_string())
        }
    }
}
