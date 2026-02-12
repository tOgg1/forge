//! Versioned internal event bus for plugin/extension compatibility.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion {
    pub major: u16,
    pub minor: u16,
}

impl SchemaVersion {
    pub const V1_0: Self = Self { major: 1, minor: 0 };

    #[must_use]
    pub fn label(self) -> String {
        format!("{}.{}", self.major, self.minor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PluginEventKind {
    LoopSelectionChanged,
    TabChanged,
    PaletteActionExecuted,
    PanelLifecycle,
    SandboxDecision,
}

impl PluginEventKind {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::LoopSelectionChanged => "loop-selection-changed",
            Self::TabChanged => "tab-changed",
            Self::PaletteActionExecuted => "palette-action-executed",
            Self::PanelLifecycle => "panel-lifecycle",
            Self::SandboxDecision => "sandbox-decision",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEventEnvelope {
    pub event_id: String,
    pub kind: PluginEventKind,
    pub schema_version: SchemaVersion,
    pub emitted_at_epoch_s: i64,
    pub payload: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaCompatibility {
    pub major: u16,
    pub min_minor: u16,
    pub max_minor: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSubscriber {
    pub plugin_id: String,
    pub subscriptions: BTreeSet<PluginEventKind>,
    pub compatibility: BTreeMap<PluginEventKind, SchemaCompatibility>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventBusError {
    InvalidPluginId,
    DuplicatePluginId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchSkip {
    pub plugin_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchReport {
    pub event_id: String,
    pub kind: PluginEventKind,
    pub schema_version: SchemaVersion,
    pub delivered_plugin_ids: Vec<String>,
    pub skipped: Vec<DispatchSkip>,
}

#[derive(Debug, Clone)]
pub struct ExtensionEventBus {
    next_event_seq: u64,
    schemas: BTreeMap<PluginEventKind, SchemaVersion>,
    subscribers: BTreeMap<String, PluginSubscriber>,
    inboxes: BTreeMap<String, Vec<PluginEventEnvelope>>,
}

impl Default for ExtensionEventBus {
    fn default() -> Self {
        Self {
            next_event_seq: 1,
            schemas: default_schema_registry(),
            subscribers: BTreeMap::new(),
            inboxes: BTreeMap::new(),
        }
    }
}

impl ExtensionEventBus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_subscriber(
        &mut self,
        mut subscriber: PluginSubscriber,
    ) -> Result<(), EventBusError> {
        subscriber.plugin_id = normalize_id(&subscriber.plugin_id);
        if subscriber.plugin_id.is_empty() {
            return Err(EventBusError::InvalidPluginId);
        }
        if self.subscribers.contains_key(&subscriber.plugin_id) {
            return Err(EventBusError::DuplicatePluginId);
        }
        subscriber.compatibility = normalized_compatibility(subscriber.compatibility);
        self.inboxes
            .insert(subscriber.plugin_id.clone(), Vec::new());
        self.subscribers
            .insert(subscriber.plugin_id.clone(), subscriber);
        Ok(())
    }

    pub fn set_schema_version(&mut self, kind: PluginEventKind, version: SchemaVersion) {
        self.schemas.insert(kind, version);
    }

    #[must_use]
    pub fn schema_version_for(&self, kind: PluginEventKind) -> SchemaVersion {
        self.schemas
            .get(&kind)
            .copied()
            .unwrap_or(SchemaVersion::V1_0)
    }

    #[must_use]
    pub fn publish(
        &mut self,
        kind: PluginEventKind,
        payload: BTreeMap<String, String>,
        emitted_at_epoch_s: i64,
    ) -> DispatchReport {
        let schema_version = self.schema_version_for(kind);
        let event_id = format!("ev-{:08}", self.next_event_seq);
        self.next_event_seq = self.next_event_seq.saturating_add(1);

        let envelope = PluginEventEnvelope {
            event_id: event_id.clone(),
            kind,
            schema_version,
            emitted_at_epoch_s: emitted_at_epoch_s.max(0),
            payload,
        };

        let mut delivered = Vec::new();
        let mut skipped = Vec::new();
        for (plugin_id, subscriber) in &self.subscribers {
            if !subscriber.subscriptions.contains(&kind) {
                continue;
            }
            let Some(compatibility) = subscriber.compatibility.get(&kind) else {
                skipped.push(DispatchSkip {
                    plugin_id: plugin_id.clone(),
                    reason: format!(
                        "missing schema compatibility declaration for {}",
                        kind.slug()
                    ),
                });
                continue;
            };
            if !is_schema_compatible(schema_version, compatibility) {
                skipped.push(DispatchSkip {
                    plugin_id: plugin_id.clone(),
                    reason: format!(
                        "incompatible schema {} for supported {}.{}-{}.{}",
                        schema_version.label(),
                        compatibility.major,
                        compatibility.min_minor,
                        compatibility.major,
                        compatibility.max_minor
                    ),
                });
                continue;
            }
            if let Some(inbox) = self.inboxes.get_mut(plugin_id) {
                inbox.push(envelope.clone());
                delivered.push(plugin_id.clone());
            }
        }

        DispatchReport {
            event_id,
            kind,
            schema_version,
            delivered_plugin_ids: delivered,
            skipped,
        }
    }

    #[must_use]
    pub fn drain_plugin_events(&mut self, plugin_id: &str) -> Vec<PluginEventEnvelope> {
        let plugin_id = normalize_id(plugin_id);
        let Some(inbox) = self.inboxes.get_mut(&plugin_id) else {
            return Vec::new();
        };
        std::mem::take(inbox)
    }
}

fn default_schema_registry() -> BTreeMap<PluginEventKind, SchemaVersion> {
    let mut registry = BTreeMap::new();
    registry.insert(PluginEventKind::LoopSelectionChanged, SchemaVersion::V1_0);
    registry.insert(PluginEventKind::TabChanged, SchemaVersion::V1_0);
    registry.insert(PluginEventKind::PaletteActionExecuted, SchemaVersion::V1_0);
    registry.insert(PluginEventKind::PanelLifecycle, SchemaVersion::V1_0);
    registry.insert(PluginEventKind::SandboxDecision, SchemaVersion::V1_0);
    registry
}

fn normalized_compatibility(
    compatibility: BTreeMap<PluginEventKind, SchemaCompatibility>,
) -> BTreeMap<PluginEventKind, SchemaCompatibility> {
    let mut normalized = BTreeMap::new();
    for (kind, mut support) in compatibility {
        if support.max_minor < support.min_minor {
            support.max_minor = support.min_minor;
        }
        normalized.insert(kind, support);
    }
    normalized
}

fn is_schema_compatible(version: SchemaVersion, support: &SchemaCompatibility) -> bool {
    version.major == support.major
        && version.minor >= support.min_minor
        && version.minor <= support.max_minor
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
    use std::collections::{BTreeMap, BTreeSet};

    use super::{
        DispatchSkip, EventBusError, ExtensionEventBus, PluginEventKind, PluginSubscriber,
        SchemaCompatibility, SchemaVersion,
    };

    fn subscriber(
        plugin_id: &str,
        kind: PluginEventKind,
        major: u16,
        min_minor: u16,
        max_minor: u16,
    ) -> PluginSubscriber {
        let mut subscriptions = BTreeSet::new();
        subscriptions.insert(kind);
        let mut compatibility = BTreeMap::new();
        compatibility.insert(
            kind,
            SchemaCompatibility {
                major,
                min_minor,
                max_minor,
            },
        );
        PluginSubscriber {
            plugin_id: plugin_id.to_owned(),
            subscriptions,
            compatibility,
        }
    }

    #[test]
    fn compatible_subscriber_receives_event() {
        let mut bus = ExtensionEventBus::new();
        let register =
            bus.register_subscriber(subscriber("plugin-a", PluginEventKind::TabChanged, 1, 0, 2));
        assert_eq!(register, Ok(()));

        let mut payload = BTreeMap::new();
        payload.insert("tab".to_owned(), "inbox".to_owned());
        let report = bus.publish(PluginEventKind::TabChanged, payload, 120);
        assert_eq!(report.delivered_plugin_ids, vec!["plugin-a".to_owned()]);
        assert_eq!(report.skipped, Vec::<DispatchSkip>::new());

        let drained = bus.drain_plugin_events("plugin-a");
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].schema_version, SchemaVersion::V1_0);
        assert_eq!(drained[0].kind, PluginEventKind::TabChanged);
    }

    #[test]
    fn incompatible_minor_range_is_skipped() {
        let mut bus = ExtensionEventBus::new();
        let register =
            bus.register_subscriber(subscriber("plugin-a", PluginEventKind::TabChanged, 1, 0, 0));
        assert_eq!(register, Ok(()));
        bus.set_schema_version(
            PluginEventKind::TabChanged,
            SchemaVersion { major: 1, minor: 2 },
        );

        let report = bus.publish(PluginEventKind::TabChanged, BTreeMap::new(), 120);
        assert_eq!(report.delivered_plugin_ids, Vec::<String>::new());
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].reason.contains("incompatible schema"));
    }

    #[test]
    fn incompatible_major_is_skipped() {
        let mut bus = ExtensionEventBus::new();
        let register = bus.register_subscriber(subscriber(
            "plugin-a",
            PluginEventKind::PaletteActionExecuted,
            1,
            0,
            5,
        ));
        assert_eq!(register, Ok(()));
        bus.set_schema_version(
            PluginEventKind::PaletteActionExecuted,
            SchemaVersion { major: 2, minor: 0 },
        );

        let report = bus.publish(PluginEventKind::PaletteActionExecuted, BTreeMap::new(), 20);
        assert!(report.delivered_plugin_ids.is_empty());
        assert_eq!(report.skipped.len(), 1);
    }

    #[test]
    fn missing_compatibility_declaration_is_skipped() {
        let mut bus = ExtensionEventBus::new();
        let mut subscriptions = BTreeSet::new();
        subscriptions.insert(PluginEventKind::SandboxDecision);
        let plugin = PluginSubscriber {
            plugin_id: "plugin-a".to_owned(),
            subscriptions,
            compatibility: BTreeMap::new(),
        };
        let register = bus.register_subscriber(plugin);
        assert_eq!(register, Ok(()));

        let report = bus.publish(PluginEventKind::SandboxDecision, BTreeMap::new(), 0);
        assert_eq!(report.delivered_plugin_ids, Vec::<String>::new());
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0]
            .reason
            .contains("missing schema compatibility"));
    }

    #[test]
    fn drain_clears_inbox() {
        let mut bus = ExtensionEventBus::new();
        let register = bus.register_subscriber(subscriber(
            "plugin-a",
            PluginEventKind::LoopSelectionChanged,
            1,
            0,
            1,
        ));
        assert_eq!(register, Ok(()));

        let _ = bus.publish(PluginEventKind::LoopSelectionChanged, BTreeMap::new(), 1);
        assert_eq!(bus.drain_plugin_events("plugin-a").len(), 1);
        assert_eq!(bus.drain_plugin_events("plugin-a").len(), 0);
    }

    #[test]
    fn duplicate_plugin_id_rejected() {
        let mut bus = ExtensionEventBus::new();
        let first =
            bus.register_subscriber(subscriber("plugin-a", PluginEventKind::TabChanged, 1, 0, 2));
        assert_eq!(first, Ok(()));
        let second =
            bus.register_subscriber(subscriber("plugin-a", PluginEventKind::TabChanged, 1, 0, 2));
        assert_eq!(second, Err(EventBusError::DuplicatePluginId));
    }

    #[test]
    fn schema_minor_range_is_normalized() {
        let mut bus = ExtensionEventBus::new();
        let register = bus.register_subscriber(subscriber(
            "plugin-a",
            PluginEventKind::PanelLifecycle,
            1,
            4,
            2,
        ));
        assert_eq!(register, Ok(()));
        bus.set_schema_version(
            PluginEventKind::PanelLifecycle,
            SchemaVersion { major: 1, minor: 4 },
        );
        let report = bus.publish(PluginEventKind::PanelLifecycle, BTreeMap::new(), 0);
        assert_eq!(report.delivered_plugin_ids, vec!["plugin-a".to_owned()]);
    }
}
