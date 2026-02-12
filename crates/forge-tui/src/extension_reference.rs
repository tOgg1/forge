//! Reference plugins + extension developer docs to accelerate safe ecosystem adoption.

use crate::extension_actions::ExtensionPermission;
use crate::extension_event_bus::SchemaVersion;
use crate::extension_package_manager::{
    sign_plugin_package, PluginArtifact, PluginManifest, PluginPackage, PluginSignature,
    PLUGIN_PACKAGE_SCHEMA_VERSION,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencePluginSpec {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub entrypoint: String,
    pub use_case: String,
    pub required_permissions: Vec<ExtensionPermission>,
    pub safe_defaults: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencePluginDoc {
    pub plugin_id: String,
    pub use_case: String,
    pub safe_defaults: Vec<String>,
    pub permission_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencePluginBundle {
    pub packages: Vec<PluginPackage>,
    pub docs: Vec<ReferencePluginDoc>,
    pub developer_markdown: String,
}

#[must_use]
pub fn reference_plugin_specs() -> Vec<ReferencePluginSpec> {
    vec![
        ReferencePluginSpec {
            plugin_id: "loop-health-inspector".to_owned(),
            name: "Loop Health Inspector".to_owned(),
            version: "1.0.0".to_owned(),
            description: "Read-only loop health panel with fleet-safe diagnostics.".to_owned(),
            entrypoint: "panels.loop_health".to_owned(),
            use_case: "Example read-only panel plugin for safe observability extensions."
                .to_owned(),
            required_permissions: vec![ExtensionPermission::ReadState],
            safe_defaults: vec![
                "read-only mode".to_owned(),
                "no network calls".to_owned(),
                "no shell execution".to_owned(),
            ],
        },
        ReferencePluginSpec {
            plugin_id: "safe-control-center".to_owned(),
            name: "Safe Control Center".to_owned(),
            version: "1.0.0".to_owned(),
            description: "Curated remediation actions with explicit loop-control permissions."
                .to_owned(),
            entrypoint: "actions.safe_control".to_owned(),
            use_case: "Example command/action plugin with constrained control scope.".to_owned(),
            required_permissions: vec![
                ExtensionPermission::ReadState,
                ExtensionPermission::ControlLoops,
            ],
            safe_defaults: vec![
                "actions require explicit operator selection".to_owned(),
                "no write-state mutations".to_owned(),
                "audit rows required for every action".to_owned(),
            ],
        },
        ReferencePluginSpec {
            plugin_id: "inbox-notes-assistant".to_owned(),
            name: "Inbox Notes Assistant".to_owned(),
            version: "1.0.0".to_owned(),
            description: "Writes structured breadcrumbs to task notes with strict audit metadata."
                .to_owned(),
            entrypoint: "actions.inbox_notes".to_owned(),
            use_case: "Example write-state plugin with explicit risk controls and rollback notes."
                .to_owned(),
            required_permissions: vec![
                ExtensionPermission::ReadState,
                ExtensionPermission::WriteState,
            ],
            safe_defaults: vec![
                "writes only to scoped note fields".to_owned(),
                "every mutation includes actor + ticket".to_owned(),
                "rollback command documented in plugin README".to_owned(),
            ],
        },
    ]
}

#[must_use]
pub fn build_reference_plugin_bundle(
    host_api_version: SchemaVersion,
    signer: &str,
    signer_key: &str,
) -> ReferencePluginBundle {
    let specs = sorted_specs(reference_plugin_specs());
    let packages: Vec<PluginPackage> = specs
        .iter()
        .map(|spec| build_signed_package(spec, host_api_version, signer, signer_key))
        .collect();
    let docs = specs
        .iter()
        .map(|spec| ReferencePluginDoc {
            plugin_id: spec.plugin_id.clone(),
            use_case: spec.use_case.clone(),
            safe_defaults: spec.safe_defaults.clone(),
            permission_warnings: permission_safety_warnings(&spec.required_permissions),
        })
        .collect::<Vec<_>>();
    let developer_markdown = render_extension_developer_guide(&specs, &docs);

    ReferencePluginBundle {
        packages,
        docs,
        developer_markdown,
    }
}

#[must_use]
pub fn permission_safety_warnings(permissions: &[ExtensionPermission]) -> Vec<String> {
    let mut warnings = Vec::new();
    for permission in permissions {
        match permission {
            ExtensionPermission::WriteState => warnings.push(
                "write-state requires audit trail, idempotent updates, and rollback guidance"
                    .to_owned(),
            ),
            ExtensionPermission::NetworkAccess => warnings.push(
                "network-access requires explicit allowlist, timeout budget, and retry limits"
                    .to_owned(),
            ),
            ExtensionPermission::ExecuteShell => warnings.push(
                "execute-shell should be avoided in reference plugins; prefer typed host actions"
                    .to_owned(),
            ),
            ExtensionPermission::ReadState | ExtensionPermission::ControlLoops => {}
        }
    }
    warnings
}

#[must_use]
pub fn render_extension_developer_guide(
    specs: &[ReferencePluginSpec],
    docs: &[ReferencePluginDoc],
) -> String {
    let mut lines = vec![
        "# Forge TUI extension developer reference".to_owned(),
        "".to_owned(),
        "## Quickstart lifecycle".to_owned(),
        "1. Package plugin using PluginPackage/PluginManifest contracts.".to_owned(),
        "2. Sign package with trusted signer and submit for discovery.".to_owned(),
        "3. Install, enable, and start only after permission review.".to_owned(),
        "4. Record lifecycle and action audit events for every state transition.".to_owned(),
        "".to_owned(),
        "## Reference plugins".to_owned(),
    ];

    for spec in specs {
        let warnings = docs
            .iter()
            .find(|doc| doc.plugin_id == spec.plugin_id)
            .map(|doc| doc.permission_warnings.clone())
            .unwrap_or_default();

        lines.push("".to_owned());
        lines.push(format!("### {}", spec.plugin_id));
        lines.push(format!("- name: {}", spec.name));
        lines.push(format!("- entrypoint: {}", spec.entrypoint));
        lines.push(format!("- use case: {}", spec.use_case));
        lines.push(format!(
            "- permissions: {}",
            spec.required_permissions
                .iter()
                .map(|permission| permission.slug().to_owned())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        lines.push("- safe defaults:".to_owned());
        for item in &spec.safe_defaults {
            lines.push(format!("  - {item}"));
        }
        if !warnings.is_empty() {
            lines.push("- risk notes:".to_owned());
            for warning in warnings {
                lines.push(format!("  - {warning}"));
            }
        }
    }

    lines.push("".to_owned());
    lines.push("## Unsafe pattern checklist".to_owned());
    lines.push("- Avoid execute-shell unless no typed host capability exists.".to_owned());
    lines.push("- Keep network-access behind explicit allowlist + timeout budgets.".to_owned());
    lines.push("- Treat write-state as privileged; require ticketed audit metadata.".to_owned());

    lines.join("\n")
}

fn build_signed_package(
    spec: &ReferencePluginSpec,
    host_api_version: SchemaVersion,
    signer: &str,
    signer_key: &str,
) -> PluginPackage {
    let package = PluginPackage {
        schema_version: PLUGIN_PACKAGE_SCHEMA_VERSION,
        manifest: PluginManifest {
            plugin_id: spec.plugin_id.clone(),
            name: spec.name.clone(),
            version: spec.version.clone(),
            description: spec.description.clone(),
            entrypoint: spec.entrypoint.clone(),
            required_permissions: spec.required_permissions.clone(),
            min_host_api: SchemaVersion {
                major: host_api_version.major,
                minor: host_api_version.minor.saturating_sub(1),
            },
            max_host_api: SchemaVersion {
                major: host_api_version.major,
                minor: host_api_version.minor.saturating_add(8),
            },
        },
        artifacts: vec![
            PluginArtifact {
                path: format!("plugins/{}/plugin.wasm", spec.plugin_id),
                digest: format!("sha256:{}:wasm", spec.plugin_id),
                size_bytes: 65_536,
            },
            PluginArtifact {
                path: format!("plugins/{}/README.md", spec.plugin_id),
                digest: format!("sha256:{}:docs", spec.plugin_id),
                size_bytes: 2_048,
            },
        ],
        signature: PluginSignature {
            signer: signer.to_owned(),
            algorithm: "forge-hash-v1".to_owned(),
            value: String::new(),
        },
    };

    sign_plugin_package(package, signer_key)
}

fn sorted_specs(mut specs: Vec<ReferencePluginSpec>) -> Vec<ReferencePluginSpec> {
    specs.sort_by(|a, b| a.plugin_id.cmp(&b.plugin_id));
    specs
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::extension_package_manager::ExtensionPackageManager;

    use super::{
        build_reference_plugin_bundle, permission_safety_warnings, reference_plugin_specs,
        ExtensionPermission, SchemaVersion,
    };

    #[test]
    fn bundle_contains_sorted_signed_packages() {
        let bundle =
            build_reference_plugin_bundle(SchemaVersion { major: 1, minor: 2 }, "forge-team", "k");

        assert_eq!(bundle.packages.len(), 3);
        assert_eq!(
            bundle
                .packages
                .iter()
                .map(|pkg| pkg.manifest.plugin_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "inbox-notes-assistant",
                "loop-health-inspector",
                "safe-control-center"
            ]
        );
        assert!(bundle
            .packages
            .iter()
            .all(|pkg| !pkg.signature.value.is_empty()));
    }

    #[test]
    fn bundle_packages_discover_with_package_manager() {
        let bundle =
            build_reference_plugin_bundle(SchemaVersion { major: 1, minor: 2 }, "forge-team", "k");

        let mut trusted_signers = BTreeMap::new();
        trusted_signers.insert("forge-team".to_owned(), "k".to_owned());
        let mut manager =
            ExtensionPackageManager::new(SchemaVersion { major: 1, minor: 2 }, trusted_signers);

        for package in bundle.packages {
            let result = manager.discover_package(package, 42);
            assert!(result.is_ok());
        }

        assert!(manager.plugin("loop-health-inspector").is_some());
        assert!(manager.plugin("safe-control-center").is_some());
        assert!(manager.plugin("inbox-notes-assistant").is_some());
    }

    #[test]
    fn developer_markdown_lists_references_and_safety_checklist() {
        let bundle =
            build_reference_plugin_bundle(SchemaVersion { major: 1, minor: 2 }, "forge-team", "k");

        assert!(bundle
            .developer_markdown
            .contains("## Quickstart lifecycle"));
        assert!(bundle
            .developer_markdown
            .contains("### loop-health-inspector"));
        assert!(bundle
            .developer_markdown
            .contains("### safe-control-center"));
        assert!(bundle
            .developer_markdown
            .contains("### inbox-notes-assistant"));
        assert!(bundle
            .developer_markdown
            .contains("## Unsafe pattern checklist"));
        assert!(bundle
            .developer_markdown
            .contains("write-state requires audit trail"));
    }

    #[test]
    fn safety_warnings_flag_high_risk_permissions() {
        let warnings = permission_safety_warnings(&[
            ExtensionPermission::ReadState,
            ExtensionPermission::ExecuteShell,
            ExtensionPermission::NetworkAccess,
        ]);

        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("execute-shell"));
        assert!(warnings[1].contains("network-access"));
    }

    #[test]
    fn reference_specs_count_is_stable() {
        assert_eq!(reference_plugin_specs().len(), 3);
    }
}
