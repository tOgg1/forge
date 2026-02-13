use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    Ok,
    Expired,
    Missing,
}

impl AuthStatus {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ok" | "authenticated" => Ok(Self::Ok),
            "expired" => Ok(Self::Expired),
            "missing" => Ok(Self::Missing),
            other => Err(format!(
                "unknown auth status {:?} (expected ok|expired|missing)",
                other
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogProfileEntry {
    pub id: String,
    pub harness: String,
    pub auth_status: AuthStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeProfileState {
    pub node_id: String,
    pub updated_at: String,
    pub profiles: Vec<CatalogProfileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileCatalog {
    pub schema_version: u32,
    pub updated_at: String,
    #[serde(default)]
    pub harness_counts: BTreeMap<String, u32>,
    #[serde(default)]
    pub nodes: BTreeMap<String, NodeProfileState>,
}

impl Default for ProfileCatalog {
    fn default() -> Self {
        Self {
            schema_version: 1,
            updated_at: now_rfc3339(),
            harness_counts: BTreeMap::new(),
            nodes: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeProfileSummary {
    pub total: usize,
    pub ok: usize,
    pub expired: usize,
    pub missing: usize,
}

impl NodeProfileSummary {
    fn from_profiles(profiles: &[CatalogProfileEntry]) -> Self {
        let mut summary = Self {
            total: profiles.len(),
            ok: 0,
            expired: 0,
            missing: 0,
        };
        for profile in profiles {
            match profile.auth_status {
                AuthStatus::Ok => summary.ok += 1,
                AuthStatus::Expired => summary.expired += 1,
                AuthStatus::Missing => summary.missing += 1,
            }
        }
        summary
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileCatalogStore {
    path: PathBuf,
}

impl ProfileCatalogStore {
    #[must_use]
    pub fn open_from_env() -> Self {
        Self::with_path(
            crate::runtime_paths::resolve_data_dir()
                .join("mesh")
                .join("profile_catalog.json"),
        )
    }

    #[must_use]
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn status(&self) -> Result<ProfileCatalog, String> {
        self.load_or_default()
    }

    pub fn provision_node(
        &self,
        node_id: &str,
        harness_counts: &BTreeMap<String, u32>,
    ) -> Result<NodeProfileState, String> {
        let node_id = normalize_node_id(node_id)?;
        if harness_counts.is_empty() {
            return Err("at least one harness count is required".to_string());
        }

        let mut catalog = self.load_or_default()?;
        for (harness, count) in harness_counts {
            if *count == 0 {
                continue;
            }
            let normalized = normalize_harness(harness);
            catalog.harness_counts.insert(normalized, *count);
        }

        let mut profiles = Vec::new();
        for (harness, count) in &catalog.harness_counts {
            for index in 1..=*count {
                profiles.push(CatalogProfileEntry {
                    id: canonical_profile_id(harness, index),
                    harness: harness.clone(),
                    auth_status: AuthStatus::Missing,
                });
            }
        }

        let now = now_rfc3339();
        let entry = NodeProfileState {
            node_id: node_id.clone(),
            updated_at: now.clone(),
            profiles,
        };
        catalog.nodes.insert(node_id, entry.clone());
        catalog.updated_at = now;
        self.write_catalog(&catalog)?;
        Ok(entry)
    }

    pub fn set_auth_status(
        &self,
        node_id: &str,
        profile_id: &str,
        status: AuthStatus,
    ) -> Result<NodeProfileState, String> {
        let node_id = normalize_node_id(node_id)?;
        let profile_id = profile_id.trim();
        if profile_id.is_empty() {
            return Err("profile id is required".to_string());
        }

        let mut catalog = self.load_or_default()?;
        let now = now_rfc3339();
        {
            let Some(node) = catalog.nodes.get_mut(&node_id) else {
                return Err(format!("node not found in catalog: {node_id}"));
            };
            let Some(profile) = node
                .profiles
                .iter_mut()
                .find(|entry| entry.id == profile_id)
            else {
                return Err(format!("profile not found on node {node_id}: {profile_id}"));
            };

            profile.auth_status = status;
            node.updated_at = now.clone();
        };
        let updated_node = catalog
            .nodes
            .get(&node_id)
            .cloned()
            .ok_or_else(|| format!("node not found in catalog: {node_id}"))?;
        catalog.updated_at = now;
        self.write_catalog(&catalog)?;
        Ok(updated_node)
    }

    pub fn node_summary(&self, node_id: &str) -> Result<Option<NodeProfileSummary>, String> {
        let catalog = self.load_or_default()?;
        let node_id = node_id.trim();
        if node_id.is_empty() {
            return Ok(None);
        }
        Ok(catalog
            .nodes
            .get(node_id)
            .map(|entry| NodeProfileSummary::from_profiles(&entry.profiles)))
    }

    fn load_or_default(&self) -> Result<ProfileCatalog, String> {
        match fs::read_to_string(&self.path) {
            Ok(raw) => parse_catalog(&raw, &self.path),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(ProfileCatalog::default()),
            Err(err) => Err(format!("read {}: {err}", self.path.display())),
        }
    }

    fn write_catalog(&self, catalog: &ProfileCatalog) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("create directory {}: {err}", parent.display()))?;
        }
        let encoded = serde_json::to_string_pretty(catalog)
            .map_err(|err| format!("encode profile catalog: {err}"))?;
        fs::write(&self.path, encoded)
            .map_err(|err| format!("write {}: {err}", self.path.display()))
    }
}

fn parse_catalog(raw: &str, path: &Path) -> Result<ProfileCatalog, String> {
    serde_json::from_str(raw).map_err(|err| format!("decode {}: {err}", path.display()))
}

fn normalize_harness(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn normalize_node_id(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("node id is required".to_string());
    }
    Ok(value.to_string())
}

pub fn canonical_profile_id(harness: &str, index: u32) -> String {
    let harness = normalize_harness(harness);
    let prefix = match harness.as_str() {
        "claude" => "CC".to_string(),
        "codex" => "Codex".to_string(),
        "opencode" => "OC".to_string(),
        "amp" => "AMP".to_string(),
        "droid" => "Droid".to_string(),
        "pi" => "PI".to_string(),
        _ => harness.to_ascii_uppercase(),
    };
    format!("{prefix}{index}")
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{canonical_profile_id, AuthStatus, ProfileCatalogStore};

    fn temp_path(tag: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("forge-profile-catalog-{tag}-{nanos}.json"))
    }

    #[test]
    fn canonical_ids_use_expected_prefixes() {
        assert_eq!(canonical_profile_id("claude", 1), "CC1");
        assert_eq!(canonical_profile_id("codex", 2), "Codex2");
        assert_eq!(canonical_profile_id("opencode", 1), "OC1");
    }

    #[test]
    fn provision_and_update_node_auth_state() {
        let path = temp_path("provision");
        let store = ProfileCatalogStore::with_path(path.clone());
        let mut counts = BTreeMap::new();
        counts.insert("claude".to_string(), 1);
        counts.insert("codex".to_string(), 2);

        let provisioned = store
            .provision_node("node-a", &counts)
            .expect("provision node");
        assert_eq!(provisioned.profiles.len(), 3);

        let updated = store
            .set_auth_status("node-a", "Codex2", AuthStatus::Ok)
            .expect("set auth");
        let codex2 = updated
            .profiles
            .iter()
            .find(|entry| entry.id == "Codex2")
            .expect("Codex2 exists");
        assert!(matches!(codex2.auth_status, AuthStatus::Ok));

        let summary = store
            .node_summary("node-a")
            .expect("summary")
            .expect("node summary");
        assert_eq!(summary.total, 3);
        assert_eq!(summary.ok, 1);
        assert_eq!(summary.missing, 2);

        let _ = std::fs::remove_file(path);
    }
}
