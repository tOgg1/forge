use std::collections::BTreeMap;
use std::io::Write;

use serde::Serialize;
use serde_json::json;

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

impl LoopState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub repo: String,
    pub pool: String,
    pub profile: String,
    pub state: LoopState,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub item_type: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopSelector {
    pub all: bool,
    pub loop_ref: String,
    pub pool: String,
    pub profile: String,
    pub state: String,
    pub tag: String,
}

pub trait MsgBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String>;
    fn enqueue_items(&mut self, loop_id: &str, items: &[QueueItem]) -> Result<(), String>;
    fn render_template(&self, name: &str, vars: &[(String, String)]) -> Result<String, String>;
    fn render_sequence(
        &self,
        name: &str,
        vars: &[(String, String)],
    ) -> Result<Vec<QueueItem>, String>;
    fn resolve_prompt_path(&self, repo: &str, prompt: &str) -> Result<String, String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryMsgBackend {
    loops: Vec<LoopRecord>,
    templates: BTreeMap<String, String>,
    sequences: BTreeMap<String, Vec<QueueItem>>,
    prompt_paths: BTreeMap<(String, String), String>,
    pub enqueued: Vec<(String, Vec<QueueItem>)>,
}

impl InMemoryMsgBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        Self {
            loops,
            ..Self::default()
        }
    }

    pub fn with_template(mut self, name: &str, rendered: &str) -> Self {
        self.templates
            .insert(name.to_string(), rendered.to_string());
        self
    }

    pub fn with_sequence(mut self, name: &str, items: Vec<QueueItem>) -> Self {
        self.sequences.insert(name.to_string(), items);
        self
    }

    pub fn with_prompt_path(mut self, repo: &str, prompt: &str, resolved: &str) -> Self {
        self.prompt_paths
            .insert((repo.to_string(), prompt.to_string()), resolved.to_string());
        self
    }
}

impl MsgBackend for InMemoryMsgBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(self.loops.clone())
    }

    fn enqueue_items(&mut self, loop_id: &str, items: &[QueueItem]) -> Result<(), String> {
        if !self.loops.iter().any(|entry| entry.id == loop_id) {
            return Err(format!("loop {loop_id} not found"));
        }
        self.enqueued.push((loop_id.to_string(), items.to_vec()));
        Ok(())
    }

    fn render_template(&self, name: &str, _vars: &[(String, String)]) -> Result<String, String> {
        match self.templates.get(name) {
            Some(value) => Ok(value.clone()),
            None => Err(format!("template '{}' not found", name)),
        }
    }

    fn render_sequence(
        &self,
        name: &str,
        _vars: &[(String, String)],
    ) -> Result<Vec<QueueItem>, String> {
        match self.sequences.get(name) {
            Some(value) => Ok(value.clone()),
            None => Err(format!("sequence '{}' not found", name)),
        }
    }

    fn resolve_prompt_path(&self, repo: &str, prompt: &str) -> Result<String, String> {
        let key = (repo.to_string(), prompt.to_string());
        if let Some(value) = self.prompt_paths.get(&key) {
            return Ok(value.clone());
        }
        if prompt.starts_with('/') || repo.is_empty() {
            return Ok(prompt.to_string());
        }
        let repo_trimmed = repo.trim_end_matches('/');
        Ok(format!("{repo_trimmed}/{prompt}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
    now: bool,
    next_prompt: String,
    template: String,
    sequence: String,
    vars: Vec<(String, String)>,
    message: String,
    selector: LoopSelector,
}

#[derive(Debug, Serialize)]
struct MsgResult {
    loops: usize,
    queued: bool,
}

pub fn run_for_test(args: &[&str], backend: &mut dyn MsgBackend) -> CommandOutput {
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
    backend: &mut dyn MsgBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

fn execute(
    args: &[String],
    backend: &mut dyn MsgBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    let mut message = parsed.message.clone();
    if !parsed.template.is_empty() {
        message = backend.render_template(&parsed.template, &parsed.vars)?;
    }

    let sequence_items = if parsed.sequence.is_empty() {
        Vec::new()
    } else {
        backend.render_sequence(&parsed.sequence, &parsed.vars)?
    };

    if message.trim().is_empty()
        && sequence_items.is_empty()
        && parsed.next_prompt.trim().is_empty()
    {
        return Err("message, --seq, or --next-prompt required".to_string());
    }

    let loops = backend.list_loops()?;
    let mut matched = filter_loops(loops, &parsed.selector);
    if !parsed.selector.loop_ref.is_empty() {
        matched = match_loop_ref(&matched, &parsed.selector.loop_ref)?;
    }
    if matched.is_empty() {
        return Err("no loops matched".to_string());
    }

    for entry in &matched {
        let mut items = Vec::new();

        if !parsed.next_prompt.trim().is_empty() {
            let prompt_path = backend.resolve_prompt_path(&entry.repo, &parsed.next_prompt)?;
            let payload = json!({
                "prompt": prompt_path,
                "is_path": true
            });
            items.push(QueueItem {
                item_type: "next_prompt_override".to_string(),
                payload: serde_json::to_string(&payload).map_err(|err| err.to_string())?,
            });
        }

        if !sequence_items.is_empty() {
            items.extend(sequence_items.clone());
        }

        if !message.trim().is_empty() {
            if parsed.now {
                let payload = json!({ "message": message });
                items.push(QueueItem {
                    item_type: "steer_message".to_string(),
                    payload: serde_json::to_string(&payload).map_err(|err| err.to_string())?,
                });
            } else {
                let payload = json!({ "text": message });
                items.push(QueueItem {
                    item_type: "message_append".to_string(),
                    payload: serde_json::to_string(&payload).map_err(|err| err.to_string())?,
                });
            }
        } else if parsed.now {
            let payload = json!({ "message": "Operator interrupt" });
            items.push(QueueItem {
                item_type: "steer_message".to_string(),
                payload: serde_json::to_string(&payload).map_err(|err| err.to_string())?,
            });
        }

        backend.enqueue_items(&entry.id, &items)?;
    }

    if parsed.json || parsed.jsonl {
        let payload = MsgResult {
            loops: matched.len(),
            queued: true,
        };
        if parsed.jsonl {
            serde_json::to_writer(&mut *stdout, &payload).map_err(|err| err.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &payload).map_err(|err| err.to_string())?;
        }
        writeln!(stdout).map_err(|err| err.to_string())?;
        return Ok(());
    }

    if parsed.quiet {
        return Ok(());
    }

    writeln!(stdout, "Queued message for {} loop(s)", matched.len())
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "msg") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
    let mut now = false;
    let mut next_prompt = String::new();
    let mut template = String::new();
    let mut sequence = String::new();
    let mut selector = LoopSelector::default();
    let mut raw_vars: Vec<String> = Vec::new();
    let mut positionals: Vec<String> = Vec::new();

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => {
                return Err(HELP_TEXT.to_string());
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
            }
            "--quiet" => {
                quiet = true;
                index += 1;
            }
            "--now" => {
                now = true;
                index += 1;
            }
            "--next-prompt" => {
                next_prompt = take_value(args, index, "--next-prompt")?;
                index += 2;
            }
            "--template" => {
                template = take_value(args, index, "--template")?;
                index += 2;
            }
            "--seq" => {
                sequence = take_value(args, index, "--seq")?;
                index += 2;
            }
            "--var" => {
                raw_vars.push(take_value(args, index, "--var")?);
                index += 2;
            }
            "--pool" => {
                selector.pool = take_value(args, index, "--pool")?;
                index += 2;
            }
            "--profile" => {
                selector.profile = take_value(args, index, "--profile")?;
                index += 2;
            }
            "--state" => {
                selector.state = take_value(args, index, "--state")?;
                index += 2;
            }
            "--tag" => {
                selector.tag = take_value(args, index, "--tag")?;
                index += 2;
            }
            "--all" => {
                selector.all = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for msg: '{flag}'"));
            }
            value => {
                positionals.push(value.to_string());
                index += 1;
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }
    if !template.is_empty() && !sequence.is_empty() {
        return Err("use either --template or --seq, not both".to_string());
    }

    let mut message = String::new();
    let selector_mode = selector.all
        || !selector.pool.is_empty()
        || !selector.profile.is_empty()
        || !selector.state.is_empty()
        || !selector.tag.is_empty();

    if selector_mode {
        message = positionals.join(" ");
    } else if !positionals.is_empty() {
        let has_msg_source =
            !template.is_empty() || !sequence.is_empty() || !next_prompt.is_empty();
        if positionals.len() < 2 && !has_msg_source {
            return Err("message text required".to_string());
        }
        if positionals.len() > 1 {
            selector.loop_ref = positionals[0].clone();
            message = positionals[1..].join(" ");
        } else if has_msg_source {
            selector.loop_ref = positionals[0].clone();
        } else {
            return Err("message text required".to_string());
        }
    }

    if selector.loop_ref.is_empty() && !selector_mode {
        return Err("specify a loop or selector".to_string());
    }

    Ok(ParsedArgs {
        json,
        jsonl,
        quiet,
        now,
        next_prompt,
        template,
        sequence,
        vars: parse_key_value_pairs(&raw_vars),
        message,
        selector,
    })
}

fn parse_key_value_pairs(pairs: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for pair in pairs {
        if let Some((key, value)) = pair.split_once('=') {
            out.push((key.to_string(), value.to_string()));
        }
    }
    out
}

fn filter_loops(loops: Vec<LoopRecord>, selector: &LoopSelector) -> Vec<LoopRecord> {
    loops
        .into_iter()
        .filter(|entry| {
            (selector.pool.is_empty() || entry.pool == selector.pool)
                && (selector.profile.is_empty() || entry.profile == selector.profile)
                && (selector.state.is_empty() || entry.state.as_str() == selector.state)
                && (selector.tag.is_empty() || entry.tags.iter().any(|tag| tag == &selector.tag))
        })
        .collect()
}

fn match_loop_ref(loops: &[LoopRecord], loop_ref: &str) -> Result<Vec<LoopRecord>, String> {
    let trimmed = loop_ref.trim();
    if trimmed.is_empty() {
        return Err("loop name or ID required".to_string());
    }
    if loops.is_empty() {
        return Err(format!("loop '{}' not found", trimmed));
    }

    let found_exact_short = loops
        .iter()
        .find(|entry| short_id(entry).eq_ignore_ascii_case(trimmed));
    if let Some(entry) = found_exact_short {
        return Ok(vec![entry.clone()]);
    }

    let found_exact_id = loops.iter().find(|entry| entry.id == trimmed);
    if let Some(entry) = found_exact_id {
        return Ok(vec![entry.clone()]);
    }

    let found_exact_name = loops.iter().find(|entry| entry.name == trimmed);
    if let Some(entry) = found_exact_name {
        return Ok(vec![entry.clone()]);
    }

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
        return Ok(vec![prefix_matches.remove(0)]);
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
            "loop '{}' is ambiguous; matches: {} (use a longer prefix or full ID)",
            trimmed, labels
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

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    match args.get(index + 1) {
        Some(value) if !value.starts_with('-') => Ok(value.clone()),
        Some(_) | None => Err(format!("error: {flag} requires a value")),
    }
}

const HELP_TEXT: &str = "\
Queue a message for loop(s)

Usage:
  forge msg [loop] [message] [flags]

Flags:
      --all               target all loops
      --now               interrupt and restart immediately
      --next-prompt path  override prompt for next iteration
      --template name     message template name
      --seq name          sequence name
      --var key=value     template/sequence variable (repeatable)
      --pool string       filter by pool
      --profile string    filter by profile
      --state string      filter by state
      --tag string        filter by tag
      --json              output JSON
      --jsonl             output JSON lines
      --quiet             suppress human output";

#[cfg(test)]
mod tests {
    use super::{run_for_test, InMemoryMsgBackend, LoopRecord, LoopState, MsgBackend, QueueItem};

    #[test]
    fn parse_requires_selector() {
        let mut backend = seeded();
        let out = run_for_test(&["msg"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "specify a loop or selector\n");
    }

    #[test]
    fn parse_rejects_template_and_sequence_together() {
        let mut backend = seeded();
        let out = run_for_test(
            &["msg", "oracle-loop", "--template", "t", "--seq", "s"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "use either --template or --seq, not both\n");
    }

    #[test]
    fn now_without_message_requires_text_for_single_loop_ref() {
        let mut backend = seeded();
        let out = run_for_test(&["msg", "oracle-loop", "--now", "--json"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "message text required\n");
    }

    #[test]
    fn template_message_is_rendered() {
        let mut backend = seeded().with_template("daily", "rendered text");
        let out = run_for_test(
            &["msg", "oracle-loop", "--template", "daily", "--json"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "{\n  \"loops\": 1,\n  \"queued\": true\n}\n");

        let (_, items) = &backend.enqueued[0];
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, "message_append");
        assert_eq!(items[0].payload, "{\"text\":\"rendered text\"}");
    }

    #[test]
    fn sequence_and_next_prompt_are_enqueued_in_order() {
        let mut backend = seeded()
            .with_sequence(
                "boot",
                vec![QueueItem {
                    item_type: "pause".to_string(),
                    payload: "{\"duration_seconds\":30}".to_string(),
                }],
            )
            .with_prompt_path(
                "/repo/alpha",
                "prompts/next.md",
                "/repo/alpha/prompts/next.md",
            );
        let out = run_for_test(
            &[
                "msg",
                "oracle-loop",
                "--next-prompt",
                "prompts/next.md",
                "--seq",
                "boot",
                "--json",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        let (_, items) = &backend.enqueued[0];
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item_type, "next_prompt_override");
        assert_eq!(
            items[0].payload,
            "{\"is_path\":true,\"prompt\":\"/repo/alpha/prompts/next.md\"}"
        );
        assert_eq!(items[1].item_type, "pause");
    }

    #[test]
    fn now_with_message_sends_steer() {
        let mut backend = seeded();
        let out = run_for_test(
            &["msg", "oracle-loop", "urgent", "--now", "--json"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "{\n  \"loops\": 1,\n  \"queued\": true\n}\n");
        let (_, items) = &backend.enqueued[0];
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, "steer_message");
        assert_eq!(items[0].payload, "{\"message\":\"urgent\"}");
    }

    #[test]
    fn msg_enqueues_message_append() {
        let mut backend = seeded();
        let out = run_for_test(
            &["msg", "oracle-loop", "hello from oracle", "--json"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.enqueued.len(), 1);
        let (loop_id, items) = &backend.enqueued[0];
        assert_eq!(loop_id, "loop-001");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, "message_append");
        assert_eq!(items[0].payload, "{\"text\":\"hello from oracle\"}");
    }

    #[test]
    fn msg_json_output_matches_oracle() {
        let mut backend = seeded();
        let out = run_for_test(
            &["msg", "oracle-loop", "hello from oracle", "--json"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "{\n  \"loops\": 1,\n  \"queued\": true\n}\n");
    }

    #[test]
    fn msg_human_output() {
        let mut backend = seeded();
        let out = run_for_test(&["msg", "oracle-loop", "hello from oracle"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "Queued message for 1 loop(s)\n");
    }

    #[test]
    fn msg_quiet_suppresses_output() {
        let mut backend = seeded();
        let out = run_for_test(&["msg", "oracle-loop", "hello", "--quiet"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn msg_no_match_returns_error() {
        let mut backend = InMemoryMsgBackend::default();
        let out = run_for_test(&["msg", "--all", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "no loops matched\n");
    }

    #[test]
    fn msg_all_enqueues_for_every_loop() {
        let mut backend = seeded();
        let out = run_for_test(&["msg", "--all", "hello all", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "{\n  \"loops\": 2,\n  \"queued\": true\n}\n");
        assert_eq!(backend.enqueued.len(), 2);
        assert_eq!(backend.enqueued[0].0, "loop-001");
        assert_eq!(backend.enqueued[1].0, "loop-002");
    }

    #[test]
    fn msg_filters_by_pool() {
        let mut backend = seeded();
        let out = run_for_test(
            &["msg", "--pool", "burst", "hello burst", "--json"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.enqueued.len(), 1);
        assert_eq!(backend.enqueued[0].0, "loop-002");
    }

    #[test]
    fn msg_jsonl_output() {
        let mut backend = seeded();
        let out = run_for_test(&["msg", "oracle-loop", "hello", "--jsonl"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "{\"loops\":1,\"queued\":true}\n");
    }

    #[test]
    fn msg_ambiguous_ref_returns_error() {
        let loops = vec![
            LoopRecord {
                id: "loop-abc001".to_string(),
                short_id: "abc001".to_string(),
                name: "alpha".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Running,
                tags: vec![],
            },
            LoopRecord {
                id: "loop-abc002".to_string(),
                short_id: "abc002".to_string(),
                name: "beta".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Running,
                tags: vec![],
            },
        ];
        let mut backend = InMemoryMsgBackend::with_loops(loops);
        let out = run_for_test(&["msg", "abc", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("loop 'abc' is ambiguous; matches: alpha (abc001), beta (abc002)"));
    }

    #[test]
    fn msg_requires_message_text_for_loop_only() {
        let mut backend = seeded();
        let out = run_for_test(&["msg", "oracle-loop"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "message text required\n");
    }

    fn seeded() -> InMemoryMsgBackend {
        InMemoryMsgBackend::with_loops(vec![
            LoopRecord {
                id: "loop-001".to_string(),
                short_id: "orc01".to_string(),
                name: "oracle-loop".to_string(),
                repo: "/repo/alpha".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Running,
                tags: vec!["team-a".to_string()],
            },
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "beta02".to_string(),
                name: "beta-loop".to_string(),
                repo: "/repo/beta".to_string(),
                pool: "burst".to_string(),
                profile: "claude".to_string(),
                state: LoopState::Stopped,
                tags: vec!["team-b".to_string()],
            },
        ])
    }

    fn _assert_backend_object_safe(_backend: &mut dyn MsgBackend) {}
}
