use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forge_ftui_adapter::input::{
    translate_input, InputEvent, Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind,
    MouseWheelDirection, ResizeEvent, UiAction,
};
use forge_ftui_adapter::upstream_ftui as ftui;
use ftui::core::event::{
    Event, KeyCode as FtuiKeyCode, KeyEvent as FtuiKeyEvent, KeyEventKind as FtuiKeyEventKind,
    Modifiers as FtuiModifiers, MouseEvent as FtuiMouseEvent, MouseEventKind as FtuiMouseEventKind,
};
use ftui::render::cell::Cell;
use ftui::render::drawing::Draw;
use ftui::runtime::{Every, Subscription};
use ftui::{App as FtuiApp, Cmd, Frame, Model, ScreenMode};

const REFRESH_INTERVAL_MS: u64 = 900;
const INLINE_UI_HEIGHT: u16 = 10;
const INLINE_AUTO_MIN_HEIGHT: u16 = 6;
const INLINE_AUTO_MAX_HEIGHT: u16 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEvent {
    Input(InputEvent),
    Tick,
    Quit,
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeShellMsg {
    Runtime(RuntimeEvent),
    SnapshotLoaded(BootstrapSnapshot),
}

impl From<Event> for ForgeShellMsg {
    fn from(event: Event) -> Self {
        Self::Runtime(translate_runtime_event(&event))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapSnapshot {
    pub loop_count: usize,
    pub refreshed_at_epoch_secs: u64,
    pub error: Option<String>,
}

impl BootstrapSnapshot {
    fn ok(loop_count: usize) -> Self {
        Self {
            loop_count,
            refreshed_at_epoch_secs: unix_timestamp_secs(),
            error: None,
        }
    }

    fn err(message: String) -> Self {
        Self {
            loop_count: 0,
            refreshed_at_epoch_secs: unix_timestamp_secs(),
            error: Some(message),
        }
    }
}

pub struct ForgeShell {
    db_path: PathBuf,
    loop_count: usize,
    refresh_count: usize,
    last_action: UiAction,
    last_event: RuntimeEvent,
    last_error: Option<String>,
    last_refreshed_at_epoch_secs: u64,
}

impl ForgeShell {
    #[must_use]
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            loop_count: 0,
            refresh_count: 0,
            last_action: UiAction::Noop,
            last_event: RuntimeEvent::Ignore,
            last_error: None,
            last_refreshed_at_epoch_secs: 0,
        }
    }

    fn apply_snapshot(&mut self, snapshot: BootstrapSnapshot) {
        self.loop_count = snapshot.loop_count;
        self.last_refreshed_at_epoch_secs = snapshot.refreshed_at_epoch_secs;
        self.last_error = snapshot.error;
        self.refresh_count = self.refresh_count.saturating_add(1);
    }

    fn perform_refresh(&self, task_name: &'static str) -> Cmd<ForgeShellMsg> {
        perform_refresh(task_name, self.db_path.clone())
    }
}

impl Model for ForgeShell {
    type Message = ForgeShellMsg;

    fn init(&mut self) -> Cmd<Self::Message> {
        self.perform_refresh("forge-shell-init-refresh")
    }

    fn update(&mut self, msg: Self::Message) -> Cmd<Self::Message> {
        match msg {
            ForgeShellMsg::Runtime(runtime_event) => {
                self.last_event = runtime_event;
                match runtime_event {
                    RuntimeEvent::Input(input) => {
                        let action = translate_input(&input);
                        self.last_action = action;
                        if action == UiAction::Refresh {
                            self.perform_refresh("forge-shell-input-refresh")
                        } else {
                            Cmd::none()
                        }
                    }
                    RuntimeEvent::Tick => self.perform_refresh("forge-shell-tick-refresh"),
                    RuntimeEvent::Quit => Cmd::quit(),
                    RuntimeEvent::Ignore => Cmd::none(),
                }
            }
            ForgeShellMsg::SnapshotLoaded(snapshot) => {
                self.apply_snapshot(snapshot);
                Cmd::none()
            }
        }
    }

    fn view(&self, frame: &mut Frame) {
        let lines = [
            "Forge TUI | FrankenTUI bootstrap".to_owned(),
            format!("db: {}", self.db_path.display()),
            format!("loops: {}", self.loop_count),
            format!("refresh-count: {}", self.refresh_count),
            format!("last-event: {:?}", self.last_event),
            format!("last-action: {:?}", self.last_action),
            format!("last-refresh-epoch: {}", self.last_refreshed_at_epoch_secs),
            format!(
                "last-error: {}",
                self.last_error.as_deref().unwrap_or("none")
            ),
            "keys: q/ctrl+c quit | r/ctrl+r refresh".to_owned(),
        ];

        let base_cell = Cell::from_char(' ');
        let max_rows = usize::from(frame.height());
        for (idx, line) in lines.iter().enumerate().take(max_rows) {
            frame.print_text(0, idx as u16, line, base_cell);
        }
    }

    fn subscriptions(&self) -> Vec<Box<dyn Subscription<Self::Message>>> {
        vec![Box::new(Every::new(
            Duration::from_millis(REFRESH_INTERVAL_MS),
            || ForgeShellMsg::Runtime(RuntimeEvent::Tick),
        ))]
    }
}

pub fn run(db_path: PathBuf) -> Result<(), String> {
    FtuiApp::new(ForgeShell::new(db_path))
        .screen_mode(resolve_screen_mode_from_env())
        .run()
        .map_err(|err| format!("run frankentui bootstrap runtime: {err}"))
}

#[must_use]
pub fn resolve_screen_mode_from_env() -> ScreenMode {
    let mode = std::env::var("FORGE_TUI_SCREEN_MODE")
        .map(|raw| raw.trim().to_ascii_lowercase())
        .unwrap_or_else(|_| "inline".to_owned());

    match mode.as_str() {
        "altscreen" | "alt" | "fullscreen" => ScreenMode::AltScreen,
        "inline-auto" | "inline_auto" | "auto" => {
            let min_height =
                parse_u16_env("FORGE_TUI_INLINE_MIN_HEIGHT").unwrap_or(INLINE_AUTO_MIN_HEIGHT);
            let mut max_height =
                parse_u16_env("FORGE_TUI_INLINE_MAX_HEIGHT").unwrap_or(INLINE_AUTO_MAX_HEIGHT);
            let min_height = min_height.max(1);
            if max_height < min_height {
                max_height = min_height;
            }
            ScreenMode::InlineAuto {
                min_height,
                max_height,
            }
        }
        _ => ScreenMode::Inline {
            ui_height: parse_u16_env("FORGE_TUI_INLINE_HEIGHT")
                .unwrap_or(INLINE_UI_HEIGHT)
                .max(1),
        },
    }
}

#[must_use]
pub fn translate_runtime_event(event: &Event) -> RuntimeEvent {
    match event {
        Event::Tick => RuntimeEvent::Tick,
        Event::Resize { width, height } => RuntimeEvent::Input(InputEvent::Resize(ResizeEvent {
            width: usize::from(*width),
            height: usize::from(*height),
        })),
        Event::Mouse(mouse_event) => map_mouse_event(*mouse_event)
            .map(|mouse| RuntimeEvent::Input(InputEvent::Mouse(mouse)))
            .unwrap_or(RuntimeEvent::Ignore),
        Event::Key(key_event) => {
            if is_quit_key(*key_event) {
                return RuntimeEvent::Quit;
            }
            map_key_event(*key_event)
                .map(|key| RuntimeEvent::Input(InputEvent::Key(key)))
                .unwrap_or(RuntimeEvent::Ignore)
        }
        _ => RuntimeEvent::Ignore,
    }
}

fn is_quit_key(key_event: FtuiKeyEvent) -> bool {
    if !matches!(
        key_event.kind,
        FtuiKeyEventKind::Press | FtuiKeyEventKind::Repeat
    ) {
        return false;
    }

    if key_event.modifiers.contains(FtuiModifiers::CTRL)
        && matches!(key_event.code, FtuiKeyCode::Char('c'))
    {
        return true;
    }

    matches!(key_event.code, FtuiKeyCode::Char('q'))
}

fn map_key_event(key_event: FtuiKeyEvent) -> Option<KeyEvent> {
    if !matches!(
        key_event.kind,
        FtuiKeyEventKind::Press | FtuiKeyEventKind::Repeat
    ) {
        return None;
    }

    let mut modifiers = Modifiers {
        shift: key_event.modifiers.contains(FtuiModifiers::SHIFT),
        ctrl: key_event.modifiers.contains(FtuiModifiers::CTRL),
        alt: key_event.modifiers.contains(FtuiModifiers::ALT),
    };

    let key = match key_event.code {
        FtuiKeyCode::Char(ch) => Key::Char(ch),
        FtuiKeyCode::Enter => Key::Enter,
        FtuiKeyCode::Escape => Key::Escape,
        FtuiKeyCode::Tab => Key::Tab,
        FtuiKeyCode::BackTab => {
            modifiers.shift = true;
            Key::Tab
        }
        FtuiKeyCode::Backspace => Key::Backspace,
        FtuiKeyCode::Up => Key::Up,
        FtuiKeyCode::Down => Key::Down,
        FtuiKeyCode::Left => Key::Left,
        FtuiKeyCode::Right => Key::Right,
        _ => return None,
    };

    Some(KeyEvent { key, modifiers })
}

fn map_mouse_event(mouse_event: FtuiMouseEvent) -> Option<MouseEvent> {
    let kind = match mouse_event.kind {
        FtuiMouseEventKind::ScrollUp => MouseEventKind::Wheel(MouseWheelDirection::Up),
        FtuiMouseEventKind::ScrollDown => MouseEventKind::Wheel(MouseWheelDirection::Down),
        FtuiMouseEventKind::Down(button) => MouseEventKind::Down(map_mouse_button(button)?),
        FtuiMouseEventKind::Up(button) => MouseEventKind::Up(map_mouse_button(button)?),
        FtuiMouseEventKind::Drag(button) => MouseEventKind::Drag(map_mouse_button(button)?),
        FtuiMouseEventKind::Moved => MouseEventKind::Move,
        _ => return None,
    };
    Some(MouseEvent {
        kind,
        column: mouse_event.x as usize,
        row: mouse_event.y as usize,
    })
}

fn map_mouse_button(button: ftui::core::event::MouseButton) -> Option<MouseButton> {
    match button {
        ftui::core::event::MouseButton::Left => Some(MouseButton::Left),
        ftui::core::event::MouseButton::Right => Some(MouseButton::Right),
        ftui::core::event::MouseButton::Middle => Some(MouseButton::Middle),
    }
}

fn perform_refresh(task_name: &'static str, db_path: PathBuf) -> Cmd<ForgeShellMsg> {
    // This pin exposes Cmd::task_named; we treat this as our perform path.
    Cmd::task_named(task_name, move || {
        ForgeShellMsg::SnapshotLoaded(load_snapshot(&db_path))
    })
}

fn load_snapshot(db_path: &Path) -> BootstrapSnapshot {
    match count_loops(db_path) {
        Ok(loop_count) => BootstrapSnapshot::ok(loop_count),
        Err(err) => BootstrapSnapshot::err(err),
    }
}

fn count_loops(db_path: &Path) -> Result<usize, String> {
    if !db_path.exists() {
        return Ok(0);
    }

    let db = forge_db::Db::open(forge_db::Config::new(db_path))
        .map_err(|err| format!("open database {}: {err}", db_path.display()))?;
    let repo = forge_db::loop_repository::LoopRepository::new(&db);
    let loops = repo
        .list()
        .map_err(|err| format!("list loops from {}: {err}", db_path.display()))?;
    Ok(loops.len())
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn parse_u16_env(key: &str) -> Option<u16> {
    std::env::var(key).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            trimmed.parse::<u16>().ok()
        }
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        resolve_screen_mode_from_env, translate_runtime_event, ForgeShell, ForgeShellMsg,
        RuntimeEvent,
    };
    use forge_ftui_adapter::input::{
        InputEvent, Key, KeyEvent, Modifiers, MouseEvent, MouseEventKind, MouseWheelDirection,
        ResizeEvent,
    };
    use forge_ftui_adapter::upstream_ftui as ftui;
    use ftui::core::event::{
        Event, KeyCode as FtuiKeyCode, KeyEvent as FtuiKeyEvent, KeyEventKind as FtuiKeyEventKind,
        MouseEvent as FtuiMouseEvent, MouseEventKind as FtuiMouseEventKind,
    };
    use ftui::{Cmd, Model, ScreenMode};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    #[test]
    fn translate_runtime_event_maps_key_resize_mouse() {
        let key_event =
            Event::Key(FtuiKeyEvent::new(FtuiKeyCode::Up).with_kind(FtuiKeyEventKind::Press));
        assert_eq!(
            translate_runtime_event(&key_event),
            RuntimeEvent::Input(InputEvent::Key(KeyEvent::plain(Key::Up)))
        );

        let resize_event = Event::Resize {
            width: 120,
            height: 44,
        };
        assert_eq!(
            translate_runtime_event(&resize_event),
            RuntimeEvent::Input(InputEvent::Resize(ResizeEvent {
                width: 120,
                height: 44,
            }))
        );

        let mouse_event = Event::Mouse(FtuiMouseEvent::new(FtuiMouseEventKind::ScrollDown, 0, 0));
        assert_eq!(
            translate_runtime_event(&mouse_event),
            RuntimeEvent::Input(InputEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Wheel(MouseWheelDirection::Down),
                column: 0,
                row: 0,
            }))
        );
    }

    #[test]
    fn translate_runtime_event_maps_quit_keys() {
        let key_event = Event::Key(FtuiKeyEvent::new(FtuiKeyCode::Char('q')));
        assert_eq!(translate_runtime_event(&key_event), RuntimeEvent::Quit);

        let ctrl_c_event = Event::Key(
            FtuiKeyEvent::new(FtuiKeyCode::Char('c')).with_modifiers(ftui::Modifiers::CTRL),
        );
        assert_eq!(translate_runtime_event(&ctrl_c_event), RuntimeEvent::Quit);
    }

    #[test]
    fn shell_tick_uses_async_task_command() {
        let mut shell = ForgeShell::new(std::env::temp_dir().join("forge-shell-bootstrap.sqlite"));
        let cmd = shell.update(ForgeShellMsg::Runtime(RuntimeEvent::Tick));
        assert!(matches!(cmd, Cmd::Task(..)));
    }

    #[test]
    fn shell_snapshot_completion_updates_state() {
        let mut shell = ForgeShell::new(std::env::temp_dir().join("forge-shell-bootstrap.sqlite"));

        let completion = ForgeShellMsg::SnapshotLoaded(super::BootstrapSnapshot {
            loop_count: 7,
            refreshed_at_epoch_secs: 123,
            error: Some("boom".to_owned()),
        });
        let cmd = shell.update(completion);

        assert!(matches!(cmd, Cmd::None));
        assert_eq!(shell.loop_count, 7);
        assert_eq!(shell.refresh_count, 1);
        assert_eq!(shell.last_refreshed_at_epoch_secs, 123);
        assert_eq!(shell.last_error.as_deref(), Some("boom"));
    }

    #[test]
    fn from_event_uses_translator() {
        let msg = ForgeShellMsg::from(Event::Resize {
            width: 88,
            height: 22,
        });
        assert_eq!(
            msg,
            ForgeShellMsg::Runtime(RuntimeEvent::Input(InputEvent::Resize(ResizeEvent {
                width: 88,
                height: 22,
            })))
        );
    }

    #[test]
    fn map_key_preserves_modifiers_for_supported_keys() {
        let input = translate_runtime_event(&Event::Key(
            FtuiKeyEvent::new(FtuiKeyCode::Char('r')).with_modifiers(ftui::Modifiers::CTRL),
        ));

        assert_eq!(
            input,
            RuntimeEvent::Input(InputEvent::Key(KeyEvent {
                key: Key::Char('r'),
                modifiers: Modifiers {
                    shift: false,
                    ctrl: true,
                    alt: false,
                },
            }))
        );
    }

    #[test]
    fn screen_mode_defaults_to_inline() {
        let _lock = env_lock();
        let _guard = EnvGuard::set("FORGE_TUI_SCREEN_MODE", "inline");
        let _reset_height = EnvGuard::unset("FORGE_TUI_INLINE_HEIGHT");

        assert_eq!(
            resolve_screen_mode_from_env(),
            ScreenMode::Inline { ui_height: 10 }
        );
    }

    #[test]
    fn screen_mode_uses_inline_height_override() {
        let _lock = env_lock();
        let _guard = EnvGuard::set("FORGE_TUI_SCREEN_MODE", "inline");
        let _height = EnvGuard::set("FORGE_TUI_INLINE_HEIGHT", "14");

        assert_eq!(
            resolve_screen_mode_from_env(),
            ScreenMode::Inline { ui_height: 14 }
        );
    }

    #[test]
    fn screen_mode_supports_inline_auto() {
        let _lock = env_lock();
        let _guard = EnvGuard::set("FORGE_TUI_SCREEN_MODE", "inline-auto");
        let _min = EnvGuard::set("FORGE_TUI_INLINE_MIN_HEIGHT", "7");
        let _max = EnvGuard::set("FORGE_TUI_INLINE_MAX_HEIGHT", "25");

        assert_eq!(
            resolve_screen_mode_from_env(),
            ScreenMode::InlineAuto {
                min_height: 7,
                max_height: 25,
            }
        );
    }

    #[test]
    fn screen_mode_inline_auto_clamps_bad_bounds() {
        let _lock = env_lock();
        let _guard = EnvGuard::set("FORGE_TUI_SCREEN_MODE", "auto");
        let _min = EnvGuard::set("FORGE_TUI_INLINE_MIN_HEIGHT", "30");
        let _max = EnvGuard::set("FORGE_TUI_INLINE_MAX_HEIGHT", "10");

        assert_eq!(
            resolve_screen_mode_from_env(),
            ScreenMode::InlineAuto {
                min_height: 30,
                max_height: 30,
            }
        );
    }

    #[test]
    fn screen_mode_supports_alt_screen() {
        let _lock = env_lock();
        let _guard = EnvGuard::set("FORGE_TUI_SCREEN_MODE", "altscreen");
        assert_eq!(resolve_screen_mode_from_env(), ScreenMode::AltScreen);
    }

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = LOCK.get_or_init(|| Mutex::new(()));
        match lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    struct EnvGuard {
        key: String,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self {
                key: key.to_owned(),
                previous,
            }
        }

        fn unset(key: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self {
                key: key.to_owned(),
                previous,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                std::env::set_var(&self.key, previous);
            } else {
                std::env::remove_var(&self.key);
            }
        }
    }
}
