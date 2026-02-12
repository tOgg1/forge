//! Stable extension API for custom Forge TUI panels.

use std::collections::BTreeMap;

use forge_ftui_adapter::input::InputEvent;
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelMode {
    ReadOnly,
    Interactive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionPanelDescriptor {
    pub id: String,
    pub title: String,
    pub version: String,
    pub mode: PanelMode,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelMountContext {
    pub workspace_id: String,
    pub now_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelRuntimeContext {
    pub now_epoch_s: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelUnmountReason {
    HostShutdown,
    Replaced,
    UserClosed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelEvent {
    Input(InputEvent),
    Tick,
    DataRefresh { source: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelEffect {
    None,
    RequestRefresh,
    SetStatus(String),
    EmitAction(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PanelUpdate {
    pub effects: Vec<PanelEffect>,
    pub close_requested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSessionState {
    Created,
    Mounted,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelRegistryError {
    InvalidId,
    DuplicateId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelSessionError {
    UnknownPanelId,
    InvalidLifecycle,
}

pub trait ExtensionPanel {
    fn descriptor(&self) -> &ExtensionPanelDescriptor;

    fn on_mount(&mut self, _context: &PanelMountContext) -> PanelUpdate {
        PanelUpdate::default()
    }

    fn on_event(&mut self, _event: &PanelEvent, _context: &PanelRuntimeContext) -> PanelUpdate {
        PanelUpdate::default()
    }

    fn render(&self, size: FrameSize, theme: ThemeSpec) -> RenderFrame;

    fn on_unmount(&mut self, _reason: PanelUnmountReason) {}
}

struct RegisteredPanel {
    descriptor: ExtensionPanelDescriptor,
    factory: Box<dyn Fn() -> Box<dyn ExtensionPanel>>,
}

#[derive(Default)]
pub struct PanelRegistry {
    panels: BTreeMap<String, RegisteredPanel>,
}

impl PanelRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        descriptor: ExtensionPanelDescriptor,
        factory: Box<dyn Fn() -> Box<dyn ExtensionPanel>>,
    ) -> Result<(), PanelRegistryError> {
        let id = normalize_id(&descriptor.id);
        if id.is_empty() {
            return Err(PanelRegistryError::InvalidId);
        }
        if self.panels.contains_key(&id) {
            return Err(PanelRegistryError::DuplicateId);
        }
        let mut normalized = descriptor;
        normalized.id = id.clone();
        if normalized.title.trim().is_empty() {
            normalized.title = "Untitled Panel".to_owned();
        }
        if normalized.version.trim().is_empty() {
            normalized.version = "0.1.0".to_owned();
        }
        if normalized.description.trim().is_empty() {
            normalized.description = "custom panel extension".to_owned();
        }
        self.panels.insert(
            id,
            RegisteredPanel {
                descriptor: normalized,
                factory,
            },
        );
        Ok(())
    }

    #[must_use]
    pub fn descriptors(&self) -> Vec<ExtensionPanelDescriptor> {
        self.panels
            .values()
            .map(|registered| registered.descriptor.clone())
            .collect()
    }

    pub fn create_session(&self, panel_id: &str) -> Result<PanelSession, PanelSessionError> {
        let panel_id = normalize_id(panel_id);
        let Some(registered) = self.panels.get(&panel_id) else {
            return Err(PanelSessionError::UnknownPanelId);
        };
        let panel = (registered.factory)();
        Ok(PanelSession {
            panel,
            state: PanelSessionState::Created,
        })
    }
}

pub struct PanelSession {
    panel: Box<dyn ExtensionPanel>,
    state: PanelSessionState,
}

impl PanelSession {
    #[must_use]
    pub fn descriptor(&self) -> &ExtensionPanelDescriptor {
        self.panel.descriptor()
    }

    #[must_use]
    pub fn state(&self) -> PanelSessionState {
        self.state
    }

    pub fn mount(&mut self, context: &PanelMountContext) -> Result<PanelUpdate, PanelSessionError> {
        if self.state != PanelSessionState::Created {
            return Err(PanelSessionError::InvalidLifecycle);
        }
        self.state = PanelSessionState::Mounted;
        Ok(self.panel.on_mount(context))
    }

    pub fn dispatch_event(
        &mut self,
        event: &PanelEvent,
        context: &PanelRuntimeContext,
    ) -> Result<PanelUpdate, PanelSessionError> {
        if self.state != PanelSessionState::Mounted {
            return Err(PanelSessionError::InvalidLifecycle);
        }
        if self.panel.descriptor().mode == PanelMode::ReadOnly
            && matches!(event, PanelEvent::Input(_))
        {
            return Ok(PanelUpdate {
                effects: vec![PanelEffect::SetStatus(
                    "read-only panel ignored interactive input".to_owned(),
                )],
                close_requested: false,
            });
        }
        Ok(self.panel.on_event(event, context))
    }

    #[must_use]
    pub fn render(&self, size: FrameSize, theme: ThemeSpec) -> RenderFrame {
        if self.state == PanelSessionState::Closed {
            let mut frame = RenderFrame::new(size, theme);
            frame.draw_text(0, 0, "panel session closed", TextRole::Muted);
            return frame;
        }
        self.panel.render(size, theme)
    }

    pub fn unmount(&mut self, reason: PanelUnmountReason) -> Result<(), PanelSessionError> {
        if self.state != PanelSessionState::Mounted {
            return Err(PanelSessionError::InvalidLifecycle);
        }
        self.panel.on_unmount(reason);
        self.state = PanelSessionState::Closed;
        Ok(())
    }
}

fn normalize_id(value: &str) -> String {
    let mut output = String::new();
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_' || ch.is_ascii_whitespace()) && !output.ends_with('-') {
            output.push('-');
        }
    }
    output.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::render::FrameSize;
    use forge_ftui_adapter::style::ThemeSpec;

    use super::{
        ExtensionPanel, ExtensionPanelDescriptor, PanelEvent, PanelMode, PanelMountContext,
        PanelRegistry, PanelRegistryError, PanelRuntimeContext, PanelSessionError,
        PanelSessionState, PanelUnmountReason, PanelUpdate,
    };

    struct FakePanel {
        descriptor: ExtensionPanelDescriptor,
        calls: Rc<RefCell<Vec<String>>>,
    }

    impl ExtensionPanel for FakePanel {
        fn descriptor(&self) -> &ExtensionPanelDescriptor {
            &self.descriptor
        }

        fn on_mount(&mut self, context: &PanelMountContext) -> PanelUpdate {
            self.calls
                .borrow_mut()
                .push(format!("mount:{}", context.workspace_id));
            PanelUpdate::default()
        }

        fn on_event(&mut self, event: &PanelEvent, _context: &PanelRuntimeContext) -> PanelUpdate {
            let label = match event {
                PanelEvent::Input(_) => "event:input",
                PanelEvent::Tick => "event:tick",
                PanelEvent::DataRefresh { .. } => "event:data",
            };
            self.calls.borrow_mut().push(label.to_owned());
            PanelUpdate::default()
        }

        fn render(
            &self,
            size: FrameSize,
            theme: ThemeSpec,
        ) -> forge_ftui_adapter::render::RenderFrame {
            let mut frame = forge_ftui_adapter::render::RenderFrame::new(size, theme);
            frame.draw_text(
                0,
                0,
                &self.descriptor.title,
                forge_ftui_adapter::render::TextRole::Primary,
            );
            frame
        }

        fn on_unmount(&mut self, _reason: PanelUnmountReason) {
            self.calls.borrow_mut().push("unmount".to_owned());
        }
    }

    fn descriptor(mode: PanelMode) -> ExtensionPanelDescriptor {
        ExtensionPanelDescriptor {
            id: "custom-panel".to_owned(),
            title: "Custom Panel".to_owned(),
            version: "1.0.0".to_owned(),
            mode,
            description: "test panel".to_owned(),
        }
    }

    #[test]
    fn register_and_list_descriptor() {
        let calls = Rc::new(RefCell::new(Vec::<String>::new()));
        let mut registry = PanelRegistry::new();
        let register_result = registry.register(
            descriptor(PanelMode::Interactive),
            Box::new({
                let calls = calls.clone();
                move || {
                    Box::new(FakePanel {
                        descriptor: descriptor(PanelMode::Interactive),
                        calls: calls.clone(),
                    })
                }
            }),
        );
        assert_eq!(register_result, Ok(()));
        let descriptors = registry.descriptors();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].id, "custom-panel");
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut registry = PanelRegistry::new();
        let first = registry.register(
            descriptor(PanelMode::Interactive),
            Box::new(|| {
                Box::new(FakePanel {
                    descriptor: descriptor(PanelMode::Interactive),
                    calls: Rc::new(RefCell::new(Vec::new())),
                })
            }),
        );
        assert_eq!(first, Ok(()));
        let second = registry.register(
            descriptor(PanelMode::Interactive),
            Box::new(|| {
                Box::new(FakePanel {
                    descriptor: descriptor(PanelMode::Interactive),
                    calls: Rc::new(RefCell::new(Vec::new())),
                })
            }),
        );
        assert_eq!(second, Err(PanelRegistryError::DuplicateId));
    }

    #[test]
    fn lifecycle_mount_event_render_unmount() {
        let calls = Rc::new(RefCell::new(Vec::<String>::new()));
        let mut registry = PanelRegistry::new();
        let register = registry.register(
            descriptor(PanelMode::Interactive),
            Box::new({
                let calls = calls.clone();
                move || {
                    Box::new(FakePanel {
                        descriptor: descriptor(PanelMode::Interactive),
                        calls: calls.clone(),
                    })
                }
            }),
        );
        assert_eq!(register, Ok(()));

        let mut session = match registry.create_session("custom-panel") {
            Ok(session) => session,
            Err(err) => panic!("expected session, got {err:?}"),
        };
        assert_eq!(session.state(), PanelSessionState::Created);
        let mount_result = session.mount(&PanelMountContext {
            workspace_id: "forge".to_owned(),
            now_epoch_s: 10,
        });
        assert_eq!(mount_result, Ok(PanelUpdate::default()));
        assert_eq!(session.state(), PanelSessionState::Mounted);

        let dispatch =
            session.dispatch_event(&PanelEvent::Tick, &PanelRuntimeContext { now_epoch_s: 11 });
        assert_eq!(dispatch, Ok(PanelUpdate::default()));

        let frame = session.render(
            FrameSize {
                width: 24,
                height: 2,
            },
            ThemeSpec::default(),
        );
        assert!(frame.row_text(0).contains("Custom Panel"));

        let unmount = session.unmount(PanelUnmountReason::HostShutdown);
        assert_eq!(unmount, Ok(()));
        assert_eq!(session.state(), PanelSessionState::Closed);

        let calls = calls.borrow().clone();
        assert_eq!(
            calls,
            vec![
                "mount:forge".to_owned(),
                "event:tick".to_owned(),
                "unmount".to_owned()
            ]
        );
    }

    #[test]
    fn read_only_panel_ignores_input_events() {
        let mut registry = PanelRegistry::new();
        let register = registry.register(
            descriptor(PanelMode::ReadOnly),
            Box::new(|| {
                Box::new(FakePanel {
                    descriptor: descriptor(PanelMode::ReadOnly),
                    calls: Rc::new(RefCell::new(Vec::new())),
                })
            }),
        );
        assert_eq!(register, Ok(()));

        let mut session = match registry.create_session("custom-panel") {
            Ok(session) => session,
            Err(err) => panic!("expected session, got {err:?}"),
        };
        let mounted = session.mount(&PanelMountContext {
            workspace_id: "forge".to_owned(),
            now_epoch_s: 20,
        });
        assert_eq!(mounted, Ok(PanelUpdate::default()));

        let update = session.dispatch_event(
            &PanelEvent::Input(InputEvent::Key(KeyEvent::plain(Key::Enter))),
            &PanelRuntimeContext { now_epoch_s: 21 },
        );
        match update {
            Ok(update) => assert_eq!(update.effects.len(), 1),
            Err(err) => panic!("expected update, got {err:?}"),
        }
    }

    #[test]
    fn unknown_panel_and_invalid_lifecycle_errors() {
        let registry = PanelRegistry::new();
        let session = registry.create_session("missing");
        match session {
            Ok(_) => panic!("expected unknown panel error"),
            Err(err) => assert_eq!(err, PanelSessionError::UnknownPanelId),
        }

        let mut registry = PanelRegistry::new();
        let registered = registry.register(
            descriptor(PanelMode::Interactive),
            Box::new(|| {
                Box::new(FakePanel {
                    descriptor: descriptor(PanelMode::Interactive),
                    calls: Rc::new(RefCell::new(Vec::new())),
                })
            }),
        );
        assert_eq!(registered, Ok(()));
        let mut session = match registry.create_session("custom-panel") {
            Ok(session) => session,
            Err(err) => panic!("expected session, got {err:?}"),
        };

        let event_before_mount =
            session.dispatch_event(&PanelEvent::Tick, &PanelRuntimeContext { now_epoch_s: 0 });
        assert_eq!(event_before_mount, Err(PanelSessionError::InvalidLifecycle));
        let unmount_before_mount = session.unmount(PanelUnmountReason::UserClosed);
        assert_eq!(
            unmount_before_mount,
            Err(PanelSessionError::InvalidLifecycle)
        );
    }
}
