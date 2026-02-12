//! Extension action + palette command API with validation, permissions, and audit metadata.

use std::collections::{BTreeMap, BTreeSet};

use crate::command_palette::{PaletteAction, PaletteActionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtensionPermission {
    ReadState,
    WriteState,
    ControlLoops,
    NetworkAccess,
    ExecuteShell,
}

impl ExtensionPermission {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::ReadState => "read-state",
            Self::WriteState => "write-state",
            Self::ControlLoops => "control-loops",
            Self::NetworkAccess => "network-access",
            Self::ExecuteShell => "execute-shell",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionActionAudit {
    pub registered_by: String,
    pub ticket: Option<String>,
    pub rationale: String,
    pub registered_at_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionActionSpec {
    pub extension_id: String,
    pub action_id: String,
    pub title: String,
    pub command: String,
    pub keywords: Vec<String>,
    pub requires_selection: bool,
    pub permissions: Vec<ExtensionPermission>,
    pub audit: ExtensionActionAudit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionActionRecord {
    pub key: String,
    pub palette_action_id: PaletteActionId,
    pub title: String,
    pub command: String,
    pub keywords: Vec<String>,
    pub requires_selection: bool,
    pub permissions: Vec<ExtensionPermission>,
    pub audit: ExtensionActionAudit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionAuditRow {
    pub key: String,
    pub palette_action_id: PaletteActionId,
    pub permissions: Vec<String>,
    pub registered_by: String,
    pub ticket: Option<String>,
    pub registered_at_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionActionError {
    InvalidExtensionId,
    InvalidActionId,
    InvalidTitle,
    InvalidCommand,
    DuplicateActionKey,
    CustomActionIdOverflow,
    MissingPermission {
        permission: ExtensionPermission,
        command: String,
    },
}

#[derive(Debug, Default, Clone)]
pub struct ExtensionActionRegistry {
    next_custom_id: u16,
    by_key: BTreeMap<String, ExtensionActionRecord>,
}

impl ExtensionActionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_custom_id: 1,
            by_key: BTreeMap::new(),
        }
    }

    pub fn register(
        &mut self,
        spec: ExtensionActionSpec,
    ) -> Result<PaletteActionId, ExtensionActionError> {
        let extension_id = normalize_id(&spec.extension_id);
        if extension_id.is_empty() {
            return Err(ExtensionActionError::InvalidExtensionId);
        }
        let action_id = normalize_id(&spec.action_id);
        if action_id.is_empty() {
            return Err(ExtensionActionError::InvalidActionId);
        }
        let key = format!("{extension_id}:{action_id}");
        if self.by_key.contains_key(&key) {
            return Err(ExtensionActionError::DuplicateActionKey);
        }

        let title = spec.title.trim().to_owned();
        if title.is_empty() {
            return Err(ExtensionActionError::InvalidTitle);
        }

        let command = normalize_command(&spec.command)?;
        let permissions = normalize_permissions(&spec.permissions);
        validate_command_permissions(&command, &permissions)?;

        let custom_id = self.next_custom_id;
        if custom_id == u16::MAX {
            return Err(ExtensionActionError::CustomActionIdOverflow);
        }
        self.next_custom_id = self.next_custom_id.saturating_add(1);

        let keywords = normalize_keywords(&spec.keywords);
        let audit = normalize_audit(spec.audit);
        let palette_action_id = PaletteActionId::Custom(custom_id);

        self.by_key.insert(
            key.clone(),
            ExtensionActionRecord {
                key,
                palette_action_id,
                title,
                command,
                keywords,
                requires_selection: spec.requires_selection,
                permissions,
                audit,
            },
        );
        Ok(palette_action_id)
    }

    pub fn unregister(&mut self, extension_id: &str, action_id: &str) -> bool {
        let key = format!("{}:{}", normalize_id(extension_id), normalize_id(action_id));
        self.by_key.remove(&key).is_some()
    }

    #[must_use]
    pub fn get(&self, palette_action_id: PaletteActionId) -> Option<&ExtensionActionRecord> {
        self.by_key
            .values()
            .find(|record| record.palette_action_id == palette_action_id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    #[must_use]
    pub fn palette_actions(&self) -> Vec<PaletteAction> {
        self.by_key
            .values()
            .map(|record| {
                let mut keywords = record.keywords.clone();
                keywords.extend(record.permissions.iter().map(|p| p.slug().to_owned()));
                PaletteAction {
                    id: record.palette_action_id,
                    title: record.title.clone(),
                    command: record.command.clone(),
                    keywords,
                    preferred_tab: None,
                    requires_selection: record.requires_selection,
                }
            })
            .collect()
    }

    #[must_use]
    pub fn audit_rows(&self) -> Vec<ExtensionAuditRow> {
        self.by_key
            .values()
            .map(|record| ExtensionAuditRow {
                key: record.key.clone(),
                palette_action_id: record.palette_action_id,
                permissions: record
                    .permissions
                    .iter()
                    .map(|permission| permission.slug().to_owned())
                    .collect(),
                registered_by: record.audit.registered_by.clone(),
                ticket: record.audit.ticket.clone(),
                registered_at_epoch_s: record.audit.registered_at_epoch_s,
            })
            .collect()
    }
}

fn normalize_command(value: &str) -> Result<String, ExtensionActionError> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Err(ExtensionActionError::InvalidCommand);
    }
    if value.contains("  ") {
        return Err(ExtensionActionError::InvalidCommand);
    }
    for ch in value.chars() {
        let allowed = ch.is_ascii_alphanumeric()
            || ch == ' '
            || ch == '.'
            || ch == '-'
            || ch == '_'
            || ch == ':'
            || ch == '/';
        if !allowed {
            return Err(ExtensionActionError::InvalidCommand);
        }
    }
    for blocked in [';', '|', '&', '$', '`', '>', '<', '\\'] {
        if value.contains(blocked) {
            return Err(ExtensionActionError::InvalidCommand);
        }
    }
    Ok(value)
}

fn validate_command_permissions(
    command: &str,
    permissions: &[ExtensionPermission],
) -> Result<(), ExtensionActionError> {
    if is_loop_control_command(command) && !permissions.contains(&ExtensionPermission::ControlLoops)
    {
        return Err(ExtensionActionError::MissingPermission {
            permission: ExtensionPermission::ControlLoops,
            command: command.to_owned(),
        });
    }
    if command.starts_with("exec ") && !permissions.contains(&ExtensionPermission::ExecuteShell) {
        return Err(ExtensionActionError::MissingPermission {
            permission: ExtensionPermission::ExecuteShell,
            command: command.to_owned(),
        });
    }
    if (command.contains("http://") || command.contains("https://"))
        && !permissions.contains(&ExtensionPermission::NetworkAccess)
    {
        return Err(ExtensionActionError::MissingPermission {
            permission: ExtensionPermission::NetworkAccess,
            command: command.to_owned(),
        });
    }
    Ok(())
}

fn is_loop_control_command(command: &str) -> bool {
    command.starts_with("loop stop")
        || command.starts_with("loop kill")
        || command.starts_with("loop delete")
        || command.starts_with("loop resume")
        || command.starts_with("loop new")
}

fn normalize_permissions(values: &[ExtensionPermission]) -> Vec<ExtensionPermission> {
    if values.is_empty() {
        return vec![ExtensionPermission::ReadState];
    }
    values
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_keywords(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| normalize_id(value))
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_audit(audit: ExtensionActionAudit) -> ExtensionActionAudit {
    let registered_by = if audit.registered_by.trim().is_empty() {
        "unknown".to_owned()
    } else {
        audit.registered_by.trim().to_owned()
    };
    let rationale = if audit.rationale.trim().is_empty() {
        "extension action registration".to_owned()
    } else {
        audit.rationale.trim().to_owned()
    };
    let ticket = audit
        .ticket
        .as_deref()
        .map(str::trim)
        .filter(|ticket| !ticket.is_empty())
        .map(ToOwned::to_owned);
    ExtensionActionAudit {
        registered_by,
        ticket,
        rationale,
        registered_at_epoch_s: audit.registered_at_epoch_s.max(0),
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
    use super::{
        ExtensionActionAudit, ExtensionActionError, ExtensionActionRegistry, ExtensionActionSpec,
        ExtensionPermission,
    };
    use crate::command_palette::PaletteActionId;

    fn sample_spec(command: &str) -> ExtensionActionSpec {
        ExtensionActionSpec {
            extension_id: "ops-tools".to_owned(),
            action_id: "sync-queues".to_owned(),
            title: "Sync Queues".to_owned(),
            command: command.to_owned(),
            keywords: vec!["queue".to_owned(), "ops".to_owned()],
            requires_selection: false,
            permissions: vec![ExtensionPermission::ReadState],
            audit: ExtensionActionAudit {
                registered_by: "agent@ops".to_owned(),
                ticket: Some("forge-123".to_owned()),
                rationale: "operational sync".to_owned(),
                registered_at_epoch_s: 10,
            },
        }
    }

    #[test]
    fn register_valid_action_and_emit_palette_entry() {
        let mut registry = ExtensionActionRegistry::new();
        let palette_id = registry.register(sample_spec("ext.ops-tools.sync-queues"));
        let palette_id = match palette_id {
            Ok(id) => id,
            Err(err) => panic!("expected success, got {err:?}"),
        };
        assert_eq!(palette_id, PaletteActionId::Custom(1));
        let actions = registry.palette_actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id, PaletteActionId::Custom(1));
        assert_eq!(actions[0].command, "ext.ops-tools.sync-queues");
        assert!(actions[0]
            .keywords
            .iter()
            .any(|keyword| keyword == "read-state"));
    }

    #[test]
    fn duplicate_action_key_is_rejected() {
        let mut registry = ExtensionActionRegistry::new();
        let first = registry.register(sample_spec("ext.ops-tools.sync-queues"));
        assert!(first.is_ok());
        let second = registry.register(sample_spec("ext.ops-tools.sync-queues-v2"));
        assert_eq!(second, Err(ExtensionActionError::DuplicateActionKey));
    }

    #[test]
    fn invalid_command_chars_are_rejected() {
        let mut registry = ExtensionActionRegistry::new();
        let result = registry.register(sample_spec("ext.ops-tools.sync;rm -rf /"));
        assert_eq!(result, Err(ExtensionActionError::InvalidCommand));
    }

    #[test]
    fn loop_control_requires_permission() {
        let mut spec = sample_spec("loop kill --all");
        spec.permissions = vec![ExtensionPermission::ReadState];
        let mut registry = ExtensionActionRegistry::new();
        let result = registry.register(spec);
        assert_eq!(
            result,
            Err(ExtensionActionError::MissingPermission {
                permission: ExtensionPermission::ControlLoops,
                command: "loop kill --all".to_owned(),
            })
        );
    }

    #[test]
    fn loop_control_with_permission_passes() {
        let mut spec = sample_spec("loop stop selected");
        spec.permissions = vec![
            ExtensionPermission::ReadState,
            ExtensionPermission::ControlLoops,
        ];
        let mut registry = ExtensionActionRegistry::new();
        let result = registry.register(spec);
        assert_eq!(result, Ok(PaletteActionId::Custom(1)));
    }

    #[test]
    fn audit_rows_are_deterministic() {
        let mut first = sample_spec("ext.ops-tools.sync-queues");
        first.action_id = "a-action".to_owned();
        let mut second = sample_spec("ext.ops-tools.sync-runs");
        second.action_id = "b-action".to_owned();
        second.permissions = vec![
            ExtensionPermission::ReadState,
            ExtensionPermission::ControlLoops,
        ];
        second.audit.registered_by = "auditor".to_owned();
        second.audit.registered_at_epoch_s = 99;

        let mut registry = ExtensionActionRegistry::new();
        let first_result = registry.register(first);
        let second_result = registry.register(second);
        assert!(first_result.is_ok());
        assert!(second_result.is_ok());

        let rows = registry.audit_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].key, "ops-tools:a-action");
        assert_eq!(rows[1].key, "ops-tools:b-action");
        assert!(rows[1]
            .permissions
            .iter()
            .any(|value| value == "control-loops"));
    }

    #[test]
    fn unregister_removes_action() {
        let mut registry = ExtensionActionRegistry::new();
        let result = registry.register(sample_spec("ext.ops-tools.sync-queues"));
        assert!(result.is_ok());
        assert_eq!(registry.len(), 1);
        assert!(registry.unregister("ops_tools", "sync queues"));
        assert_eq!(registry.len(), 0);
    }
}
