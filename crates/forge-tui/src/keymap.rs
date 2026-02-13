//! Centralized keybinding registry for Forge TUI.
//!
//! Provides:
//! - scoped key resolution (mode + view + global)
//! - collision detection
//! - conflict diagnostics rendering

use std::collections::HashMap;

use forge_ftui_adapter::input::{Key, KeyEvent};

use crate::app::MainTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModeScope {
    Main,
    Filter,
    ExpandedLogs,
    Confirm,
    Wizard,
    Help,
    Palette,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyScope {
    Global,
    Mode(ModeScope),
    View(MainTab),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyToken {
    Char(char),
    Enter,
    Escape,
    Tab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub token: KeyToken,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl KeyChord {
    #[must_use]
    pub const fn plain(token: KeyToken) -> Self {
        Self {
            token,
            shift: false,
            ctrl: false,
            alt: false,
        }
    }

    #[must_use]
    pub const fn ctrl_char(ch: char) -> Self {
        Self {
            token: KeyToken::Char(ch),
            shift: false,
            ctrl: true,
            alt: false,
        }
    }

    #[must_use]
    pub const fn shift_tab() -> Self {
        Self {
            token: KeyToken::Tab,
            shift: true,
            ctrl: false,
            alt: false,
        }
    }

    #[must_use]
    pub fn from_event(event: KeyEvent) -> Self {
        Self {
            token: match event.key {
                Key::Char(ch) => KeyToken::Char(ch),
                Key::Enter => KeyToken::Enter,
                Key::Escape => KeyToken::Escape,
                Key::Tab => KeyToken::Tab,
                Key::Backspace => KeyToken::Backspace,
                Key::Up => KeyToken::Up,
                Key::Down => KeyToken::Down,
                Key::Left => KeyToken::Left,
                Key::Right => KeyToken::Right,
            },
            shift: event.modifiers.shift,
            ctrl: event.modifiers.ctrl,
            alt: event.modifiers.alt,
        }
    }

    #[must_use]
    pub fn display(self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        if self.shift {
            parts.push("Shift".to_owned());
        }
        let key = match self.token {
            KeyToken::Char(ch) => ch.to_ascii_uppercase().to_string(),
            KeyToken::Enter => "Enter".to_owned(),
            KeyToken::Escape => "Esc".to_owned(),
            KeyToken::Tab => "Tab".to_owned(),
            KeyToken::Backspace => "Backspace".to_owned(),
            KeyToken::Up => "Up".to_owned(),
            KeyToken::Down => "Down".to_owned(),
            KeyToken::Left => "Left".to_owned(),
            KeyToken::Right => "Right".to_owned(),
        };
        parts.push(key);
        parts.join("+")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCommand {
    Quit,
    ToggleHelp,
    OpenPalette,
    SwitchTabOverview,
    SwitchTabLogs,
    SwitchTabRuns,
    SwitchTabMultiLogs,
    SwitchTabInbox,
    CycleTabNext,
    CycleTabPrev,
    MoveSelectionNext,
    MoveSelectionPrev,
    OpenFilter,
    ExportCurrentView,
    CycleTheme,
    CycleAccessibilityTheme,
    ToggleZen,
    OpenWizard,
    ResumeSelected,
    ConfirmStop,
    ConfirmKill,
    ConfirmDelete,
    LogsCycleSource,
    CycleLogLayer,
    ScrollLogsUp,
    ScrollLogsDown,
    OpenExpandedLogs,
    RunSelectionPrev,
    RunSelectionNext,
    MultiCycleLayout,
    MultiPageStart,
    MultiPageEnd,
    MultiPagePrev,
    MultiPageNext,
    TogglePin,
    ClearPinned,
    PaletteClose,
    PaletteMoveNext,
    PaletteMovePrev,
    PaletteQueryBackspace,
    PaletteExecute,
    OpenSearch,
    SearchClose,
    SearchMoveNext,
    SearchMovePrev,
    SearchQueryBackspace,
    SearchExecute,
    SearchNextMatch,
    SearchPrevMatch,
    ToggleFollow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
    pub scope: KeyScope,
    pub chord: KeyChord,
    pub command: KeyCommand,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyConflict {
    pub scope: KeyScope,
    pub chord: KeyChord,
    pub commands: Vec<KeyCommand>,
}

#[derive(Debug, Clone, Default)]
pub struct Keymap {
    bindings: Vec<KeyBinding>,
}

impl Keymap {
    #[must_use]
    pub fn new(bindings: Vec<KeyBinding>) -> Self {
        Self { bindings }
    }

    #[must_use]
    pub fn default_forge_tui() -> Self {
        use KeyCommand as Cmd;
        use KeyScope as Scope;
        use KeyToken as Tok;
        let bindings = vec![
            bind(Scope::Global, KeyChord::ctrl_char('c'), Cmd::Quit, "quit"),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('q')),
                Cmd::Quit,
                "quit",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('?')),
                Cmd::ToggleHelp,
                "help",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::ctrl_char('p'),
                Cmd::OpenPalette,
                "open command palette",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('1')),
                Cmd::SwitchTabOverview,
                "switch overview",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('2')),
                Cmd::SwitchTabLogs,
                "switch logs",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('3')),
                Cmd::SwitchTabRuns,
                "switch runs",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('4')),
                Cmd::SwitchTabMultiLogs,
                "switch multi logs",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('5')),
                Cmd::SwitchTabInbox,
                "switch inbox",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char(']')),
                Cmd::CycleTabNext,
                "cycle tab next",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('[')),
                Cmd::CycleTabPrev,
                "cycle tab prev",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('j')),
                Cmd::MoveSelectionNext,
                "selection next",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Down),
                Cmd::MoveSelectionNext,
                "selection next",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('k')),
                Cmd::MoveSelectionPrev,
                "selection prev",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Up),
                Cmd::MoveSelectionPrev,
                "selection prev",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('/')),
                Cmd::OpenFilter,
                "open filter",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('E')),
                Cmd::ExportCurrentView,
                "export current view",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('t')),
                Cmd::CycleTheme,
                "cycle theme",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('T')),
                Cmd::CycleAccessibilityTheme,
                "cycle accessibility theme",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('z')),
                Cmd::ToggleZen,
                "toggle zen",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('n')),
                Cmd::OpenWizard,
                "new wizard",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('r')),
                Cmd::ResumeSelected,
                "resume selected",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('S')),
                Cmd::ConfirmStop,
                "stop selected",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('K')),
                Cmd::ConfirmKill,
                "kill selected",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('D')),
                Cmd::ConfirmDelete,
                "delete selected",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('x')),
                Cmd::CycleLogLayer,
                "cycle log layer",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('l')),
                Cmd::OpenExpandedLogs,
                "expand logs",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char(',')),
                Cmd::RunSelectionPrev,
                "run previous",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('.')),
                Cmd::RunSelectionNext,
                "run next",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char(' ')),
                Cmd::TogglePin,
                "toggle pin",
            ),
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::plain(Tok::Char('c')),
                Cmd::ClearPinned,
                "clear pins",
            ),
            bind(
                Scope::View(MainTab::Logs),
                KeyChord::plain(Tok::Char('v')),
                Cmd::LogsCycleSource,
                "cycle source",
            ),
            bind(
                Scope::View(MainTab::Logs),
                KeyChord::plain(Tok::Char('u')),
                Cmd::ScrollLogsUp,
                "scroll up",
            ),
            bind(
                Scope::View(MainTab::Logs),
                KeyChord::plain(Tok::Char('d')),
                Cmd::ScrollLogsDown,
                "scroll down",
            ),
            bind(
                Scope::View(MainTab::Runs),
                KeyChord::plain(Tok::Char('u')),
                Cmd::ScrollLogsUp,
                "scroll up",
            ),
            bind(
                Scope::View(MainTab::Runs),
                KeyChord::plain(Tok::Char('d')),
                Cmd::ScrollLogsDown,
                "scroll down",
            ),
            bind(
                Scope::View(MainTab::MultiLogs),
                KeyChord::plain(Tok::Char('m')),
                Cmd::MultiCycleLayout,
                "cycle layout",
            ),
            bind(
                Scope::View(MainTab::MultiLogs),
                KeyChord::plain(Tok::Char('g')),
                Cmd::MultiPageStart,
                "first page",
            ),
            bind(
                Scope::View(MainTab::MultiLogs),
                KeyChord::plain(Tok::Char('G')),
                Cmd::MultiPageEnd,
                "last page",
            ),
            bind(
                Scope::View(MainTab::MultiLogs),
                KeyChord::plain(Tok::Char(',')),
                Cmd::MultiPagePrev,
                "page prev",
            ),
            bind(
                Scope::View(MainTab::MultiLogs),
                KeyChord::plain(Tok::Char('.')),
                Cmd::MultiPageNext,
                "page next",
            ),
            // -- Follow mode --
            bind(
                Scope::View(MainTab::Logs),
                KeyChord::plain(Tok::Char('F')),
                Cmd::ToggleFollow,
                "toggle follow",
            ),
            bind(
                Scope::View(MainTab::Runs),
                KeyChord::plain(Tok::Char('F')),
                Cmd::ToggleFollow,
                "toggle follow",
            ),
            bind(
                Scope::Mode(ModeScope::ExpandedLogs),
                KeyChord::plain(Tok::Char('F')),
                Cmd::ToggleFollow,
                "toggle follow",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::plain(Tok::Escape),
                Cmd::PaletteClose,
                "close palette",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::plain(Tok::Char('q')),
                Cmd::PaletteClose,
                "close palette",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::plain(Tok::Char('?')),
                Cmd::ToggleHelp,
                "palette help",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::plain(Tok::Tab),
                Cmd::PaletteMoveNext,
                "next palette item",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::shift_tab(),
                Cmd::PaletteMovePrev,
                "previous palette item",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::plain(Tok::Down),
                Cmd::PaletteMoveNext,
                "next palette item",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::plain(Tok::Char('j')),
                Cmd::PaletteMoveNext,
                "next palette item",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::plain(Tok::Up),
                Cmd::PaletteMovePrev,
                "previous palette item",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::plain(Tok::Char('k')),
                Cmd::PaletteMovePrev,
                "previous palette item",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::plain(Tok::Backspace),
                Cmd::PaletteQueryBackspace,
                "query backspace",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::ctrl_char('h'),
                Cmd::PaletteQueryBackspace,
                "query backspace",
            ),
            bind(
                Scope::Mode(ModeScope::Palette),
                KeyChord::plain(Tok::Enter),
                Cmd::PaletteExecute,
                "execute palette",
            ),
            // -- Search mode --
            bind(
                Scope::Mode(ModeScope::Main),
                KeyChord::ctrl_char('f'),
                Cmd::OpenSearch,
                "open search",
            ),
            bind(
                Scope::Mode(ModeScope::Search),
                KeyChord::plain(Tok::Escape),
                Cmd::SearchClose,
                "close search",
            ),
            bind(
                Scope::Mode(ModeScope::Search),
                KeyChord::plain(Tok::Down),
                Cmd::SearchMoveNext,
                "next search result",
            ),
            bind(
                Scope::Mode(ModeScope::Search),
                KeyChord::plain(Tok::Tab),
                Cmd::SearchMoveNext,
                "next search result",
            ),
            bind(
                Scope::Mode(ModeScope::Search),
                KeyChord::plain(Tok::Up),
                Cmd::SearchMovePrev,
                "previous search result",
            ),
            bind(
                Scope::Mode(ModeScope::Search),
                KeyChord::shift_tab(),
                Cmd::SearchMovePrev,
                "previous search result",
            ),
            bind(
                Scope::Mode(ModeScope::Search),
                KeyChord::plain(Tok::Backspace),
                Cmd::SearchQueryBackspace,
                "search query backspace",
            ),
            bind(
                Scope::Mode(ModeScope::Search),
                KeyChord::ctrl_char('h'),
                Cmd::SearchQueryBackspace,
                "search query backspace",
            ),
            bind(
                Scope::Mode(ModeScope::Search),
                KeyChord::plain(Tok::Enter),
                Cmd::SearchExecute,
                "jump to search result",
            ),
            bind(
                Scope::Mode(ModeScope::Search),
                KeyChord::ctrl_char('n'),
                Cmd::SearchNextMatch,
                "next match",
            ),
            bind(
                Scope::Mode(ModeScope::Search),
                KeyChord::ctrl_char('p'),
                Cmd::SearchPrevMatch,
                "previous match",
            ),
        ];
        Self { bindings }
    }

    #[must_use]
    pub fn resolve(&self, scopes: &[KeyScope], chord: KeyChord) -> Option<KeyCommand> {
        for scope in scopes {
            if let Some(binding) = self
                .bindings
                .iter()
                .find(|binding| binding.scope == *scope && binding.chord == chord)
            {
                return Some(binding.command);
            }
        }
        None
    }

    #[must_use]
    pub fn conflicts(&self) -> Vec<KeyConflict> {
        let mut by_scope_chord: HashMap<(KeyScope, KeyChord), Vec<KeyCommand>> = HashMap::new();
        for binding in &self.bindings {
            by_scope_chord
                .entry((binding.scope, binding.chord))
                .or_default()
                .push(binding.command);
        }
        let mut conflicts: Vec<KeyConflict> = by_scope_chord
            .into_iter()
            .filter_map(|((scope, chord), commands)| {
                if commands.len() > 1 {
                    Some(KeyConflict {
                        scope,
                        chord,
                        commands,
                    })
                } else {
                    None
                }
            })
            .collect();
        conflicts.sort_by(|a, b| {
            format!("{:?}", a.scope)
                .cmp(&format!("{:?}", b.scope))
                .then(a.chord.display().cmp(&b.chord.display()))
        });
        conflicts
    }

    #[must_use]
    pub fn conflict_diagnostics_lines(&self, width: usize, max_rows: usize) -> Vec<String> {
        if max_rows == 0 {
            return Vec::new();
        }
        let conflicts = self.conflicts();
        let mut lines = vec![truncate("Keymap diagnostics", width)];
        if lines.len() >= max_rows {
            return lines;
        }
        if conflicts.is_empty() {
            lines.push(truncate("  no conflicts detected", width));
            return lines;
        }
        lines.push(truncate(
            &format!("  {} conflict(s) detected", conflicts.len()),
            width,
        ));
        for conflict in conflicts {
            if lines.len() >= max_rows {
                break;
            }
            let actions = conflict
                .commands
                .iter()
                .map(|command| format!("{command:?}"))
                .collect::<Vec<String>>()
                .join(",");
            let row = format!(
                "  {:?} {} -> {}",
                conflict.scope,
                conflict.chord.display(),
                actions
            );
            lines.push(truncate(&row, width));
        }
        lines
    }
}

fn bind(
    scope: KeyScope,
    chord: KeyChord,
    command: KeyCommand,
    description: &'static str,
) -> KeyBinding {
    KeyBinding {
        scope,
        chord,
        command,
        description,
    }
}

fn truncate(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    value.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::{KeyChord, KeyCommand, KeyScope, KeyToken, Keymap, ModeScope};
    use crate::app::MainTab;

    #[test]
    fn resolves_with_scope_precedence_snapshot() {
        let map = Keymap::default_forge_tui();
        let lines = [
            (
                vec![
                    KeyScope::View(MainTab::Logs),
                    KeyScope::Mode(ModeScope::Main),
                    KeyScope::Global,
                ],
                KeyChord::plain(KeyToken::Char('v')),
            ),
            (
                vec![
                    KeyScope::View(MainTab::Runs),
                    KeyScope::Mode(ModeScope::Main),
                    KeyScope::Global,
                ],
                KeyChord::plain(KeyToken::Char('v')),
            ),
            (
                vec![KeyScope::Mode(ModeScope::Main), KeyScope::Global],
                KeyChord::ctrl_char('p'),
            ),
            (
                vec![KeyScope::Mode(ModeScope::Main), KeyScope::Global],
                KeyChord::ctrl_char('c'),
            ),
        ]
        .iter()
        .map(|(scopes, chord)| {
            let command = map.resolve(scopes, *chord);
            format!("{} => {:?}", chord.display(), command)
        })
        .collect::<Vec<String>>()
        .join("\n");

        let expected = [
            "V => Some(LogsCycleSource)",
            "V => None",
            "Ctrl+P => Some(OpenPalette)",
            "Ctrl+C => Some(Quit)",
        ]
        .join("\n");
        assert_eq!(lines, expected);
    }

    #[test]
    fn default_keymap_has_no_collisions() {
        let map = Keymap::default_forge_tui();
        assert!(map.conflicts().is_empty());
    }

    #[test]
    fn conflict_detector_reports_duplicates() {
        let mut map = Keymap::default_forge_tui();
        map.bindings.push(super::bind(
            KeyScope::Mode(ModeScope::Main),
            KeyChord::plain(KeyToken::Char('q')),
            KeyCommand::OpenFilter,
            "duplicate for test",
        ));
        let conflicts = map.conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].scope, KeyScope::Mode(ModeScope::Main));
        assert_eq!(conflicts[0].chord, KeyChord::plain(KeyToken::Char('q')));
    }

    #[test]
    fn conflict_diagnostics_panel_snapshot() {
        let map = Keymap::default_forge_tui();
        let lines = map.conflict_diagnostics_lines(80, 4);
        assert_eq!(
            lines.join("\n"),
            "Keymap diagnostics\n  no conflicts detected"
        );
    }
}
