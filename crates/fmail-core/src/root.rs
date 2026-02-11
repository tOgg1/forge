use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::constants::ENV_ROOT;

/// Order: `FMAIL_ROOT` -> `.fmail` -> `.git` -> current directory.
pub fn discover_project_root(start_dir: Option<&Path>) -> Result<PathBuf, String> {
    if let Ok(root_env) = env::var(ENV_ROOT) {
        let root_env = root_env.trim().to_string();
        if !root_env.is_empty() {
            return normalize_root(start_dir, &root_env);
        }
    }

    let start = resolve_start_dir(start_dir)?;
    if let Some(root) = walk_up(&start, ".fmail", true)? {
        return Ok(root);
    }
    if let Some(root) = walk_up(&start, ".git", false)? {
        if let Some(shared) = worktree_shared_root(&root)? {
            return Ok(shared);
        }
        return Ok(root);
    }

    Ok(start)
}

fn resolve_start_dir(start_dir: Option<&Path>) -> Result<PathBuf, String> {
    let dir = match start_dir {
        Some(p) => p.to_path_buf(),
        None => env::current_dir().map_err(|e| e.to_string())?,
    };
    dir.canonicalize().map_err(|e| e.to_string())
}

fn normalize_root(base_dir: Option<&Path>, root: &str) -> Result<PathBuf, String> {
    let path = root.trim().to_string();
    if path.is_empty() {
        return Err(format!("empty {ENV_ROOT}"));
    }
    let mut pb = PathBuf::from(&path);
    if !pb.is_absolute() {
        let base = resolve_start_dir(base_dir)?;
        pb = base.join(pb);
    }
    let abs = pb.canonicalize().map_err(|e| e.to_string())?;
    let meta = fs::metadata(&abs).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err(format!("{ENV_ROOT} is not a directory: {}", abs.display()));
    }
    Ok(abs)
}

fn walk_up(start: &Path, marker: &str, dir_only: bool) -> Result<Option<PathBuf>, String> {
    let mut current = start.to_path_buf();
    loop {
        let candidate = current.join(marker);
        if let Ok(meta) = fs::metadata(&candidate) {
            if !dir_only || meta.is_dir() {
                return Ok(Some(current));
            }
        }

        let parent = current.parent().map(|p| p.to_path_buf());
        match parent {
            Some(p) if p != current => current = p,
            _ => return Ok(None),
        }
    }
}

fn worktree_shared_root(repo_root: &Path) -> Result<Option<PathBuf>, String> {
    let git_path = repo_root.join(".git");
    let meta = match fs::metadata(&git_path) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if meta.is_dir() {
        return Ok(None);
    }

    let data = fs::read_to_string(&git_path).map_err(|e| e.to_string())?;
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("gitdir:") {
            let path = rest.trim().to_string();
            if path.is_empty() {
                return Ok(None);
            }
            let mut pb = PathBuf::from(&path);
            if !pb.is_absolute() {
                pb = repo_root.join(pb);
            }
            let git_dir = pb.canonicalize().map_err(|e| e.to_string())?;
            return Ok(shared_root_from_git_dir(&git_dir));
        }
    }
    Ok(None)
}

fn shared_root_from_git_dir(git_dir: &Path) -> Option<PathBuf> {
    let clean = git_dir.to_string_lossy().replace('\\', "/");
    let marker = "/.git/worktrees/";
    let idx = clean.find(marker)?;
    let common_git_dir = PathBuf::from(&clean[..idx]).join(".git");
    if fs::metadata(&common_git_dir).ok()?.is_dir() {
        let root = common_git_dir.parent()?.to_path_buf();
        if fs::metadata(&root).ok()?.is_dir() {
            return Some(root);
        }
    }
    None
}
