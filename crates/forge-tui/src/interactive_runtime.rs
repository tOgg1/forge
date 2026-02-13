use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, Event as TerminalEvent, KeyCode as TerminalKeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::style::{
    Attribute, Color, Print, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};
use forge_cli::{kill, logs, resume, rm, stop, up};
use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers, ResizeEvent};
use forge_ftui_adapter::render::{CellStyle, RenderFrame};
use forge_tui::app::{
    ActionKind, ActionResult, ActionType, App, Command, LogLayer, LogSource, LogTailView, RunView,
};
use forge_tui::theme::detect_terminal_color_capability;

const REFRESH_INTERVAL: Duration = Duration::from_millis(900);
const FOLLOW_REFRESH_INTERVAL: Duration = Duration::from_millis(400);
const LIVE_LOG_LINE_LIMIT: usize = 1200;
const MULTI_LOG_LINE_LIMIT: usize = 48;
const MULTI_LOG_LOOP_LIMIT: usize = 12;

pub fn run() -> Result<(), String> {
    let mut terminal_session =
        TerminalSession::enter().map_err(|err| format!("enter tui terminal mode: {err}"))?;
    let db_path = super::resolve_database_path();
    let capability = detect_terminal_color_capability();
    let palette_name = std::env::var("FORGE_TUI_PALETTE").unwrap_or_else(|_| "default".to_owned());
    let mut app = App::new_with_capability(&palette_name, capability, 200);
    let mut backend = RuntimeBackend::new(db_path);

    let (width, height) = terminal_size().map_err(|err| format!("read terminal size: {err}"))?;
    let _ = app.update(InputEvent::Resize(ResizeEvent { width, height }));
    backend.refresh(&mut app)?;

    let mut dirty = true;
    let mut next_refresh = Instant::now() + REFRESH_INTERVAL;

    loop {
        if dirty {
            let frame = app.render();
            render_frame(&mut terminal_session.stdout, &frame)
                .map_err(|err| format!("render frame: {err}"))?;
            dirty = false;
        }

        if app.quitting() {
            break;
        }

        let now = Instant::now();
        if now >= next_refresh {
            let refreshed = dispatch_command(Command::Fetch, &mut app, &mut backend)?;
            dirty |= refreshed;
            let interval = if app.follow_mode() {
                FOLLOW_REFRESH_INTERVAL
            } else {
                REFRESH_INTERVAL
            };
            next_refresh = Instant::now() + interval;
            continue;
        }

        let timeout = next_refresh.saturating_duration_since(now);
        let has_event =
            event::poll(timeout).map_err(|err| format!("poll terminal event: {err}"))?;
        if !has_event {
            continue;
        }

        let event = event::read().map_err(|err| format!("read terminal event: {err}"))?;
        if is_interrupt(&event) {
            break;
        }

        if let Some(input) = map_terminal_event(event) {
            let command = app.update(input);
            dirty = true;
            let changed = dispatch_command(command, &mut app, &mut backend)?;
            dirty |= changed;
        }
    }

    Ok(())
}

struct RuntimeBackend {
    db_path: PathBuf,
    spawn_owner: String,
}

impl RuntimeBackend {
    fn new(db_path: PathBuf) -> Self {
        let spawn_owner =
            std::env::var("FORGE_TUI_SPAWN_OWNER").unwrap_or_else(|_| "auto".to_owned());
        Self {
            db_path,
            spawn_owner,
        }
    }

    fn refresh(&mut self, app: &mut App) -> Result<(), String> {
        let snapshot = super::load_live_loop_snapshot(&self.db_path)?;
        app.set_loops(snapshot.loops.clone());

        let selected_loop_id = app.selected_id().to_owned();
        let run_bundle = load_run_bundle(&self.db_path, &selected_loop_id)?;
        app.set_run_history(run_bundle.views.clone());

        let selected_log = self.load_selected_log(
            app,
            &selected_loop_id,
            &snapshot.log_paths,
            &run_bundle.output_tails,
        );
        app.set_selected_log(selected_log);
        app.set_multi_logs(self.load_multi_logs(app.loops(), &snapshot.log_paths));
        Ok(())
    }

    fn load_selected_log(
        &self,
        app: &App,
        loop_id: &str,
        log_paths: &HashMap<String, String>,
        run_output_tails: &[Vec<String>],
    ) -> LogTailView {
        if loop_id.trim().is_empty() {
            return LogTailView {
                lines: Vec::new(),
                message: "No loop selected".to_owned(),
            };
        }

        if app.log_source() != LogSource::Live {
            let selected_index = if app.log_source() == LogSource::RunSelection {
                app.selected_run_view().and_then(|selected| {
                    app.run_history()
                        .iter()
                        .position(|candidate| candidate.id == selected.id)
                })
            } else {
                Some(0)
            };
            if let Some(index) = selected_index {
                if let Some(lines) = run_output_tails.get(index) {
                    if !lines.is_empty() {
                        return LogTailView {
                            lines: lines.clone(),
                            message: String::new(),
                        };
                    }
                }
            }
        }

        let Some(path) = log_paths.get(loop_id) else {
            return LogTailView {
                lines: Vec::new(),
                message: "No live log file for selected loop".to_owned(),
            };
        };

        match self.load_live_log_lines(path, app.log_layer()) {
            Ok(lines) if !lines.is_empty() => LogTailView {
                lines,
                message: String::new(),
            },
            Ok(_) => LogTailView {
                lines: Vec::new(),
                message: "No log content yet".to_owned(),
            },
            Err(err) => LogTailView {
                lines: Vec::new(),
                message: err,
            },
        }
    }

    fn load_multi_logs(
        &self,
        loops: &[forge_tui::app::LoopView],
        log_paths: &HashMap<String, String>,
    ) -> HashMap<String, LogTailView> {
        let mut out = HashMap::new();
        for loop_view in loops.iter().take(MULTI_LOG_LOOP_LIMIT) {
            let log_view = match log_paths.get(&loop_view.id) {
                Some(path) => match read_log_tail(path, MULTI_LOG_LINE_LIMIT) {
                    Ok(lines) if !lines.is_empty() => LogTailView {
                        lines,
                        message: String::new(),
                    },
                    Ok(_) => LogTailView {
                        lines: Vec::new(),
                        message: "No log content yet".to_owned(),
                    },
                    Err(err) => LogTailView {
                        lines: Vec::new(),
                        message: err,
                    },
                },
                None => LogTailView {
                    lines: Vec::new(),
                    message: "No live log file".to_owned(),
                },
            };
            out.insert(loop_view.id.clone(), log_view);
        }
        out
    }

    fn load_live_log_lines(&self, path: &str, layer: LogLayer) -> Result<Vec<String>, String> {
        let raw_lines = read_log_tail(path, LIVE_LOG_LINE_LIMIT)?;
        if raw_lines.is_empty() {
            return Ok(Vec::new());
        }
        Ok(logs::render_lines_for_layer(
            &raw_lines,
            map_log_render_layer(layer),
            true,
        ))
    }

    fn execute_action(&mut self, action: ActionKind) -> ActionResult {
        match action {
            ActionKind::Resume { loop_id } => {
                let mut backend = resume::SqliteResumeBackend::open_from_env();
                let mut args = vec!["resume".to_owned(), loop_id.clone(), "--json".to_owned()];
                if self.spawn_owner != "auto" {
                    args.push("--spawn-owner".to_owned());
                    args.push(self.spawn_owner.clone());
                }
                let (exit_code, stdout, stderr) = run_resume(&args, &mut backend);
                if exit_code == 0 {
                    let message = parse_resume_success_message(&stdout, &loop_id)
                        .unwrap_or_else(|| format!("Loop {loop_id} resumed"));
                    ActionResult {
                        kind: ActionType::Resume,
                        loop_id,
                        selected_loop_id: String::new(),
                        message,
                        error: None,
                    }
                } else {
                    ActionResult {
                        kind: ActionType::Resume,
                        loop_id,
                        selected_loop_id: String::new(),
                        message: String::new(),
                        error: Some(error_from_stderr("resume loop", &stderr)),
                    }
                }
            }
            ActionKind::Stop { loop_id } => {
                let mut backend = stop::SqliteStopBackend::open_from_env();
                let args = vec!["stop".to_owned(), loop_id.clone(), "--json".to_owned()];
                let (exit_code, _stdout, stderr) = run_stop(&args, &mut backend);
                if exit_code == 0 {
                    ActionResult {
                        kind: ActionType::Stop,
                        loop_id: loop_id.clone(),
                        selected_loop_id: String::new(),
                        message: format!("Stop queued for loop {loop_id}"),
                        error: None,
                    }
                } else {
                    ActionResult {
                        kind: ActionType::Stop,
                        loop_id: loop_id.clone(),
                        selected_loop_id: String::new(),
                        message: String::new(),
                        error: Some(error_from_stderr("stop loop", &stderr)),
                    }
                }
            }
            ActionKind::Kill { loop_id } => {
                let mut backend = kill::SqliteKillBackend::open_from_env();
                let args = vec!["kill".to_owned(), loop_id.clone(), "--json".to_owned()];
                let (exit_code, _stdout, stderr) = run_kill(&args, &mut backend);
                if exit_code == 0 {
                    ActionResult {
                        kind: ActionType::Kill,
                        loop_id: loop_id.clone(),
                        selected_loop_id: String::new(),
                        message: format!("Kill queued for loop {loop_id}"),
                        error: None,
                    }
                } else {
                    ActionResult {
                        kind: ActionType::Kill,
                        loop_id: loop_id.clone(),
                        selected_loop_id: String::new(),
                        message: String::new(),
                        error: Some(error_from_stderr("kill loop", &stderr)),
                    }
                }
            }
            ActionKind::Delete { loop_id, force } => {
                let mut backend = rm::SqliteLoopBackend::open_from_env();
                let mut args = vec!["rm".to_owned(), loop_id.clone(), "--json".to_owned()];
                if force {
                    args.push("--force".to_owned());
                }
                let (exit_code, _stdout, stderr) = run_rm(&args, &mut backend);
                if exit_code == 0 {
                    ActionResult {
                        kind: ActionType::Delete,
                        loop_id: loop_id.clone(),
                        selected_loop_id: String::new(),
                        message: format!("Removed loop {loop_id}"),
                        error: None,
                    }
                } else {
                    ActionResult {
                        kind: ActionType::Delete,
                        loop_id: loop_id.clone(),
                        selected_loop_id: String::new(),
                        message: String::new(),
                        error: Some(error_from_stderr("remove loop", &stderr)),
                    }
                }
            }
            ActionKind::Create { wizard } => {
                let mut backend = up::SqliteUpBackend::open_from_env();
                let args = build_up_args(&wizard, &self.spawn_owner);
                let (exit_code, stdout, stderr) = run_up(&args, &mut backend);
                if exit_code == 0 {
                    let selected_loop_id =
                        parse_created_loop_id(&self.db_path, &stdout).unwrap_or_default();
                    ActionResult {
                        kind: ActionType::Create,
                        loop_id: String::new(),
                        selected_loop_id,
                        message: create_success_message(&stdout),
                        error: None,
                    }
                } else {
                    ActionResult {
                        kind: ActionType::Create,
                        loop_id: String::new(),
                        selected_loop_id: String::new(),
                        message: String::new(),
                        error: Some(error_from_stderr("create loop", &stderr)),
                    }
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct RunBundle {
    views: Vec<RunView>,
    output_tails: Vec<Vec<String>>,
}

fn load_run_bundle(db_path: &PathBuf, loop_id: &str) -> Result<RunBundle, String> {
    if loop_id.trim().is_empty() || !db_path.exists() {
        return Ok(RunBundle::default());
    }

    let db = forge_db::Db::open(forge_db::Config::new(db_path))
        .map_err(|err| format!("open database {}: {err}", db_path.display()))?;
    let run_repo = forge_db::loop_run_repository::LoopRunRepository::new(&db);
    let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);

    let runs = match run_repo.list_by_loop(loop_id) {
        Ok(value) => value,
        Err(err) if super::is_missing_table(&err, "loop_runs") => Vec::new(),
        Err(err) => return Err(err.to_string()),
    };

    let profiles = match profile_repo.list() {
        Ok(items) => items
            .into_iter()
            .map(|profile| {
                (
                    profile.id,
                    (profile.name, profile.harness, profile.auth_kind),
                )
            })
            .collect::<HashMap<_, _>>(),
        Err(err) if super::is_missing_table(&err, "profiles") => HashMap::new(),
        Err(err) => return Err(err.to_string()),
    };

    let mut bundle = RunBundle::default();
    for run in runs {
        let (profile_name, harness, auth_kind) = match profiles.get(&run.profile_id) {
            Some(value) => value.clone(),
            None => (run.profile_id.clone(), String::new(), String::new()),
        };
        let status = run.status.as_str().to_owned();
        let duration = if status == "running" {
            "running".to_owned()
        } else {
            "-".to_owned()
        };

        bundle.views.push(RunView {
            id: run.id.clone(),
            status,
            exit_code: run.exit_code,
            duration,
            profile_name,
            harness,
            auth_kind,
        });
        bundle
            .output_tails
            .push(split_log_lines(&run.output_tail, LIVE_LOG_LINE_LIMIT));
    }

    Ok(bundle)
}

fn dispatch_command(
    cmd: Command,
    app: &mut App,
    backend: &mut RuntimeBackend,
) -> Result<bool, String> {
    match cmd {
        Command::None | Command::Quit => Ok(false),
        Command::Fetch => {
            backend.refresh(app)?;
            Ok(true)
        }
        Command::Batch(commands) => {
            let mut dirty = false;
            for child in commands {
                dirty |= dispatch_command(child, app, backend)?;
            }
            Ok(dirty)
        }
        Command::RunAction(action) => {
            let result = backend.execute_action(action);
            let follow_up = app.handle_action_result(result);
            let _ = dispatch_command(follow_up, app, backend)?;
            Ok(true)
        }
    }
}

fn terminal_size() -> io::Result<(usize, usize)> {
    let (width, height) = terminal::size()?;
    Ok((usize::from(width), usize::from(height)))
}

fn map_terminal_event(event: TerminalEvent) -> Option<InputEvent> {
    match event {
        TerminalEvent::Resize(width, height) => Some(InputEvent::Resize(ResizeEvent {
            width: usize::from(width),
            height: usize::from(height),
        })),
        TerminalEvent::Key(key_event) => {
            if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                return None;
            }

            let key = match key_event.code {
                TerminalKeyCode::Char(ch) => Key::Char(ch),
                TerminalKeyCode::Enter => Key::Enter,
                TerminalKeyCode::Esc => Key::Escape,
                TerminalKeyCode::Tab => Key::Tab,
                TerminalKeyCode::BackTab => Key::Tab,
                TerminalKeyCode::Backspace => Key::Backspace,
                TerminalKeyCode::Up => Key::Up,
                TerminalKeyCode::Down => Key::Down,
                TerminalKeyCode::Left => Key::Left,
                TerminalKeyCode::Right => Key::Right,
                _ => return None,
            };

            let mut modifiers = Modifiers {
                shift: key_event.modifiers.contains(KeyModifiers::SHIFT),
                ctrl: key_event.modifiers.contains(KeyModifiers::CONTROL),
                alt: key_event.modifiers.contains(KeyModifiers::ALT),
            };
            if matches!(key_event.code, TerminalKeyCode::BackTab) {
                modifiers.shift = true;
            }

            Some(InputEvent::Key(KeyEvent { key, modifiers }))
        }
        _ => None,
    }
}

fn is_interrupt(event: &TerminalEvent) -> bool {
    let TerminalEvent::Key(key_event) = event else {
        return false;
    };

    if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return false;
    }

    matches!(key_event.code, TerminalKeyCode::Char('c'))
        && key_event.modifiers.contains(KeyModifiers::CONTROL)
}

fn render_frame<W: Write>(out: &mut W, frame: &RenderFrame) -> io::Result<()> {
    queue!(out, MoveTo(0, 0), Clear(ClearType::All))?;
    let size = frame.size();

    for y in 0..size.height {
        queue!(out, MoveTo(0, to_u16(y)))?;
        let mut style = None;
        for x in 0..size.width {
            if let Some(cell) = frame.cell(x, y) {
                if style != Some(cell.style) {
                    queue_style(out, cell.style)?;
                    style = Some(cell.style);
                }
                queue!(out, Print(cell.glyph))?;
            }
        }
    }

    queue!(
        out,
        SetAttribute(Attribute::Reset),
        MoveTo(0, to_u16(size.height))
    )?;
    out.flush()
}

fn term_color_to_crossterm(tc: forge_ftui_adapter::render::TermColor) -> Color {
    match tc {
        forge_ftui_adapter::render::TermColor::Ansi256(idx) => Color::AnsiValue(idx),
        forge_ftui_adapter::render::TermColor::Rgb(r, g, b) => Color::Rgb { r, g, b },
    }
}

fn queue_style<W: Write>(out: &mut W, style: CellStyle) -> io::Result<()> {
    queue!(
        out,
        SetAttribute(Attribute::Reset),
        SetForegroundColor(term_color_to_crossterm(style.fg)),
        SetBackgroundColor(term_color_to_crossterm(style.bg)),
    )?;
    if style.bold {
        queue!(out, SetAttribute(Attribute::Bold))?;
    } else if style.dim {
        queue!(out, SetAttribute(Attribute::Dim))?;
    } else {
        queue!(out, SetAttribute(Attribute::NormalIntensity))?;
    }
    if style.underline {
        queue!(out, SetAttribute(Attribute::Underlined))?;
    } else {
        queue!(out, SetAttribute(Attribute::NoUnderline))?;
    }
    Ok(())
}

fn to_u16(value: usize) -> u16 {
    value.min(usize::from(u16::MAX)) as u16
}

fn map_log_render_layer(layer: LogLayer) -> logs::LogRenderLayer {
    match layer {
        LogLayer::Raw => logs::LogRenderLayer::Raw,
        LogLayer::Events => logs::LogRenderLayer::Events,
        LogLayer::Errors => logs::LogRenderLayer::Errors,
        LogLayer::Tools => logs::LogRenderLayer::Tools,
        LogLayer::Diff => logs::LogRenderLayer::Diff,
    }
}

fn read_log_tail(path: &str, limit: usize) -> Result<Vec<String>, String> {
    let content = std::fs::read_to_string(path).map_err(|err| format!("open {path}: {err}"))?;
    Ok(split_log_lines(&content, limit))
}

fn split_log_lines(content: &str, limit: usize) -> Vec<String> {
    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    if lines.len() > limit {
        let start = lines.len().saturating_sub(limit);
        lines = lines.split_off(start);
    }
    lines
}

fn run_resume(args: &[String], backend: &mut resume::SqliteResumeBackend) -> (i32, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = resume::run_with_backend(args, backend, &mut stdout, &mut stderr);
    (
        exit_code,
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

fn run_stop(args: &[String], backend: &mut stop::SqliteStopBackend) -> (i32, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = stop::run_with_backend(args, backend, &mut stdout, &mut stderr);
    (
        exit_code,
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

fn run_kill(args: &[String], backend: &mut kill::SqliteKillBackend) -> (i32, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = kill::run_with_backend(args, backend, &mut stdout, &mut stderr);
    (
        exit_code,
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

fn run_rm(args: &[String], backend: &mut rm::SqliteLoopBackend) -> (i32, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = rm::run_with_backend(args, backend, &mut stdout, &mut stderr);
    (
        exit_code,
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

fn run_up(args: &[String], backend: &mut up::SqliteUpBackend) -> (i32, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = up::run_with_backend(args, backend, &mut stdout, &mut stderr);
    (
        exit_code,
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

fn build_up_args(wizard: &[(String, String)], spawn_owner: &str) -> Vec<String> {
    let mut args = vec!["up".to_owned(), "--json".to_owned()];
    if spawn_owner != "auto" {
        args.push("--spawn-owner".to_owned());
        args.push(spawn_owner.to_owned());
    }

    push_non_empty_pair(&mut args, "--name", wizard_value(wizard, "name"));
    push_non_empty_pair(
        &mut args,
        "--name-prefix",
        wizard_value(wizard, "name_prefix"),
    );
    push_non_empty_pair(&mut args, "--count", wizard_value(wizard, "count"));
    push_non_empty_pair(&mut args, "--pool", wizard_value(wizard, "pool"));
    push_non_empty_pair(&mut args, "--profile", wizard_value(wizard, "profile"));
    push_non_empty_pair(&mut args, "--prompt", wizard_value(wizard, "prompt"));
    push_non_empty_pair(
        &mut args,
        "--prompt-msg",
        wizard_value(wizard, "prompt_msg"),
    );
    push_non_empty_pair(&mut args, "--interval", wizard_value(wizard, "interval"));
    push_non_empty_pair(
        &mut args,
        "--max-runtime",
        wizard_value(wizard, "max_runtime"),
    );
    push_non_empty_pair(
        &mut args,
        "--max-iterations",
        wizard_value(wizard, "max_iterations"),
    );
    push_non_empty_pair(&mut args, "--tags", wizard_value(wizard, "tags"));
    args
}

fn wizard_value<'a>(wizard: &'a [(String, String)], key: &str) -> Option<&'a String> {
    wizard
        .iter()
        .find(|(entry_key, _)| entry_key == key)
        .map(|(_, value)| value)
}

fn push_non_empty_pair(args: &mut Vec<String>, flag: &str, value: Option<&String>) {
    let Some(raw) = value else {
        return;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    args.push(flag.to_owned());
    args.push(trimmed.to_owned());
}

fn parse_resume_success_message(stdout: &str, loop_id: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(stdout).ok()?;
    let resumed = value.get("resumed")?.as_bool()?;
    if !resumed {
        return None;
    }
    let name = value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(loop_id);
    let id = value
        .get("loop_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(loop_id);
    Some(format!("Loop {name} resumed ({id})"))
}

fn parse_created_loop_id(db_path: &PathBuf, stdout: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(stdout).ok()?;
    let entries = value.as_array()?;
    let first = entries.first()?;
    let name = first.get("name")?.as_str()?.to_owned();

    if !db_path.exists() {
        return None;
    }

    let db = forge_db::Db::open(forge_db::Config::new(db_path)).ok()?;
    let repo = forge_db::loop_repository::LoopRepository::new(&db);
    let mut loops = repo.list().ok()?;
    loops.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    loops
        .into_iter()
        .find(|loop_entry| loop_entry.name == name)
        .map(|loop_entry| loop_entry.id)
}

fn create_success_message(stdout: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout) else {
        return "Loop created".to_owned();
    };
    let Some(entries) = value.as_array() else {
        return "Loop created".to_owned();
    };
    match entries.len() {
        0 => "Loop created".to_owned(),
        1 => {
            let name = entries
                .first()
                .and_then(|entry| entry.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("loop");
            format!("Loop {name} created")
        }
        count => format!("Created {count} loops"),
    }
}

fn error_from_stderr(action: &str, stderr: &str) -> String {
    let message = stderr.trim();
    if message.is_empty() {
        return format!("{action} failed");
    }
    format!("{action} failed: {message}")
}

struct TerminalSession {
    stdout: io::Stdout,
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            Hide,
            Clear(ClearType::All),
            MoveTo(0, 0)
        )?;
        Ok(Self { stdout })
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = execute!(
            self.stdout,
            SetAttribute(Attribute::Reset),
            LeaveAlternateScreen,
            Show,
            MoveTo(0, 0)
        );
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::{build_up_args, split_log_lines};

    #[test]
    fn build_up_args_maps_wizard_values() {
        let wizard = vec![
            ("name".to_owned(), "alpha".to_owned()),
            ("count".to_owned(), "2".to_owned()),
            ("profile".to_owned(), "dev-profile".to_owned()),
            ("prompt_msg".to_owned(), "ship it".to_owned()),
            ("tags".to_owned(), "rust,parity".to_owned()),
        ];

        let args = build_up_args(&wizard, "daemon");

        assert!(args.iter().any(|arg| arg == "--spawn-owner"));
        assert!(args.iter().any(|arg| arg == "daemon"));
        assert!(args.iter().any(|arg| arg == "--name"));
        assert!(args.iter().any(|arg| arg == "alpha"));
        assert!(args.iter().any(|arg| arg == "--count"));
        assert!(args.iter().any(|arg| arg == "2"));
        assert!(args.iter().any(|arg| arg == "--prompt-msg"));
        assert!(args.iter().any(|arg| arg == "ship it"));
    }

    #[test]
    fn split_log_lines_keeps_tail() {
        let content = "l1\nl2\nl3\nl4\n";
        let lines = split_log_lines(content, 2);
        assert_eq!(lines, vec!["l3".to_owned(), "l4".to_owned()]);
    }
}
