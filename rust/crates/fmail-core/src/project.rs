//! Project identity and derivation for fmail.
//!
//! Go parity: `project.go` (Project struct, DeriveProjectID, hashProjectID, gitRemoteURL).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

use crate::constants::ENV_PROJECT;

/// A project identity stored in `.fmail/project.json`.
///
/// Go parity: `Project` struct in `project.go`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub created: chrono::DateTime<chrono::Utc>,
}

/// Derive a stable project ID from env, git remote, or directory name.
///
/// Go parity: `DeriveProjectID` in `project.go`.
pub fn derive_project_id(project_root: &Path) -> Result<String, String> {
    if let Ok(val) = std::env::var(ENV_PROJECT) {
        let trimmed = val.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    let root_str = project_root.to_string_lossy();
    let root_trimmed = root_str.trim();
    if root_trimmed.is_empty() {
        return Err("project root required".to_string());
    }

    if let Ok(remote) = git_remote_url(project_root) {
        if !remote.is_empty() {
            return Ok(hash_project_id(&remote));
        }
    }

    let base = project_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if base.is_empty() || base == "." || base == "/" || base == std::path::MAIN_SEPARATOR_STR {
        return Err(format!("invalid project root: {}", project_root.display()));
    }
    Ok(hash_project_id(base))
}

/// Hash a value into a project ID: `proj-` + first 12 hex chars of SHA-256.
///
/// Go parity: `hashProjectID` in `project.go`.
fn hash_project_id(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let result = hasher.finalize();
    let hex = hex::encode(result);
    format!("proj-{}", &hex[..12])
}

/// Get the git remote origin URL for a directory.
///
/// Go parity: `gitRemoteURL` in `project.go`.
fn git_remote_url(root: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("git: {e}"))?;

    if !output.status.success() {
        return Ok(String::new());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn hash_project_id_deterministic() {
        let id1 = hash_project_id("https://github.com/example/repo.git");
        let id2 = hash_project_id("https://github.com/example/repo.git");
        assert_eq!(id1, id2);
        assert!(id1.starts_with("proj-"));
        assert_eq!(id1.len(), 5 + 12); // "proj-" + 12 hex chars
    }

    #[test]
    fn hash_project_id_different_inputs() {
        let id1 = hash_project_id("repo-a");
        let id2 = hash_project_id("repo-b");
        assert_ne!(id1, id2);
    }

    #[test]
    fn hash_project_id_format() {
        let id = hash_project_id("test");
        assert!(id.starts_with("proj-"));
        // All chars after prefix should be hex
        for c in id[5..].chars() {
            assert!(c.is_ascii_hexdigit());
        }
    }

    #[test]
    fn derive_project_id_from_dir_name() {
        // Use a temp dir with a known name
        let tmp = tempfile::Builder::new()
            .prefix("my-project")
            .tempdir()
            .expect("tempdir");
        let result = derive_project_id(tmp.path());
        assert!(result.is_ok());
        let id = result.expect("derive project id");
        assert!(id.starts_with("proj-"));
    }
}
