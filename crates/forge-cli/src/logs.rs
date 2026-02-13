use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use serde_json::Value;

use crate::command_renderer::{
    looks_like_command_prompt, looks_like_exit_code, render_command_lines,
};
use crate::diff_renderer::{
    flush_diff_lines, render_diff_lines, render_diff_lines_incremental, DiffRenderState,
};
use crate::error_renderer::render_error_lines;
use crate::section_parser::{SectionEvent, SectionKind, SectionParser};
use crate::structured_data_renderer::render_structured_data_line;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub repo: String,
    pub log_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    loop_ref: String,
    all: bool,
    follow: bool,
    lines: i32,
    since: String,
    no_color: bool,
    raw: bool,
    compact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderOptions {
    no_color: bool,
    raw: bool,
    compact: bool,
}

/// Semantic log layer used by shared renderers (CLI/TUI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogRenderLayer {
    Raw,
    Events,
    Errors,
    Tools,
    Diff,
}

/// Render already-loaded log lines using the same parser/renderer pipeline as
/// `forge logs`, constrained to one semantic layer.
#[must_use]
pub fn render_lines_for_layer(
    lines: &[String],
    layer: LogRenderLayer,
    no_color: bool,
) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }

    let filtered = filter_lines_for_layer(lines, layer);
    if filtered.is_empty() {
        return Vec::new();
    }

    let rendered = render_log_content(
        &filtered.join("\n"),
        RenderOptions {
            no_color,
            raw: false,
            compact: false,
        },
    );

    rendered.lines().map(str::to_owned).collect()
}

pub trait LogsBackend {
    fn data_dir(&self) -> &str;
    fn repo_path(&self) -> Result<String, String>;
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String>;
    fn read_log(&self, path: &str, lines: i32, since: &str) -> Result<String, String>;
    fn follow_log(
        &mut self,
        path: &str,
        lines: i32,
        render: RenderOptions,
        stdout: &mut dyn Write,
    ) -> Result<(), String>;
}

#[derive(Debug, Clone)]
pub struct InMemoryLogsBackend {
    loops: Vec<LoopRecord>,
    data_dir: String,
    repo_path: String,
    logs: BTreeMap<String, String>,
    follow_output: BTreeMap<String, String>,
    pub followed_paths: Vec<(String, i32)>,
}

impl Default for InMemoryLogsBackend {
    fn default() -> Self {
        Self {
            loops: Vec::new(),
            data_dir: "/tmp/forge".to_string(),
            repo_path: "/repo".to_string(),
            logs: BTreeMap::new(),
            follow_output: BTreeMap::new(),
            followed_paths: Vec::new(),
        }
    }
}

impl InMemoryLogsBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        Self {
            loops,
            ..Self::default()
        }
    }

    pub fn with_repo_path(mut self, repo_path: &str) -> Self {
        self.repo_path = repo_path.to_string();
        self
    }

    pub fn with_data_dir(mut self, data_dir: &str) -> Self {
        self.data_dir = data_dir.to_string();
        self
    }

    pub fn with_log(mut self, path: &str, content: &str) -> Self {
        self.logs.insert(path.to_string(), content.to_string());
        self
    }

    pub fn with_follow_output(mut self, path: &str, content: &str) -> Self {
        self.follow_output
            .insert(path.to_string(), content.to_string());
        self
    }
}

impl LogsBackend for InMemoryLogsBackend {
    fn data_dir(&self) -> &str {
        &self.data_dir
    }

    fn repo_path(&self) -> Result<String, String> {
        Ok(self.repo_path.clone())
    }

    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(self.loops.clone())
    }

    fn read_log(&self, path: &str, lines: i32, since: &str) -> Result<String, String> {
        let Some(content) = self.logs.get(path) else {
            return Err(format!("open {path}: no such file or directory"));
        };
        Ok(filter_log_content(content, lines, since))
    }

    fn follow_log(
        &mut self,
        path: &str,
        lines: i32,
        render: RenderOptions,
        stdout: &mut dyn Write,
    ) -> Result<(), String> {
        self.followed_paths.push((path.to_string(), lines));
        if let Some(text) = self.follow_output.get(path) {
            let rendered = render_log_content(text, render);
            write_log_block(stdout, &rendered)?;
            return Ok(());
        }
        let tail = self.read_log(path, lines, "")?;
        let rendered = render_log_content(&tail, render);
        write_log_block(stdout, &rendered)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SqliteLogsBackend {
    db_path: PathBuf,
    data_dir: String,
}

impl SqliteLogsBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
            data_dir: resolve_data_dir(),
        }
    }

    pub fn new(db_path: PathBuf, data_dir: String) -> Self {
        Self { db_path, data_dir }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))
    }
}

impl LogsBackend for SqliteLogsBackend {
    fn data_dir(&self) -> &str {
        &self.data_dir
    }

    fn repo_path(&self) -> Result<String, String> {
        std::env::current_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|err| format!("resolve current directory: {err}"))
    }

    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = self.open_db()?;
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let loops = match loop_repo.list() {
            Ok(value) => value,
            Err(err) if err.to_string().contains("no such table: loops") => return Ok(Vec::new()),
            Err(err) => return Err(err.to_string()),
        };

        Ok(loops
            .into_iter()
            .map(|entry| LoopRecord {
                id: entry.id.clone(),
                short_id: if entry.short_id.is_empty() {
                    entry.id
                } else {
                    entry.short_id
                },
                name: entry.name,
                repo: entry.repo_path,
                log_path: entry.log_path,
            })
            .collect())
    }

    fn read_log(&self, path: &str, lines: i32, since: &str) -> Result<String, String> {
        let content = std::fs::read_to_string(path).map_err(|err| format!("open {path}: {err}"))?;
        Ok(filter_log_content(&content, lines, since))
    }

    fn follow_log(
        &mut self,
        path: &str,
        lines: i32,
        render: RenderOptions,
        stdout: &mut dyn Write,
    ) -> Result<(), String> {
        let mut diff_state = DiffRenderState::default();
        let tail = self.read_log(path, lines, "")?;
        let rendered = render_log_chunk(&tail, render, &mut diff_state);
        write_log_block(stdout, &rendered)?;
        if std::env::var_os("FORGE_LOGS_FOLLOW_ONCE").is_some() {
            let trailing = flush_diff_lines(colors_enabled(render.no_color), &mut diff_state);
            if !trailing.is_empty() {
                write_log_block(stdout, &trailing.join("\n"))?;
            }
            return Ok(());
        }

        let mut known_content =
            std::fs::read_to_string(path).map_err(|err| format!("open {path}: {err}"))?;
        let mut carry = String::new();

        loop {
            thread::sleep(Duration::from_millis(250));
            let current =
                std::fs::read_to_string(path).map_err(|err| format!("open {path}: {err}"))?;
            if current == known_content {
                continue;
            }

            let delta = if current.starts_with(&known_content) {
                current[known_content.len()..].to_string()
            } else {
                // File truncated/rotated/replaced: treat full current content as new.
                diff_state.reset();
                carry.clear();
                current.clone()
            };
            known_content = current;
            if delta.is_empty() {
                continue;
            }

            let chunk = if carry.is_empty() {
                delta
            } else {
                format!("{carry}{delta}")
            };
            let (complete, rest) = split_complete_lines(&chunk);
            carry = rest;

            if complete.is_empty() {
                continue;
            }
            let rendered = render_log_chunk(&complete, render, &mut diff_state);
            write_log_block(stdout, &rendered)?;
        }
    }
}

pub fn default_log_path(data_dir: &str, name: &str, id: &str) -> String {
    let slug = loop_slug(name);
    let file_stem = if slug.is_empty() { id } else { slug.as_str() };
    format!("{data_dir}/logs/loops/{file_stem}.log")
}

fn resolve_database_path() -> PathBuf {
    crate::runtime_paths::resolve_database_path()
}

fn resolve_data_dir() -> String {
    crate::runtime_paths::resolve_data_dir()
        .to_string_lossy()
        .into_owned()
}

fn filter_log_content(content: &str, lines: i32, since: &str) -> String {
    let limit = if lines <= 0 { 50 } else { lines as usize };
    let since_marker = parse_since_marker(since);
    let mut filtered = Vec::new();

    for line in content.lines() {
        if let Some(marker) = since_marker.as_deref() {
            if let Some(ts) = parse_log_timestamp(line) {
                if ts < marker {
                    continue;
                }
            }
        }
        filtered.push(line.to_string());
    }

    if filtered.len() > limit {
        filtered = filtered.split_off(filtered.len() - limit);
    }
    filtered.join("\n")
}

pub fn run_for_test(args: &[&str], backend: &mut dyn LogsBackend) -> CommandOutput {
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
    backend: &mut dyn LogsBackend,
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
    backend: &mut dyn LogsBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    let render = RenderOptions {
        no_color: parsed.no_color,
        raw: parsed.raw,
        compact: parsed.compact,
    };
    let mut loops = backend.list_loops()?;

    if parsed.all {
        let repo = backend.repo_path()?;
        loops.retain(|entry| entry.repo == repo);
    }

    if !parsed.loop_ref.is_empty() {
        loops = match_loop_ref(&loops, &parsed.loop_ref)?;
    }

    if loops.is_empty() {
        return Err("no loops matched".to_string());
    }

    for (index, entry) in loops.iter().enumerate() {
        let path = if entry.log_path.is_empty() {
            default_log_path(backend.data_dir(), &entry.name, &entry.id)
        } else {
            entry.log_path.clone()
        };

        if index > 0 {
            writeln!(stdout).map_err(|err| err.to_string())?;
        }
        writeln!(stdout, "==> {} <==", entry.name).map_err(|err| err.to_string())?;

        if parsed.follow {
            backend.follow_log(&path, parsed.lines, render, stdout)?;
            continue;
        }

        let content = backend.read_log(&path, parsed.lines, &parsed.since)?;
        let rendered = render_log_content(&content, render);
        write_log_block(stdout, &rendered)?;
    }
    Ok(())
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args
        .get(index)
        .is_some_and(|token| token == "logs" || token == "log")
    {
        index += 1;
    }

    let mut all = false;
    let mut follow = false;
    let mut lines: i32 = 50;
    let mut since = String::new();
    let mut no_color = false;
    let mut raw = false;
    let mut compact = false;
    let mut positionals = Vec::new();

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => return Err(HELP_TEXT.to_string()),
            "-f" | "--follow" => {
                follow = true;
                index += 1;
            }
            "-n" | "--lines" => {
                let raw = take_value(args, index, "--lines")?;
                lines = raw
                    .parse::<i32>()
                    .map_err(|_| format!("error: invalid value '{}' for --lines", raw))?;
                index += 2;
            }
            "--since" => {
                since = take_value(args, index, "--since")?;
                index += 2;
            }
            "--all" => {
                all = true;
                index += 1;
            }
            "--no-color" => {
                no_color = true;
                index += 1;
            }
            "--raw" => {
                raw = true;
                index += 1;
            }
            "--compact" => {
                compact = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for logs: '{flag}'"));
            }
            value => {
                positionals.push(value.to_string());
                index += 1;
            }
        }
    }

    if positionals.len() > 1 {
        return Err("error: accepts at most 1 argument, received multiple".to_string());
    }

    let loop_ref = positionals.into_iter().next().unwrap_or_default();
    if loop_ref.is_empty() && !all {
        return Err("loop name required (or use --all)".to_string());
    }

    Ok(ParsedArgs {
        loop_ref,
        all,
        follow,
        lines,
        since,
        no_color,
        raw,
        compact,
    })
}

fn write_log_block(stdout: &mut dyn Write, content: &str) -> Result<(), String> {
    if content.is_empty() {
        return Ok(());
    }
    write!(stdout, "{content}").map_err(|err| err.to_string())?;
    if !content.ends_with('\n') {
        writeln!(stdout).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn split_complete_lines(input: &str) -> (String, String) {
    if input.is_empty() {
        return (String::new(), String::new());
    }
    if input.ends_with('\n') {
        return (input.to_string(), String::new());
    }
    match input.rfind('\n') {
        Some(index) => (input[..=index].to_string(), input[index + 1..].to_string()),
        None => (String::new(), input.to_string()),
    }
}

fn render_log_content(content: &str, options: RenderOptions) -> String {
    if options.raw || content.is_empty() {
        return content.to_string();
    }

    let use_color = colors_enabled(options.no_color);
    let out = collect_render_lines(content, use_color);
    let out = render_section_aware(&out, use_color, options.compact);
    render_diff_lines(&out, use_color).join("\n")
}

fn render_log_chunk(
    content: &str,
    options: RenderOptions,
    diff_state: &mut DiffRenderState,
) -> String {
    if options.raw || content.is_empty() {
        return content.to_string();
    }

    let use_color = colors_enabled(options.no_color);
    let out = collect_render_lines(content, use_color);
    let out = render_section_aware(&out, use_color, options.compact);
    render_diff_lines_incremental(&out, use_color, diff_state).join("\n")
}

fn filter_lines_for_layer(lines: &[String], layer: LogRenderLayer) -> Vec<String> {
    if matches!(layer, LogRenderLayer::Raw) {
        return lines.to_vec();
    }

    let mut parser = SectionParser::new();
    let mut filtered = Vec::new();
    let mut in_command_block = false;

    for line in lines {
        let events = parser.feed(line);
        let kind = line_kind_for_events(&events);

        let starts_command = matches!(kind, SectionKind::Unknown)
            && (looks_like_command_prompt(line) || looks_like_exit_code(line));
        if starts_command {
            in_command_block = true;
        } else if !matches!(kind, SectionKind::Unknown) {
            in_command_block = false;
        }

        let include = match layer {
            LogRenderLayer::Raw => true,
            LogRenderLayer::Events => is_event_layer_kind(kind),
            LogRenderLayer::Errors => kind == SectionKind::ErrorBlock,
            LogRenderLayer::Tools => kind == SectionKind::ToolCall || in_command_block,
            LogRenderLayer::Diff => kind == SectionKind::Diff,
        };

        if include {
            filtered.push(line.clone());
        }
    }

    filtered
}

fn line_kind_for_events(events: &[SectionEvent]) -> SectionKind {
    for event in events {
        match event {
            SectionEvent::Start { kind, .. } | SectionEvent::Continue { kind, .. } => {
                return *kind;
            }
            SectionEvent::End { .. } => {}
        }
    }
    SectionKind::Unknown
}

fn is_event_layer_kind(kind: SectionKind) -> bool {
    matches!(
        kind,
        SectionKind::HarnessHeader
            | SectionKind::RoleMarker
            | SectionKind::Thinking
            | SectionKind::JsonEvent
            | SectionKind::Summary
            | SectionKind::Approval
            | SectionKind::StatusLine
    )
}

fn collect_render_lines(content: &str, use_color: bool) -> Vec<String> {
    let mut out = Vec::new();

    for line in content.lines() {
        match maybe_render_claude_stream_line(line, use_color) {
            Some(rendered) if !rendered.is_empty() => out.push(rendered),
            Some(_) => {}
            None => out.push(line.to_string()),
        }
    }
    out
}

const COLOR_BOLD: &str = "\x1b[1m";
const COLOR_MAGENTA: &str = "\x1b[35m";

/// Section-aware rendering pass: emphasize headers, dim timestamps, insert
/// separators between major sections, collapse blocks in compact mode, and
/// route error/command blocks through their respective renderers.
fn render_section_aware(lines: &[String], use_color: bool, compact: bool) -> Vec<String> {
    let mut parser = SectionParser::new();
    let mut out = Vec::with_capacity(lines.len());
    let mut error_buf: Vec<String> = Vec::new();
    let mut in_error = false;
    let mut cmd_buf: Vec<String> = Vec::new();
    let mut in_cmd = false;
    let mut prev_major: Option<SectionKind> = None;
    // Compact-mode collapse tracking.
    let mut collapse_kind: Option<SectionKind> = None;
    let mut collapse_count: usize = 0;

    for line in lines {
        let events = parser.feed(line);
        for event in &events {
            match event {
                // ── Error block: accumulate and delegate ─────────────
                SectionEvent::Start {
                    kind: SectionKind::ErrorBlock,
                    line: l,
                    ..
                } => {
                    flush_cmd_buf(&mut out, &mut cmd_buf, &mut in_cmd, use_color);
                    flush_collapse(&mut out, &mut collapse_kind, &mut collapse_count, use_color);
                    maybe_insert_separator(
                        &mut out,
                        &mut prev_major,
                        SectionKind::ErrorBlock,
                        use_color,
                    );
                    in_error = true;
                    error_buf.push(l.clone());
                }
                SectionEvent::Continue {
                    kind: SectionKind::ErrorBlock,
                    line: l,
                    ..
                } => {
                    error_buf.push(l.clone());
                }
                SectionEvent::End {
                    kind: SectionKind::ErrorBlock,
                    ..
                } => {
                    out.extend(render_error_lines(&error_buf, use_color));
                    error_buf.clear();
                    in_error = false;
                }
                // ── Section start ────────────────────────────────────
                SectionEvent::Start { kind, line: l, .. } => {
                    if in_error {
                        continue;
                    }
                    // Check for command transcript: Unknown lines that look
                    // like shell prompt or are continuation of a command block.
                    if *kind == SectionKind::Unknown {
                        if looks_like_command_prompt(l) || looks_like_exit_code(l) {
                            if !in_cmd {
                                flush_collapse(
                                    &mut out,
                                    &mut collapse_kind,
                                    &mut collapse_count,
                                    use_color,
                                );
                                in_cmd = true;
                            }
                            cmd_buf.push(l.clone());
                            continue;
                        }
                        if in_cmd {
                            // Continuation of command output (stdout lines).
                            cmd_buf.push(l.clone());
                            continue;
                        }
                    } else {
                        flush_cmd_buf(&mut out, &mut cmd_buf, &mut in_cmd, use_color);
                    }
                    flush_collapse(&mut out, &mut collapse_kind, &mut collapse_count, use_color);
                    maybe_insert_separator(&mut out, &mut prev_major, *kind, use_color);
                    let styled = style_section_line(l, *kind, use_color);
                    if compact && is_collapsible(*kind) {
                        out.push(styled);
                        collapse_kind = Some(*kind);
                        collapse_count = 0;
                    } else {
                        out.push(styled);
                    }
                }
                // ── Section continue ─────────────────────────────────
                SectionEvent::Continue { kind, line: l, .. } => {
                    if in_error {
                        continue;
                    }
                    if compact && collapse_kind == Some(*kind) {
                        collapse_count += 1;
                        continue;
                    }
                    out.push(style_section_line(l, *kind, use_color));
                }
                // ── Section end ──────────────────────────────────────
                SectionEvent::End { kind, .. } => {
                    if *kind != SectionKind::ErrorBlock {
                        flush_collapse(
                            &mut out,
                            &mut collapse_kind,
                            &mut collapse_count,
                            use_color,
                        );
                    }
                }
            }
        }
    }

    // Flush any remaining error block.
    if !error_buf.is_empty() {
        out.extend(render_error_lines(&error_buf, use_color));
    }
    flush_cmd_buf(&mut out, &mut cmd_buf, &mut in_cmd, use_color);
    flush_collapse(&mut out, &mut collapse_kind, &mut collapse_count, use_color);

    out
}

/// Flush accumulated command transcript buffer through the command renderer.
fn flush_cmd_buf(
    out: &mut Vec<String>,
    cmd_buf: &mut Vec<String>,
    in_cmd: &mut bool,
    use_color: bool,
) {
    if !cmd_buf.is_empty() {
        out.extend(render_command_lines(cmd_buf, use_color));
        cmd_buf.clear();
    }
    *in_cmd = false;
}

/// Apply section-aware styling to a single line.
fn style_section_line(line: &str, kind: SectionKind, use_color: bool) -> String {
    match kind {
        SectionKind::HarnessHeader => {
            if use_color {
                format!("{COLOR_BOLD}{COLOR_CYAN}{line}{COLOR_RESET}")
            } else {
                format!("== {line}")
            }
        }
        SectionKind::RoleMarker => {
            if use_color {
                format!("{COLOR_BOLD}{COLOR_MAGENTA}{line}{COLOR_RESET}")
            } else {
                format!(">> {line}")
            }
        }
        SectionKind::StatusLine => {
            if use_color {
                format!("{COLOR_DIM}{line}{COLOR_RESET}")
            } else {
                line.to_string()
            }
        }
        SectionKind::Summary => {
            if use_color {
                format!("{COLOR_DIM}{line}{COLOR_RESET}")
            } else {
                line.to_string()
            }
        }
        SectionKind::ToolCall => {
            if use_color {
                format!("{COLOR_YELLOW}{line}{COLOR_RESET}")
            } else {
                line.to_string()
            }
        }
        SectionKind::Thinking => {
            if use_color {
                format!("{COLOR_DIM}{line}{COLOR_RESET}")
            } else {
                line.to_string()
            }
        }
        // JsonEvent: apply structured-data semantic highlighting.
        SectionKind::JsonEvent => render_structured_data_line(line, use_color),
        // CodeFence, Diff, Approval, Unknown, ErrorBlock — pass through.
        // Diff and CodeFence get styled by diff_renderer later in the pipeline.
        _ => line.to_string(),
    }
}

/// Insert a visual separator between major sections when appropriate.
fn maybe_insert_separator(
    out: &mut Vec<String>,
    prev_major: &mut Option<SectionKind>,
    next: SectionKind,
    use_color: bool,
) {
    if let Some(prev) = *prev_major {
        if should_insert_separator(prev, next) {
            let rule = "\u{2500}".repeat(40); // ─ repeated
            if use_color {
                out.push(format!("{COLOR_DIM}{rule}{COLOR_RESET}"));
            } else {
                out.push(rule);
            }
        }
    }
    if is_major_section(next) {
        *prev_major = Some(next);
    }
}

fn should_insert_separator(prev: SectionKind, next: SectionKind) -> bool {
    // Don't double-separate same-kind adjacent major sections.
    if prev == next {
        return false;
    }
    true
}

fn is_major_section(kind: SectionKind) -> bool {
    matches!(
        kind,
        SectionKind::HarnessHeader
            | SectionKind::RoleMarker
            | SectionKind::Diff
            | SectionKind::CodeFence
            | SectionKind::Summary
            | SectionKind::Approval
            | SectionKind::ErrorBlock
    )
}

fn is_collapsible(kind: SectionKind) -> bool {
    matches!(kind, SectionKind::Thinking | SectionKind::CodeFence)
}

fn flush_collapse(
    out: &mut Vec<String>,
    collapse_kind: &mut Option<SectionKind>,
    collapse_count: &mut usize,
    use_color: bool,
) {
    if let Some(ck) = collapse_kind.take() {
        if *collapse_count > 0 {
            let label = match ck {
                SectionKind::Thinking => "thinking",
                SectionKind::CodeFence => "code",
                _ => "content",
            };
            let text = format!("  ... ({} {label} lines collapsed)", *collapse_count);
            if use_color {
                out.push(format!("{COLOR_DIM}{text}{COLOR_RESET}"));
            } else {
                out.push(text);
            }
        }
        *collapse_count = 0;
    }
}

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_DIM: &str = "\x1b[2m";
const COLOR_CYAN: &str = "\x1b[36m";
const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_YELLOW: &str = "\x1b[33m";
const COLOR_RED: &str = "\x1b[31m";

fn colors_enabled(no_color: bool) -> bool {
    if no_color {
        return false;
    }
    std::env::var_os("NO_COLOR").is_none()
}

fn colorize(input: &str, color: &str, enabled: bool) -> String {
    if !enabled || input.is_empty() {
        return input.to_string();
    }
    format!("{color}{input}{COLOR_RESET}")
}

fn maybe_render_claude_stream_line(line: &str, use_color: bool) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return None;
    }

    let value: Value = serde_json::from_str(trimmed).ok()?;
    let event_type = value.get("type").and_then(Value::as_str)?;

    match event_type {
        "system" => render_claude_system_line(&value, use_color),
        "stream_event" => render_claude_stream_event_line(&value, use_color),
        "result" => render_claude_result_line(&value, use_color),
        "error" => {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            Some(format!(
                "{} {}",
                colorize("[claude:error]", COLOR_RED, use_color),
                message
            ))
        }
        // Avoid duplicate full-response payloads in stream-json mode.
        "assistant" | "user" => Some(String::new()),
        _ => None,
    }
}

fn render_claude_system_line(value: &Value, use_color: bool) -> Option<String> {
    if value.get("subtype").and_then(Value::as_str) != Some("init") {
        return None;
    }
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let tools = value
        .get("tools")
        .and_then(Value::as_array)
        .map_or(0, std::vec::Vec::len);
    let mcp = value
        .get("mcp_servers")
        .and_then(Value::as_array)
        .map_or(0, std::vec::Vec::len);
    let session = value
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("-");
    Some(format!(
        "{} model={model} tools={tools} mcp={mcp} session={session}",
        colorize("[claude:init]", COLOR_CYAN, use_color)
    ))
}

fn render_claude_stream_event_line(value: &Value, use_color: bool) -> Option<String> {
    let event = value.get("event")?;
    let event_type = event.get("type").and_then(Value::as_str).unwrap_or("-");
    match event_type {
        "content_block_delta" => {
            let delta = event.get("delta")?;
            if delta.get("type").and_then(Value::as_str) != Some("text_delta") {
                return Some(String::new());
            }
            let text = delta
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if text.is_empty() {
                return Some(String::new());
            }
            Some(colorize(text, COLOR_GREEN, use_color))
        }
        "message_delta" => {
            let stop_reason = event
                .get("delta")
                .and_then(|delta| delta.get("stop_reason"))
                .and_then(Value::as_str);
            if let Some(reason) = stop_reason {
                return Some(format!(
                    "{} stop_reason={reason}",
                    colorize("[claude:event]", COLOR_DIM, use_color)
                ));
            }
            Some(String::new())
        }
        "message_start" | "message_stop" | "content_block_start" | "content_block_stop" => {
            Some(String::new())
        }
        other => Some(format!(
            "{} {}",
            colorize("[claude:event]", COLOR_DIM, use_color),
            other
        )),
    }
}

fn render_claude_result_line(value: &Value, use_color: bool) -> Option<String> {
    let turns = value.get("num_turns").and_then(Value::as_i64).unwrap_or(0);
    let duration_ms = value
        .get("duration_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let usage = value.get("usage");
    let input_tokens = usage
        .and_then(|usage| usage.get("input_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output_tokens = usage
        .and_then(|usage| usage.get("output_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let cost = value
        .get("total_cost_usd")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let duration_seconds = duration_ms as f64 / 1000.0;
    Some(format!(
        "{} turns={turns} duration={duration_seconds:.1}s input={input_tokens} output={output_tokens} cost=${cost:.6}",
        colorize("[claude:result]", COLOR_YELLOW, use_color)
    ))
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

fn match_loop_ref(loops: &[LoopRecord], loop_ref: &str) -> Result<Vec<LoopRecord>, String> {
    let trimmed = loop_ref.trim();
    if trimmed.is_empty() {
        return Err("loop name or ID required".to_string());
    }
    if loops.is_empty() {
        return Err(format!("loop '{trimmed}' not found"));
    }

    if let Some(entry) = loops
        .iter()
        .find(|entry| short_id(entry).eq_ignore_ascii_case(trimmed))
    {
        return Ok(vec![entry.clone()]);
    }

    if let Some(entry) = loops.iter().find(|entry| entry.id == trimmed) {
        return Ok(vec![entry.clone()]);
    }

    if let Some(entry) = loops.iter().find(|entry| entry.name == trimmed) {
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

fn parse_since_marker(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if is_rfc3339_utc(trimmed) {
        return Some(trimmed.to_string());
    }
    None
}

fn parse_log_timestamp(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    let end = trimmed.find(']')?;
    let ts = &trimmed[1..end];
    if is_rfc3339_utc(ts) {
        return Some(ts);
    }
    None
}

fn is_rfc3339_utc(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 20 {
        return false;
    }
    matches_format(bytes)
}

fn matches_format(bytes: &[u8]) -> bool {
    is_digit(bytes[0])
        && is_digit(bytes[1])
        && is_digit(bytes[2])
        && is_digit(bytes[3])
        && bytes[4] == b'-'
        && is_digit(bytes[5])
        && is_digit(bytes[6])
        && bytes[7] == b'-'
        && is_digit(bytes[8])
        && is_digit(bytes[9])
        && bytes[10] == b'T'
        && is_digit(bytes[11])
        && is_digit(bytes[12])
        && bytes[13] == b':'
        && is_digit(bytes[14])
        && is_digit(bytes[15])
        && bytes[16] == b':'
        && is_digit(bytes[17])
        && is_digit(bytes[18])
        && bytes[19] == b'Z'
}

fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

fn loop_slug(name: &str) -> String {
    let lowered = name.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let mut prev_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            prev_dash = false;
            continue;
        }
        if (ch == ' ' || ch == '-' || ch == '_') && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

const HELP_TEXT: &str = "\
Tail loop logs.

Usage:
  forge logs [loop]

Flags:
  -f, --follow      follow log output
  -n, --lines N     number of lines to show (default 50)
      --since VAL   show logs since duration or timestamp
      --all         show logs for all loops in repo
      --compact     collapse thinking blocks and large code fences
      --raw         disable Claude stream-json rendering
      --no-color    disable colored log rendering
";

#[cfg(test)]
mod tests {
    use super::{
        default_log_path, render_lines_for_layer, render_log_chunk, run_for_test,
        split_complete_lines, InMemoryLogsBackend, LogRenderLayer, LoopRecord, RenderOptions,
    };
    use crate::diff_renderer::DiffRenderState;

    fn mixed_layer_lines() -> Vec<String> {
        vec![
            "OpenAI Codex v0.81.0".to_owned(),
            "model: gpt-5".to_owned(),
            "user".to_owned(),
            "tool: Bash(command=\"ls\")".to_owned(),
            "$ cargo test -q".to_owned(),
            "running 3 tests".to_owned(),
            "exit code: 1".to_owned(),
            "error: failed to compile".to_owned(),
            "  at src/main.rs:10:5".to_owned(),
            "diff --git a/src/main.rs b/src/main.rs".to_owned(),
            "@@ -1 +1 @@".to_owned(),
            "-old".to_owned(),
            "+new".to_owned(),
            "{\"type\":\"result\",\"num_turns\":1}".to_owned(),
        ]
    }

    #[test]
    fn render_lines_for_layer_events_filters_non_event_blocks() {
        let rendered = render_lines_for_layer(&mixed_layer_lines(), LogRenderLayer::Events, true);
        let text = rendered.join("\n");

        assert!(text.contains("OpenAI Codex v0.81.0"));
        assert!(text.contains("user"));
        assert!(text.contains("result"));
        assert!(!text.contains("tool: Bash"));
        assert!(!text.contains("cargo test"));
        assert!(!text.contains("error: failed to compile"));
        assert!(!text.contains("diff --git"));
    }

    #[test]
    fn render_lines_for_layer_errors_only_keeps_error_block() {
        let rendered = render_lines_for_layer(&mixed_layer_lines(), LogRenderLayer::Errors, true);
        let text = rendered.join("\n");

        assert!(text.contains("failed to compile"));
        assert!(text.contains("src/main.rs:10:5"));
        assert!(!text.contains("tool: Bash"));
        assert!(!text.contains("cargo test"));
        assert!(!text.contains("diff --git"));
    }

    #[test]
    fn render_lines_for_layer_tools_keeps_tool_and_command_transcript() {
        let rendered = render_lines_for_layer(&mixed_layer_lines(), LogRenderLayer::Tools, true);
        let text = rendered.join("\n");

        assert!(text.contains("tool: Bash"));
        assert!(text.contains("cargo test -q"));
        assert!(text.contains("exit code: 1"));
        assert!(!text.contains("failed to compile"));
        assert!(!text.contains("diff --git"));
    }

    #[test]
    fn render_lines_for_layer_diff_only_keeps_diff_lines() {
        let rendered = render_lines_for_layer(&mixed_layer_lines(), LogRenderLayer::Diff, true);
        let text = rendered.join("\n");

        assert!(text.contains("diff --git a/src/main.rs b/src/main.rs"));
        assert!(text.contains("@@ -1 +1 @@"));
        assert!(text.contains("-old"));
        assert!(text.contains("+new"));
        assert!(!text.contains("tool: Bash"));
        assert!(!text.contains("failed to compile"));
    }

    #[test]
    fn logs_requires_loop_or_all() {
        let mut backend = InMemoryLogsBackend::default();
        let out = run_for_test(&["logs"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "loop name required (or use --all)\n");
    }

    #[test]
    fn logs_tail_by_loop_name() {
        let alpha_path = "/tmp/forge/logs/loops/alpha.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo".to_string(),
            log_path: alpha_path.to_string(),
        }])
        .with_log(
            alpha_path,
            "[2026-01-01T00:00:00Z] one\n[2026-01-01T00:00:01Z] two\n[2026-01-01T00:00:02Z] three\n",
        );

        let out = run_for_test(
            &["logs", "alpha", "--lines", "2", "--no-color"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(
            out.stdout,
            "==> alpha <==\n[2026-01-01T00:00:01Z] two\n[2026-01-01T00:00:02Z] three\n"
        );
    }

    #[test]
    fn logs_tail_by_unique_short_id_prefix() {
        let alpha_path = "/tmp/forge/logs/loops/alpha.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo".to_string(),
            log_path: alpha_path.to_string(),
        }])
        .with_log(alpha_path, "[2026-01-01T00:00:01Z] one\n");

        let out = run_for_test(&["logs", "a"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("==> alpha <=="));
    }

    #[test]
    fn logs_rejects_ambiguous_short_id_prefix() {
        let alpha_path = "/tmp/forge/logs/loops/alpha.log";
        let beta_path = "/tmp/forge/logs/loops/beta.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![
            LoopRecord {
                id: "loop-001".to_string(),
                short_id: "ab123456".to_string(),
                name: "alpha".to_string(),
                repo: "/repo".to_string(),
                log_path: alpha_path.to_string(),
            },
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "ad123547".to_string(),
                name: "beta".to_string(),
                repo: "/repo".to_string(),
                log_path: beta_path.to_string(),
            },
        ])
        .with_log(alpha_path, "[2026-01-01T00:00:01Z] alpha\n")
        .with_log(beta_path, "[2026-01-01T00:00:01Z] beta\n");

        let out = run_for_test(&["logs", "a"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("ambiguous"));
        assert!(out.stderr.contains("alpha (ab123456)"));
        assert!(out.stderr.contains("beta (ad123547)"));

        let resolved = run_for_test(&["logs", "ab"], &mut backend);
        assert_eq!(resolved.exit_code, 0);
        assert!(resolved.stderr.is_empty());
        assert!(resolved.stdout.contains("==> alpha <=="));
    }

    #[test]
    fn logs_all_filters_by_repo() {
        let alpha_path = "/tmp/forge/logs/loops/alpha.log";
        let beta_path = "/tmp/forge/logs/loops/beta.log";
        let gamma_path = "/tmp/forge/logs/loops/gamma.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![
            LoopRecord {
                id: "loop-001".to_string(),
                short_id: "abc001".to_string(),
                name: "alpha".to_string(),
                repo: "/repo-a".to_string(),
                log_path: alpha_path.to_string(),
            },
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "def002".to_string(),
                name: "beta".to_string(),
                repo: "/repo-a".to_string(),
                log_path: beta_path.to_string(),
            },
            LoopRecord {
                id: "loop-003".to_string(),
                short_id: "ghi003".to_string(),
                name: "gamma".to_string(),
                repo: "/repo-b".to_string(),
                log_path: gamma_path.to_string(),
            },
        ])
        .with_repo_path("/repo-a")
        .with_log(alpha_path, "[2026-01-01T00:00:00Z] alpha\n")
        .with_log(beta_path, "[2026-01-01T00:00:00Z] beta\n")
        .with_log(gamma_path, "[2026-01-01T00:00:00Z] gamma\n");

        let out = run_for_test(&["logs", "--all"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("==> alpha <=="));
        assert!(out.stdout.contains("==> beta <=="));
        assert!(!out.stdout.contains("==> gamma <=="));
    }

    #[test]
    fn logs_since_rfc3339_filters_old_entries() {
        let path = "/tmp/forge/logs/loops/alpha.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo".to_string(),
            log_path: path.to_string(),
        }])
        .with_log(
            path,
            "[2026-01-01T00:00:00Z] old\n[2026-01-01T00:00:01Z] keep\n[2026-01-01T00:00:02Z] keep2\n",
        );
        let out = run_for_test(
            &[
                "logs",
                "alpha",
                "--since",
                "2026-01-01T00:00:01Z",
                "--no-color",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(
            out.stdout,
            "==> alpha <==\n[2026-01-01T00:00:01Z] keep\n[2026-01-01T00:00:02Z] keep2\n"
        );
    }

    #[test]
    fn logs_alias_log_is_supported() {
        let path = "/tmp/forge/logs/loops/alpha.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo".to_string(),
            log_path: path.to_string(),
        }])
        .with_log(path, "[2026-01-01T00:00:00Z] line\n");

        let out = run_for_test(&["log", "alpha"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("==> alpha <=="));
    }

    #[test]
    fn logs_follow_uses_backend_follow_path() {
        let path = "/tmp/forge/logs/loops/alpha.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo".to_string(),
            log_path: path.to_string(),
        }])
        .with_follow_output(path, "[2026-01-01T00:00:03Z] streaming\n");
        let out = run_for_test(&["logs", "alpha", "--follow", "--no-color"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(
            backend.followed_paths,
            vec![(path.to_string(), 50)],
            "follow should use default --lines=50"
        );
        assert_eq!(
            out.stdout,
            "==> alpha <==\n[2026-01-01T00:00:03Z] streaming\n"
        );
    }

    #[test]
    fn logs_unknown_flag_is_error() {
        let mut backend = InMemoryLogsBackend::default();
        let out = run_for_test(&["logs", "--bogus"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "error: unknown argument for logs: '--bogus'\n");
    }

    #[test]
    fn logs_formats_claude_stream_json_lines() {
        let path = "/tmp/forge/logs/loops/claude.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-claude".to_string(),
            short_id: "cc123456".to_string(),
            name: "claude-loop".to_string(),
            repo: "/repo".to_string(),
            log_path: path.to_string(),
        }])
        .with_log(
            path,
            "{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-opus-4-6\",\"tools\":[\"Bash\"],\"mcp_servers\":[],\"session_id\":\"sess-1\"}\n\
             {\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}}\n\
             {\"type\":\"result\",\"num_turns\":1,\"duration_ms\":2100,\"total_cost_usd\":0.120001,\"usage\":{\"input_tokens\":10,\"output_tokens\":20}}\n",
        );

        let out = run_for_test(&["logs", "claude-loop", "--no-color"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("[claude:init] model=claude-opus-4-6"));
        assert!(out.stdout.contains("hello"));
        assert!(out
            .stdout
            .contains("[claude:result] turns=1 duration=2.1s input=10 output=20"));
        assert!(!out.stdout.contains("{\"type\":\"system\""));
    }

    #[test]
    fn logs_raw_preserves_original_claude_stream_json() {
        let path = "/tmp/forge/logs/loops/claude-raw.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-claude-raw".to_string(),
            short_id: "cc654321".to_string(),
            name: "claude-raw-loop".to_string(),
            repo: "/repo".to_string(),
            log_path: path.to_string(),
        }])
        .with_log(
            path,
            "{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-opus-4-6\"}\n",
        );

        let out = run_for_test(&["logs", "claude-raw-loop", "--raw"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out
            .stdout
            .contains("{\"type\":\"system\",\"subtype\":\"init\""));
    }

    #[test]
    fn logs_formats_diff_patch_and_intraline_changes() {
        let path = "/tmp/forge/logs/loops/diff.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-diff".to_string(),
            short_id: "df123456".to_string(),
            name: "diff-loop".to_string(),
            repo: "/repo".to_string(),
            log_path: path.to_string(),
        }])
        .with_log(
            path,
            "diff --git a/src/main.rs b/src/main.rs\n\
             index 1111111..2222222 100644\n\
             --- a/src/main.rs\n\
             +++ b/src/main.rs\n\
             @@ -1,2 +1,2 @@\n\
             -let answer = 41;\n\
             +let answer = 42;\n",
        );

        let out = run_for_test(&["logs", "diff-loop", "--no-color"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out
            .stdout
            .contains("diff --git a/src/main.rs b/src/main.rs"));
        assert!(out.stdout.contains("@@ -1,2 +1,2 @@"));
        assert!(out.stdout.contains("-let answer = 4[-1-];"));
        assert!(out.stdout.contains("+let answer = 4{+2+};"));
    }

    #[test]
    fn default_log_path_matches_go_shape() {
        assert_eq!(
            default_log_path("/tmp/forge", "My Loop_Name", "loop-1"),
            "/tmp/forge/logs/loops/my-loop-name.log"
        );
        assert_eq!(
            default_log_path("/tmp/forge", " ", "loop-1"),
            "/tmp/forge/logs/loops/loop-1.log"
        );
    }

    #[test]
    fn split_complete_lines_handles_trailing_partial() {
        let (complete, rest) = split_complete_lines("one\ntwo\nthr");
        assert_eq!(complete, "one\ntwo\n");
        assert_eq!(rest, "thr");
    }

    #[test]
    fn split_complete_lines_handles_full_block() {
        let (complete, rest) = split_complete_lines("one\ntwo\n");
        assert_eq!(complete, "one\ntwo\n");
        assert_eq!(rest, "");
    }

    #[test]
    fn split_complete_lines_handles_no_newline() {
        let (complete, rest) = split_complete_lines("partial");
        assert_eq!(complete, "");
        assert_eq!(rest, "partial");
    }

    #[test]
    fn render_log_chunk_carries_diff_run_across_boundaries() {
        let mut diff_state = DiffRenderState::default();
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };

        let first = render_log_chunk("@@ -1 +1 @@\n-old\n", options, &mut diff_state);
        assert_eq!(first, "@@ -1 +1 @@");

        let second = render_log_chunk("+new\n", options, &mut diff_state);
        assert_eq!(second, "-[-old-]\n+{+new+}");
    }

    #[test]
    fn render_log_chunk_flushes_pending_diff_on_context_line() {
        let mut diff_state = DiffRenderState::default();
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };

        let _ = render_log_chunk("@@ -1 +1 @@\n-old\n", options, &mut diff_state);
        let second = render_log_chunk(" context\n", options, &mut diff_state);
        assert_eq!(second, "-old\n context");
    }

    // ── PAR-111: Readability layer tests ────────────────────────────

    #[test]
    fn readability_harness_header_emphasized_no_color() {
        use super::render_log_content;
        let content = "OpenAI Codex v0.80.0 (research preview)\n--------\nworkdir: /repo/forge\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        // Header should get "== " prefix in no-color mode.
        assert!(
            rendered.contains("== OpenAI Codex v0.80.0"),
            "header should be prefixed with == ; got: {rendered}"
        );
    }

    #[test]
    fn readability_harness_header_emphasized_color() {
        use super::{render_log_content, COLOR_BOLD, COLOR_CYAN, COLOR_RESET};
        let content = "OpenAI Codex v0.80.0 (research preview)\n";
        let options = RenderOptions {
            no_color: false,
            raw: false,
            compact: false,
        };
        // Force NO_COLOR unset for this test.
        std::env::remove_var("NO_COLOR");
        let rendered = render_log_content(content, options);
        assert!(
            rendered.contains(COLOR_BOLD) && rendered.contains(COLOR_CYAN),
            "header should be bold+cyan; got: {rendered}"
        );
        assert!(rendered.contains(COLOR_RESET));
    }

    #[test]
    fn readability_role_marker_emphasized_no_color() {
        use super::render_log_content;
        let content = "user\nHello world\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        assert!(
            rendered.contains(">> user"),
            "role marker should get >> prefix; got: {rendered}"
        );
    }

    #[test]
    fn readability_timestamp_dimmed_color() {
        use super::{render_log_content, COLOR_DIM, COLOR_RESET};
        let content = "[2026-02-09T16:00:01Z] status: idle\n";
        let options = RenderOptions {
            no_color: false,
            raw: false,
            compact: false,
        };
        std::env::remove_var("NO_COLOR");
        let rendered = render_log_content(content, options);
        assert!(
            rendered.contains(COLOR_DIM),
            "timestamp line should be dimmed; got: {rendered}"
        );
        assert!(rendered.contains(COLOR_RESET));
    }

    #[test]
    fn readability_section_separators_between_major_sections() {
        use super::render_log_content;
        let content = "user\nHello\n```rust\nfn main() {}\n```\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        // Should contain a separator (─ characters) between role marker and code fence.
        assert!(
            rendered.contains('\u{2500}'),
            "should contain separator between major sections; got: {rendered}"
        );
    }

    #[test]
    fn readability_compact_collapses_thinking_block() {
        use super::render_log_content;
        let content =
            "thinking\n**Planning approach**\nLine 2\nLine 3\nLine 4\nLine 5\ncodex\nDone\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: true,
        };
        let rendered = render_log_content(content, options);
        // The thinking block should show the first line + collapse summary.
        assert!(
            rendered.contains("thinking lines collapsed"),
            "compact mode should collapse thinking block; got: {rendered}"
        );
        // The individual continuation lines should NOT appear.
        assert!(
            !rendered.contains("Line 5"),
            "compact mode should hide continuation lines; got: {rendered}"
        );
    }

    #[test]
    fn readability_compact_collapses_code_fence() {
        use super::render_log_content;
        let content = "```rust\nfn one() {}\nfn two() {}\nfn three() {}\n```\nplain text\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: true,
        };
        let rendered = render_log_content(content, options);
        assert!(
            rendered.contains("code lines collapsed"),
            "compact mode should collapse code fence; got: {rendered}"
        );
    }

    #[test]
    fn readability_no_compact_shows_all_lines() {
        use super::render_log_content;
        let content = "thinking\n**Planning approach**\nLine 2\nLine 3\ncodex\nDone\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        assert!(
            rendered.contains("Line 2") && rendered.contains("Line 3"),
            "non-compact mode should show all lines; got: {rendered}"
        );
        assert!(
            !rendered.contains("collapsed"),
            "non-compact mode should not collapse; got: {rendered}"
        );
    }

    #[test]
    fn readability_tool_call_highlighted_color() {
        use super::{render_log_content, COLOR_RESET, COLOR_YELLOW};
        let content = "tool: read file `foo.rs`\n";
        let options = RenderOptions {
            no_color: false,
            raw: false,
            compact: false,
        };
        std::env::remove_var("NO_COLOR");
        let rendered = render_log_content(content, options);
        assert!(
            rendered.contains(COLOR_YELLOW),
            "tool call should be yellow; got: {rendered}"
        );
        assert!(rendered.contains(COLOR_RESET));
    }

    #[test]
    fn readability_compact_flag_parsed() {
        let mut backend = InMemoryLogsBackend::default();
        let out = run_for_test(&["logs", "--compact", "--bogus"], &mut backend);
        // --bogus should still fail; --compact alone shouldn't cause issues.
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--bogus"));
    }

    #[test]
    fn readability_raw_bypasses_section_styling() {
        use super::render_log_content;
        let content = "user\nHello\n";
        let options = RenderOptions {
            no_color: true,
            raw: true,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        // Raw mode should pass through without ">> " prefix.
        assert_eq!(rendered, content);
    }

    // ── PAR-107: Command transcript renderer tests ──────────────────

    #[test]
    fn command_prompt_highlighted_color() {
        use super::{render_log_content, COLOR_RESET};
        let content = "exec\n$ cargo test --lib\nrunning 8 tests\nexit code: 0\n";
        let options = RenderOptions {
            no_color: false,
            raw: false,
            compact: false,
        };
        std::env::remove_var("NO_COLOR");
        let rendered = render_log_content(content, options);
        // Prompt line should be styled with ANSI escape sequences.
        assert!(
            rendered.contains("\x1b["),
            "command prompt should be styled; got: {rendered}"
        );
        assert!(rendered.contains(COLOR_RESET));
    }

    #[test]
    fn command_prompt_highlighted_no_color() {
        use super::render_log_content;
        let content = "exec\n$ cargo test --lib\nrunning 8 tests\nexit code: 0\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        // In no-color mode, command prompt gets "$ " signifier.
        assert!(
            rendered.contains("$ $ cargo test --lib"),
            "command prompt should get $ prefix in no-color; got: {rendered}"
        );
    }

    #[test]
    fn command_exit_code_zero_not_error_styled() {
        use super::render_log_content;
        let content = "exec\n$ cargo test\nexit code: 0\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        // Exit code 0 should NOT get [ERROR] prefix.
        assert!(
            !rendered.contains("[ERROR] exit code: 0"),
            "exit code 0 should not be error-styled; got: {rendered}"
        );
    }

    #[test]
    fn command_exit_code_nonzero_error_styled() {
        use super::render_log_content;
        let content = "exec\n$ cargo test\nexit code: 1\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        // Exit code 1 should get [ERROR] prefix in no-color mode.
        assert!(
            rendered.contains("[ERROR] exit code: 1"),
            "exit code 1 should be error-styled; got: {rendered}"
        );
    }

    #[test]
    fn command_exit_code_nonzero_bold_red_in_color() {
        use super::render_log_content;
        let content = "exec\n$ cargo test\nexit code: 1\n";
        let options = RenderOptions {
            no_color: false,
            raw: false,
            compact: false,
        };
        std::env::remove_var("NO_COLOR");
        let rendered = render_log_content(content, options);
        assert!(
            rendered.contains("\x1b[1;31m"),
            "exit code 1 should be bold red; got: {rendered}"
        );
    }

    #[test]
    fn command_stdout_preserved_plain() {
        use super::render_log_content;
        let content =
            "exec\n$ cargo test\nrunning 8 tests\ntest pool::tests::acquire ... ok\nexit code: 0\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        // Stdout lines should be preserved as-is.
        assert!(
            rendered.contains("running 8 tests"),
            "stdout should be preserved; got: {rendered}"
        );
        assert!(
            rendered.contains("test pool::tests::acquire ... ok"),
            "stdout should be preserved; got: {rendered}"
        );
    }

    #[test]
    fn command_stderr_highlighted() {
        use super::render_log_content;
        let content = "exec\n$ cargo test\nstderr: thread panicked\nexit code: 101\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        // Stderr lines should get [WARN] prefix in no-color mode.
        assert!(
            rendered.contains("[WARN] stderr: thread panicked"),
            "stderr should be warning-styled; got: {rendered}"
        );
    }

    #[test]
    fn command_known_commands_detected() {
        use super::render_log_content;
        // Test multiple known commands.
        let content = "exec\n$ git status\nexit code: 0\n$ sv task list\nexit code: 0\n$ forge logs alpha\nexit code: 0\n";
        let options = RenderOptions {
            no_color: false,
            raw: false,
            compact: false,
        };
        std::env::remove_var("NO_COLOR");
        let rendered = render_log_content(content, options);
        // All prompt lines should be styled.
        assert!(
            rendered.contains("\x1b["),
            "known commands should be styled; got: {rendered}"
        );
    }

    #[test]
    fn command_raw_bypasses_styling() {
        use super::render_log_content;
        let content = "$ cargo test\nrunning 8 tests\nexit code: 0\n";
        let options = RenderOptions {
            no_color: true,
            raw: true,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        // Raw mode should pass through without any modification.
        assert_eq!(rendered, content);
    }

    #[test]
    fn command_plain_mode_preserves_ambiguous_lines() {
        use super::render_log_content;
        // Lines that don't look like commands should remain plain.
        let content = "Just some text\nMore text\n";
        let options = RenderOptions {
            no_color: true,
            raw: false,
            compact: false,
        };
        let rendered = render_log_content(content, options);
        assert!(
            rendered.contains("Just some text"),
            "plain lines should be preserved; got: {rendered}"
        );
    }
}
