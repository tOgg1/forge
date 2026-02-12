//! Plugin packaging, discovery, signature verification, and lifecycle controls.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::extension_actions::ExtensionPermission;
use crate::extension_event_bus::SchemaVersion;

pub const PLUGIN_PACKAGE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub entrypoint: String,
    pub required_permissions: Vec<ExtensionPermission>,
    pub min_host_api: SchemaVersion,
    pub max_host_api: SchemaVersion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginArtifact {
    pub path: String,
    pub digest: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSignature {
    pub signer: String,
    pub algorithm: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginPackage {
    pub schema_version: u32,
    pub manifest: PluginManifest,
    pub artifacts: Vec<PluginArtifact>,
    pub signature: PluginSignature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginLifecycleState {
    Discovered,
    Installed,
    Enabled,
    Running,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPlugin {
    pub package: PluginPackage,
    pub state: PluginLifecycleState,
    pub discovered_at_epoch_s: i64,
    pub installed_at_epoch_s: Option<i64>,
    pub enabled_at_epoch_s: Option<i64>,
    pub running_at_epoch_s: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginLifecycleAction {
    Discovered,
    Installed,
    Uninstalled,
    Enabled,
    Disabled,
    Started,
    Stopped,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLifecycleEvent {
    pub plugin_id: String,
    pub action: PluginLifecycleAction,
    pub at_epoch_s: i64,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginManagerError {
    InvalidPackageSchema,
    InvalidPluginId,
    InvalidPackage,
    UntrustedSigner,
    SignatureMismatch,
    HostIncompatible,
    AlreadyExists,
    NotFound,
    InvalidStateTransition,
}

#[derive(Debug, Clone)]
pub struct ExtensionPackageManager {
    host_api_version: SchemaVersion,
    trusted_signers: BTreeMap<String, String>,
    plugins: BTreeMap<String, ManagedPlugin>,
    lifecycle_events: Vec<PluginLifecycleEvent>,
}

impl ExtensionPackageManager {
    #[must_use]
    pub fn new(host_api_version: SchemaVersion, trusted_signers: BTreeMap<String, String>) -> Self {
        let trusted_signers = trusted_signers
            .into_iter()
            .map(|(signer, key)| (normalize_id(&signer), key.trim().to_owned()))
            .filter(|(signer, key)| !signer.is_empty() && !key.is_empty())
            .collect();
        Self {
            host_api_version,
            trusted_signers,
            plugins: BTreeMap::new(),
            lifecycle_events: Vec::new(),
        }
    }

    pub fn discover_package(
        &mut self,
        package: PluginPackage,
        now_epoch_s: i64,
    ) -> Result<(), PluginManagerError> {
        validate_package_shape(&package)?;
        verify_package_signature(&package, &self.trusted_signers)?;
        ensure_host_compatibility(
            package.manifest.min_host_api,
            package.manifest.max_host_api,
            self.host_api_version,
        )?;

        let plugin_id = normalize_id(&package.manifest.plugin_id);
        if self.plugins.contains_key(&plugin_id) {
            return Err(PluginManagerError::AlreadyExists);
        }
        self.plugins.insert(
            plugin_id.clone(),
            ManagedPlugin {
                package,
                state: PluginLifecycleState::Discovered,
                discovered_at_epoch_s: now_epoch_s.max(0),
                installed_at_epoch_s: None,
                enabled_at_epoch_s: None,
                running_at_epoch_s: None,
                last_error: None,
            },
        );
        self.record_event(
            &plugin_id,
            PluginLifecycleAction::Discovered,
            now_epoch_s,
            "package discovered and verified".to_owned(),
        );
        Ok(())
    }

    pub fn install(&mut self, plugin_id: &str, now_epoch_s: i64) -> Result<(), PluginManagerError> {
        let plugin_id = normalize_id(plugin_id);
        let plugin = self
            .plugins
            .get_mut(&plugin_id)
            .ok_or(PluginManagerError::NotFound)?;
        if plugin.state != PluginLifecycleState::Discovered {
            return Err(PluginManagerError::InvalidStateTransition);
        }
        plugin.state = PluginLifecycleState::Installed;
        plugin.installed_at_epoch_s = Some(now_epoch_s.max(0));
        self.record_event(
            &plugin_id,
            PluginLifecycleAction::Installed,
            now_epoch_s,
            "plugin installed".to_owned(),
        );
        Ok(())
    }

    pub fn set_enabled(
        &mut self,
        plugin_id: &str,
        enabled: bool,
        now_epoch_s: i64,
    ) -> Result<(), PluginManagerError> {
        let plugin_id = normalize_id(plugin_id);
        let plugin = self
            .plugins
            .get_mut(&plugin_id)
            .ok_or(PluginManagerError::NotFound)?;
        match (enabled, plugin.state) {
            (true, PluginLifecycleState::Installed) | (true, PluginLifecycleState::Discovered) => {
                plugin.state = PluginLifecycleState::Enabled;
                plugin.enabled_at_epoch_s = Some(now_epoch_s.max(0));
                self.record_event(
                    &plugin_id,
                    PluginLifecycleAction::Enabled,
                    now_epoch_s,
                    "plugin enabled".to_owned(),
                );
                Ok(())
            }
            (false, PluginLifecycleState::Enabled) | (false, PluginLifecycleState::Running) => {
                plugin.state = PluginLifecycleState::Installed;
                plugin.running_at_epoch_s = None;
                self.record_event(
                    &plugin_id,
                    PluginLifecycleAction::Disabled,
                    now_epoch_s,
                    "plugin disabled".to_owned(),
                );
                Ok(())
            }
            _ => Err(PluginManagerError::InvalidStateTransition),
        }
    }

    pub fn set_running(
        &mut self,
        plugin_id: &str,
        running: bool,
        now_epoch_s: i64,
    ) -> Result<(), PluginManagerError> {
        let plugin_id = normalize_id(plugin_id);
        let plugin = self
            .plugins
            .get_mut(&plugin_id)
            .ok_or(PluginManagerError::NotFound)?;
        match (running, plugin.state) {
            (true, PluginLifecycleState::Enabled) => {
                plugin.state = PluginLifecycleState::Running;
                plugin.running_at_epoch_s = Some(now_epoch_s.max(0));
                self.record_event(
                    &plugin_id,
                    PluginLifecycleAction::Started,
                    now_epoch_s,
                    "plugin runtime started".to_owned(),
                );
                Ok(())
            }
            (false, PluginLifecycleState::Running) => {
                plugin.state = PluginLifecycleState::Enabled;
                plugin.running_at_epoch_s = None;
                self.record_event(
                    &plugin_id,
                    PluginLifecycleAction::Stopped,
                    now_epoch_s,
                    "plugin runtime stopped".to_owned(),
                );
                Ok(())
            }
            _ => Err(PluginManagerError::InvalidStateTransition),
        }
    }

    pub fn uninstall(
        &mut self,
        plugin_id: &str,
        now_epoch_s: i64,
    ) -> Result<(), PluginManagerError> {
        let plugin_id = normalize_id(plugin_id);
        if self.plugins.remove(&plugin_id).is_none() {
            return Err(PluginManagerError::NotFound);
        }
        self.record_event(
            &plugin_id,
            PluginLifecycleAction::Uninstalled,
            now_epoch_s,
            "plugin uninstalled".to_owned(),
        );
        Ok(())
    }

    #[must_use]
    pub fn plugin(&self, plugin_id: &str) -> Option<&ManagedPlugin> {
        self.plugins.get(&normalize_id(plugin_id))
    }

    #[must_use]
    pub fn lifecycle_events(&self) -> &[PluginLifecycleEvent] {
        &self.lifecycle_events
    }

    fn record_event(
        &mut self,
        plugin_id: &str,
        action: PluginLifecycleAction,
        at_epoch_s: i64,
        detail: String,
    ) {
        self.lifecycle_events.push(PluginLifecycleEvent {
            plugin_id: plugin_id.to_owned(),
            action,
            at_epoch_s: at_epoch_s.max(0),
            detail,
        });
    }
}

#[must_use]
pub fn encode_plugin_package(package: &PluginPackage) -> String {
    let mut root = Map::new();
    root.insert(
        "schema_version".to_owned(),
        Value::from(package.schema_version),
    );
    root.insert(
        "manifest".to_owned(),
        Value::Object(encode_manifest(&package.manifest)),
    );
    root.insert(
        "artifacts".to_owned(),
        Value::Array(
            package
                .artifacts
                .iter()
                .map(|artifact| {
                    let mut obj = Map::new();
                    obj.insert("path".to_owned(), Value::from(artifact.path.clone()));
                    obj.insert("digest".to_owned(), Value::from(artifact.digest.clone()));
                    obj.insert("size_bytes".to_owned(), Value::from(artifact.size_bytes));
                    Value::Object(obj)
                })
                .collect(),
        ),
    );
    let mut signature = Map::new();
    signature.insert(
        "signer".to_owned(),
        Value::from(package.signature.signer.clone()),
    );
    signature.insert(
        "algorithm".to_owned(),
        Value::from(package.signature.algorithm.clone()),
    );
    signature.insert(
        "value".to_owned(),
        Value::from(package.signature.value.clone()),
    );
    root.insert("signature".to_owned(), Value::Object(signature));

    match serde_json::to_string_pretty(&Value::Object(root)) {
        Ok(json) => json,
        Err(_) => "{}".to_owned(),
    }
}

pub fn decode_plugin_package(raw: &str) -> Result<PluginPackage, PluginManagerError> {
    let value =
        serde_json::from_str::<Value>(raw).map_err(|_| PluginManagerError::InvalidPackage)?;
    let obj = value
        .as_object()
        .ok_or(PluginManagerError::InvalidPackage)?;
    let schema_version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or(PluginManagerError::InvalidPackage)? as u32;
    let manifest = decode_manifest(
        obj.get("manifest")
            .ok_or(PluginManagerError::InvalidPackage)?,
    )?;
    let artifacts = decode_artifacts(
        obj.get("artifacts")
            .ok_or(PluginManagerError::InvalidPackage)?,
    )?;
    let signature = decode_signature(
        obj.get("signature")
            .ok_or(PluginManagerError::InvalidPackage)?,
    )?;
    Ok(PluginPackage {
        schema_version,
        manifest,
        artifacts,
        signature,
    })
}

fn validate_package_shape(package: &PluginPackage) -> Result<(), PluginManagerError> {
    if package.schema_version != PLUGIN_PACKAGE_SCHEMA_VERSION {
        return Err(PluginManagerError::InvalidPackageSchema);
    }
    if normalize_id(&package.manifest.plugin_id).is_empty() {
        return Err(PluginManagerError::InvalidPluginId);
    }
    if package.manifest.version.trim().is_empty()
        || package.manifest.entrypoint.trim().is_empty()
        || package.signature.signer.trim().is_empty()
        || package.signature.value.trim().is_empty()
    {
        return Err(PluginManagerError::InvalidPackage);
    }
    if package.artifacts.is_empty() {
        return Err(PluginManagerError::InvalidPackage);
    }
    Ok(())
}

fn verify_package_signature(
    package: &PluginPackage,
    trusted_signers: &BTreeMap<String, String>,
) -> Result<(), PluginManagerError> {
    let signer = normalize_id(&package.signature.signer);
    let Some(key) = trusted_signers.get(&signer) else {
        return Err(PluginManagerError::UntrustedSigner);
    };
    let expected = compute_signature(package, key);
    if package.signature.value == expected {
        Ok(())
    } else {
        Err(PluginManagerError::SignatureMismatch)
    }
}

fn ensure_host_compatibility(
    min_host: SchemaVersion,
    max_host: SchemaVersion,
    host: SchemaVersion,
) -> Result<(), PluginManagerError> {
    let min_ok = host.major > min_host.major
        || (host.major == min_host.major && host.minor >= min_host.minor);
    let max_ok = host.major < max_host.major
        || (host.major == max_host.major && host.minor <= max_host.minor);
    if min_ok && max_ok {
        Ok(())
    } else {
        Err(PluginManagerError::HostIncompatible)
    }
}

#[must_use]
pub fn sign_plugin_package(mut package: PluginPackage, signer_key: &str) -> PluginPackage {
    package.signature.value = compute_signature(&package, signer_key);
    package
}

fn compute_signature(package: &PluginPackage, signer_key: &str) -> String {
    let mut hash = 1469598103934665603_u64;
    for byte in canonical_signature_payload(package, signer_key).as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211_u64);
    }
    format!("{hash:016x}")
}

fn canonical_signature_payload(package: &PluginPackage, signer_key: &str) -> String {
    let manifest = &package.manifest;
    let mut permissions = manifest
        .required_permissions
        .iter()
        .map(|permission| format!("{permission:?}"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    permissions.sort();

    let mut artifacts = package
        .artifacts
        .iter()
        .map(|artifact| {
            format!(
                "{}:{}:{}",
                normalize_id(&artifact.path),
                artifact.digest.trim().to_ascii_lowercase(),
                artifact.size_bytes
            )
        })
        .collect::<Vec<_>>();
    artifacts.sort();

    format!(
        "{}|{}|{}|{}|{}|{}|{}.{}|{}.{}|{}|{}",
        normalize_id(&manifest.plugin_id),
        manifest.name.trim(),
        manifest.version.trim(),
        manifest.description.trim(),
        normalize_id(&manifest.entrypoint),
        permissions.join(","),
        manifest.min_host_api.major,
        manifest.min_host_api.minor,
        manifest.max_host_api.major,
        manifest.max_host_api.minor,
        artifacts.join(";"),
        signer_key.trim()
    )
}

fn encode_manifest(manifest: &PluginManifest) -> Map<String, Value> {
    let mut obj = Map::new();
    obj.insert(
        "plugin_id".to_owned(),
        Value::from(manifest.plugin_id.clone()),
    );
    obj.insert("name".to_owned(), Value::from(manifest.name.clone()));
    obj.insert("version".to_owned(), Value::from(manifest.version.clone()));
    obj.insert(
        "description".to_owned(),
        Value::from(manifest.description.clone()),
    );
    obj.insert(
        "entrypoint".to_owned(),
        Value::from(manifest.entrypoint.clone()),
    );
    obj.insert(
        "required_permissions".to_owned(),
        Value::Array(
            manifest
                .required_permissions
                .iter()
                .map(|permission| Value::from(format!("{permission:?}")))
                .collect(),
        ),
    );
    obj.insert(
        "min_host_api".to_owned(),
        Value::from(manifest.min_host_api.label()),
    );
    obj.insert(
        "max_host_api".to_owned(),
        Value::from(manifest.max_host_api.label()),
    );
    obj
}

fn decode_manifest(value: &Value) -> Result<PluginManifest, PluginManagerError> {
    let obj = value
        .as_object()
        .ok_or(PluginManagerError::InvalidPackage)?;
    Ok(PluginManifest {
        plugin_id: obj
            .get("plugin_id")
            .and_then(Value::as_str)
            .ok_or(PluginManagerError::InvalidPackage)?
            .to_owned(),
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .ok_or(PluginManagerError::InvalidPackage)?
            .to_owned(),
        version: obj
            .get("version")
            .and_then(Value::as_str)
            .ok_or(PluginManagerError::InvalidPackage)?
            .to_owned(),
        description: obj
            .get("description")
            .and_then(Value::as_str)
            .ok_or(PluginManagerError::InvalidPackage)?
            .to_owned(),
        entrypoint: obj
            .get("entrypoint")
            .and_then(Value::as_str)
            .ok_or(PluginManagerError::InvalidPackage)?
            .to_owned(),
        required_permissions: decode_permissions(
            obj.get("required_permissions")
                .ok_or(PluginManagerError::InvalidPackage)?,
        )?,
        min_host_api: parse_schema(
            obj.get("min_host_api")
                .and_then(Value::as_str)
                .ok_or(PluginManagerError::InvalidPackage)?,
        )?,
        max_host_api: parse_schema(
            obj.get("max_host_api")
                .and_then(Value::as_str)
                .ok_or(PluginManagerError::InvalidPackage)?,
        )?,
    })
}

fn decode_artifacts(value: &Value) -> Result<Vec<PluginArtifact>, PluginManagerError> {
    let items = value.as_array().ok_or(PluginManagerError::InvalidPackage)?;
    let mut artifacts = Vec::new();
    for item in items {
        let obj = item.as_object().ok_or(PluginManagerError::InvalidPackage)?;
        artifacts.push(PluginArtifact {
            path: obj
                .get("path")
                .and_then(Value::as_str)
                .ok_or(PluginManagerError::InvalidPackage)?
                .to_owned(),
            digest: obj
                .get("digest")
                .and_then(Value::as_str)
                .ok_or(PluginManagerError::InvalidPackage)?
                .to_owned(),
            size_bytes: obj
                .get("size_bytes")
                .and_then(Value::as_u64)
                .ok_or(PluginManagerError::InvalidPackage)?,
        });
    }
    Ok(artifacts)
}

fn decode_signature(value: &Value) -> Result<PluginSignature, PluginManagerError> {
    let obj = value
        .as_object()
        .ok_or(PluginManagerError::InvalidPackage)?;
    Ok(PluginSignature {
        signer: obj
            .get("signer")
            .and_then(Value::as_str)
            .ok_or(PluginManagerError::InvalidPackage)?
            .to_owned(),
        algorithm: obj
            .get("algorithm")
            .and_then(Value::as_str)
            .ok_or(PluginManagerError::InvalidPackage)?
            .to_owned(),
        value: obj
            .get("value")
            .and_then(Value::as_str)
            .ok_or(PluginManagerError::InvalidPackage)?
            .to_owned(),
    })
}

fn decode_permissions(value: &Value) -> Result<Vec<ExtensionPermission>, PluginManagerError> {
    let items = value.as_array().ok_or(PluginManagerError::InvalidPackage)?;
    let mut permissions = BTreeSet::new();
    for item in items {
        let label = item.as_str().ok_or(PluginManagerError::InvalidPackage)?;
        let permission = match label {
            "ReadState" => ExtensionPermission::ReadState,
            "WriteState" => ExtensionPermission::WriteState,
            "ControlLoops" => ExtensionPermission::ControlLoops,
            "NetworkAccess" => ExtensionPermission::NetworkAccess,
            "ExecuteShell" => ExtensionPermission::ExecuteShell,
            _ => return Err(PluginManagerError::InvalidPackage),
        };
        permissions.insert(permission);
    }
    Ok(permissions.into_iter().collect())
}

fn parse_schema(value: &str) -> Result<SchemaVersion, PluginManagerError> {
    let mut parts = value.trim().split('.');
    let major = parts
        .next()
        .ok_or(PluginManagerError::InvalidPackage)?
        .parse::<u16>()
        .map_err(|_| PluginManagerError::InvalidPackage)?;
    let minor = parts
        .next()
        .ok_or(PluginManagerError::InvalidPackage)?
        .parse::<u16>()
        .map_err(|_| PluginManagerError::InvalidPackage)?;
    if parts.next().is_some() {
        return Err(PluginManagerError::InvalidPackage);
    }
    Ok(SchemaVersion { major, minor })
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
    use std::collections::BTreeMap;

    use super::{
        decode_plugin_package, encode_plugin_package, sign_plugin_package, ExtensionPackageManager,
        PluginArtifact, PluginLifecycleAction, PluginLifecycleState, PluginManagerError,
        PluginManifest, PluginPackage, PluginSignature, SchemaVersion,
    };
    use crate::extension_actions::ExtensionPermission;

    fn sample_package() -> PluginPackage {
        PluginPackage {
            schema_version: 1,
            manifest: PluginManifest {
                plugin_id: "plugin-alpha".to_owned(),
                name: "Plugin Alpha".to_owned(),
                version: "1.0.0".to_owned(),
                description: "demo plugin".to_owned(),
                entrypoint: "alpha-main".to_owned(),
                required_permissions: vec![ExtensionPermission::ReadState],
                min_host_api: SchemaVersion { major: 1, minor: 0 },
                max_host_api: SchemaVersion { major: 1, minor: 9 },
            },
            artifacts: vec![PluginArtifact {
                path: "plugin.wasm".to_owned(),
                digest: "abc".to_owned(),
                size_bytes: 42,
            }],
            signature: PluginSignature {
                signer: "forge-team".to_owned(),
                algorithm: "forge-hash-v1".to_owned(),
                value: String::new(),
            },
        }
    }

    fn manager() -> ExtensionPackageManager {
        let mut signers = BTreeMap::new();
        signers.insert("forge-team".to_owned(), "trusted-key".to_owned());
        ExtensionPackageManager::new(SchemaVersion { major: 1, minor: 2 }, signers)
    }

    #[test]
    fn encode_decode_round_trip() {
        let signed = sign_plugin_package(sample_package(), "trusted-key");
        let encoded = encode_plugin_package(&signed);
        let decoded = decode_plugin_package(&encoded);
        let decoded = match decoded {
            Ok(value) => value,
            Err(err) => panic!("expected decoded package, got {err:?}"),
        };
        assert_eq!(decoded.manifest.plugin_id, "plugin-alpha");
        assert_eq!(decoded.signature.signer, "forge-team");
    }

    #[test]
    fn discover_rejects_untrusted_signer() {
        let mut manager = manager();
        let mut package = sign_plugin_package(sample_package(), "trusted-key");
        package.signature.signer = "unknown".to_owned();
        let result = manager.discover_package(package, 10);
        assert_eq!(result, Err(PluginManagerError::UntrustedSigner));
    }

    #[test]
    fn discover_rejects_signature_mismatch() {
        let mut manager = manager();
        let mut package = sign_plugin_package(sample_package(), "trusted-key");
        package.signature.value = "bad".to_owned();
        let result = manager.discover_package(package, 10);
        assert_eq!(result, Err(PluginManagerError::SignatureMismatch));
    }

    #[test]
    fn discover_rejects_incompatible_host_version() {
        let mut manager = manager();
        let mut package = sample_package();
        package.manifest.min_host_api = SchemaVersion { major: 2, minor: 0 };
        let package = sign_plugin_package(package, "trusted-key");
        let result = manager.discover_package(package, 0);
        assert_eq!(result, Err(PluginManagerError::HostIncompatible));
    }

    #[test]
    fn lifecycle_install_enable_start_stop_uninstall() {
        let mut manager = manager();
        let package = sign_plugin_package(sample_package(), "trusted-key");
        let discover = manager.discover_package(package, 10);
        assert_eq!(discover, Ok(()));

        let install = manager.install("plugin-alpha", 11);
        assert_eq!(install, Ok(()));
        let plugin = manager.plugin("plugin-alpha");
        match plugin {
            Some(plugin) => assert_eq!(plugin.state, PluginLifecycleState::Installed),
            None => panic!("expected installed plugin"),
        }

        let enable = manager.set_enabled("plugin-alpha", true, 12);
        assert_eq!(enable, Ok(()));
        let start = manager.set_running("plugin-alpha", true, 13);
        assert_eq!(start, Ok(()));
        let stop = manager.set_running("plugin-alpha", false, 14);
        assert_eq!(stop, Ok(()));
        let disable = manager.set_enabled("plugin-alpha", false, 15);
        assert_eq!(disable, Ok(()));
        let uninstall = manager.uninstall("plugin-alpha", 16);
        assert_eq!(uninstall, Ok(()));
        assert_eq!(manager.plugin("plugin-alpha"), None);

        let actions = manager
            .lifecycle_events()
            .iter()
            .map(|event| event.action.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            actions,
            vec![
                PluginLifecycleAction::Discovered,
                PluginLifecycleAction::Installed,
                PluginLifecycleAction::Enabled,
                PluginLifecycleAction::Started,
                PluginLifecycleAction::Stopped,
                PluginLifecycleAction::Disabled,
                PluginLifecycleAction::Uninstalled,
            ]
        );
    }

    #[test]
    fn start_requires_enabled_state() {
        let mut manager = manager();
        let package = sign_plugin_package(sample_package(), "trusted-key");
        let discover = manager.discover_package(package, 10);
        assert_eq!(discover, Ok(()));
        let install = manager.install("plugin-alpha", 11);
        assert_eq!(install, Ok(()));

        let start = manager.set_running("plugin-alpha", true, 12);
        assert_eq!(start, Err(PluginManagerError::InvalidStateTransition));
    }

    #[test]
    fn duplicate_discovery_rejected() {
        let mut manager = manager();
        let package = sign_plugin_package(sample_package(), "trusted-key");
        let first = manager.discover_package(package.clone(), 10);
        let second = manager.discover_package(package, 12);
        assert_eq!(first, Ok(()));
        assert_eq!(second, Err(PluginManagerError::AlreadyExists));
    }
}
