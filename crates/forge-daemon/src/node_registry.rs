use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRegistration {
    pub node_id: String,
    pub endpoint: String,
    pub auth_token: String,
    pub registry_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MeshRegistry {
    schema_version: u32,
    updated_at: String,
    mesh_id: String,
    master_node_id: Option<String>,
    nodes: BTreeMap<String, MeshNodeRecord>,
}

impl Default for MeshRegistry {
    fn default() -> Self {
        Self {
            schema_version: 1,
            updated_at: now_rfc3339(),
            mesh_id: "local-mesh".to_string(),
            master_node_id: None,
            nodes: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
struct MeshNodeRecord {
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    auth_token: String,
}

pub fn register_local_node(
    data_dir: &Path,
    node_id: &str,
    endpoint: &str,
    auth_token_override: Option<&str>,
) -> Result<NodeRegistration, String> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Err("node id is required for registration".to_string());
    }
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err("endpoint is required for registration".to_string());
    }

    let path = data_dir.join("mesh").join("registry.json");
    let mut registry = load_registry(&path)?;

    let entry = registry.nodes.entry(node_id.to_string()).or_default();
    entry.endpoint = endpoint.to_string();

    let selected_token = auth_token_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            let existing = entry.auth_token.trim();
            if existing.is_empty() {
                None
            } else {
                Some(existing.to_string())
            }
        })
        .unwrap_or_else(|| format!("nt_{}", Uuid::new_v4().simple()));

    entry.auth_token = selected_token.clone();
    registry.updated_at = now_rfc3339();
    write_registry(&path, &registry)?;

    Ok(NodeRegistration {
        node_id: node_id.to_string(),
        endpoint: endpoint.to_string(),
        auth_token: selected_token,
        registry_path: path,
    })
}

fn load_registry(path: &Path) -> Result<MeshRegistry, String> {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<MeshRegistry>(&raw)
            .map_err(|err| format!("parse {}: {err}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(MeshRegistry::default()),
        Err(err) => Err(format!("read {}: {err}", path.display())),
    }
}

fn write_registry(path: &Path, registry: &MeshRegistry) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create directory {}: {err}", parent.display()))?;
    }
    let encoded = serde_json::to_string_pretty(registry)
        .map_err(|err| format!("encode {}: {err}", path.display()))?;
    fs::write(path, encoded).map_err(|err| format!("write {}: {err}", path.display()))
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::register_local_node;

    #[test]
    fn register_creates_registry_and_token() {
        let dir = temp_dir_path("create");
        let reg = match register_local_node(&dir, "local-node", "grpc://127.0.0.1:50051", None) {
            Ok(value) => value,
            Err(err) => panic!("register node failed: {err}"),
        };

        assert_eq!(reg.node_id, "local-node");
        assert_eq!(reg.endpoint, "grpc://127.0.0.1:50051");
        assert!(reg.auth_token.starts_with("nt_"));
        assert!(reg.registry_path.exists());

        cleanup_dir(&dir);
    }

    #[test]
    fn register_reuses_existing_token_without_override() {
        let dir = temp_dir_path("reuse-token");
        let first = match register_local_node(&dir, "local-node", "grpc://127.0.0.1:50051", None) {
            Ok(value) => value,
            Err(err) => panic!("register first failed: {err}"),
        };
        let second = match register_local_node(&dir, "local-node", "grpc://127.0.0.1:50052", None) {
            Ok(value) => value,
            Err(err) => panic!("register second failed: {err}"),
        };

        assert_eq!(first.auth_token, second.auth_token);
        assert_eq!(second.endpoint, "grpc://127.0.0.1:50052");

        cleanup_dir(&dir);
    }

    #[test]
    fn register_override_token_wins() {
        let dir = temp_dir_path("override-token");
        if let Err(err) = register_local_node(&dir, "local-node", "grpc://127.0.0.1:50051", None) {
            panic!("register first failed: {err}");
        }
        let second = register_local_node(
            &dir,
            "local-node",
            "grpc://127.0.0.1:50051",
            Some("nt_custom"),
        )
        .unwrap_or_else(|err| panic!("register second failed: {err}"));

        assert_eq!(second.auth_token, "nt_custom");

        cleanup_dir(&dir);
    }

    fn temp_dir_path(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("forge-node-registry-{tag}-{pid}-{nanos}-{seq}"))
    }

    fn cleanup_dir(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }
}
