#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::persistent_agent_repository::{
    PersistentAgent, PersistentAgentFilter, PersistentAgentRepository,
};
use forge_db::{Config, Db};

fn open_db(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let mut db = Db::open(Config::new(&path)).expect("open db");
    db.migrate_up().expect("migrate_up");
    (db, path)
}

#[test]
fn create_and_get() {
    let (db, path) = open_db("pa-create-get");
    let repo = PersistentAgentRepository::new(&db);

    let mut agent = PersistentAgent {
        workspace_id: "ws-1".into(),
        harness: "claude-code".into(),
        mode: "continuous".into(),
        state: "idle".into(),
        ..Default::default()
    };

    repo.create(&mut agent).expect("create");
    assert!(!agent.id.is_empty(), "id should be assigned");
    assert!(!agent.created_at.is_empty(), "created_at should be set");

    let fetched = repo.get(&agent.id).expect("get");
    assert_eq!(fetched.workspace_id, "ws-1");
    assert_eq!(fetched.harness, "claude-code");
    assert_eq!(fetched.mode, "continuous");
    assert_eq!(fetched.state, "idle");

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_with_labels_and_tags() {
    let (db, path) = open_db("pa-labels-tags");
    let repo = PersistentAgentRepository::new(&db);

    let mut labels = HashMap::new();
    labels.insert("team".into(), "infra".into());
    labels.insert("env".into(), "prod".into());

    let mut agent = PersistentAgent {
        workspace_id: "ws-1".into(),
        harness: "codex".into(),
        mode: "one-shot".into(),
        state: "starting".into(),
        parent_agent_id: Some("parent-001".into()),
        repo: Some("/repo/test".into()),
        node: Some("node-1".into()),
        ttl_seconds: Some(3600),
        labels,
        tags: vec!["fast".into(), "priority".into()],
        ..Default::default()
    };

    repo.create(&mut agent).expect("create with labels/tags");

    let fetched = repo.get(&agent.id).expect("get");
    assert_eq!(fetched.parent_agent_id, Some("parent-001".into()));
    assert_eq!(fetched.repo, Some("/repo/test".into()));
    assert_eq!(fetched.node, Some("node-1".into()));
    assert_eq!(fetched.ttl_seconds, Some(3600));
    assert_eq!(fetched.labels.get("team"), Some(&"infra".to_string()));
    assert_eq!(fetched.labels.get("env"), Some(&"prod".to_string()));
    assert!(fetched.tags.contains(&"fast".to_string()));
    assert!(fetched.tags.contains(&"priority".to_string()));

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_validation() {
    let (db, path) = open_db("pa-validation");
    let repo = PersistentAgentRepository::new(&db);

    // Missing workspace_id.
    let mut agent = PersistentAgent {
        harness: "claude-code".into(),
        ..Default::default()
    };
    let err = repo.create(&mut agent);
    assert!(err.is_err(), "should reject empty workspace_id");

    // Missing harness.
    let mut agent = PersistentAgent {
        workspace_id: "ws-1".into(),
        ..Default::default()
    };
    let err = repo.create(&mut agent);
    assert!(err.is_err(), "should reject empty harness");

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_state() {
    let (db, path) = open_db("pa-update-state");
    let repo = PersistentAgentRepository::new(&db);

    let mut agent = PersistentAgent {
        workspace_id: "ws-1".into(),
        harness: "claude-code".into(),
        state: "starting".into(),
        ..Default::default()
    };
    repo.create(&mut agent).expect("create");

    repo.update_state(&agent.id, "running")
        .expect("update_state");
    let fetched = repo.get(&agent.id).expect("get after update");
    assert_eq!(fetched.state, "running");

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_state_not_found() {
    let (db, path) = open_db("pa-update-notfound");
    let repo = PersistentAgentRepository::new(&db);

    let err = repo.update_state("nonexistent", "idle");
    assert!(err.is_err(), "should error on missing agent");

    let _ = std::fs::remove_file(path);
}

#[test]
fn touch_activity() {
    let (db, path) = open_db("pa-touch");
    let repo = PersistentAgentRepository::new(&db);

    let mut agent = PersistentAgent {
        workspace_id: "ws-1".into(),
        harness: "claude-code".into(),
        state: "idle".into(),
        ..Default::default()
    };
    repo.create(&mut agent).expect("create");
    let before = repo.get(&agent.id).expect("get").last_activity_at;

    // Small delay to ensure timestamp differs.
    std::thread::sleep(std::time::Duration::from_millis(10));
    repo.touch_activity(&agent.id).expect("touch");

    let after = repo.get(&agent.id).expect("get").last_activity_at;
    assert!(after >= before, "last_activity_at should be >= before");

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_agent() {
    let (db, path) = open_db("pa-delete");
    let repo = PersistentAgentRepository::new(&db);

    let mut agent = PersistentAgent {
        workspace_id: "ws-1".into(),
        harness: "claude-code".into(),
        state: "stopped".into(),
        ..Default::default()
    };
    repo.create(&mut agent).expect("create");

    repo.delete(&agent.id).expect("delete");
    let err = repo.get(&agent.id);
    assert!(err.is_err(), "should not find deleted agent");

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_not_found() {
    let (db, path) = open_db("pa-delete-notfound");
    let repo = PersistentAgentRepository::new(&db);

    let err = repo.delete("nonexistent");
    assert!(err.is_err(), "should error on missing agent");

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_with_filters() {
    let (db, path) = open_db("pa-list");
    let repo = PersistentAgentRepository::new(&db);

    // Create 3 agents in different states/workspaces.
    for (ws, state, harness) in &[
        ("ws-1", "idle", "claude-code"),
        ("ws-1", "running", "codex"),
        ("ws-2", "idle", "claude-code"),
    ] {
        let mut agent = PersistentAgent {
            workspace_id: ws.to_string(),
            harness: harness.to_string(),
            state: state.to_string(),
            mode: "continuous".into(),
            ..Default::default()
        };
        repo.create(&mut agent).expect("create");
    }

    // List all.
    let all = repo
        .list(PersistentAgentFilter::default())
        .expect("list all");
    assert_eq!(all.len(), 3);

    // Filter by workspace.
    let ws1 = repo
        .list(PersistentAgentFilter {
            workspace_id: Some("ws-1".into()),
            ..Default::default()
        })
        .expect("list ws-1");
    assert_eq!(ws1.len(), 2);

    // Filter by state.
    let idle = repo
        .list(PersistentAgentFilter {
            states: vec!["idle".into()],
            ..Default::default()
        })
        .expect("list idle");
    assert_eq!(idle.len(), 2);

    // Filter by harness.
    let codex = repo
        .list(PersistentAgentFilter {
            harness: Some("codex".into()),
            ..Default::default()
        })
        .expect("list codex");
    assert_eq!(codex.len(), 1);

    // Compound filter: ws-1 + idle.
    let ws1_idle = repo
        .list(PersistentAgentFilter {
            workspace_id: Some("ws-1".into()),
            states: vec!["idle".into()],
            ..Default::default()
        })
        .expect("list ws-1 idle");
    assert_eq!(ws1_idle.len(), 1);

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_by_parent() {
    let (db, path) = open_db("pa-list-parent");
    let repo = PersistentAgentRepository::new(&db);

    let mut parent = PersistentAgent {
        workspace_id: "ws-1".into(),
        harness: "claude-code".into(),
        state: "idle".into(),
        ..Default::default()
    };
    repo.create(&mut parent).expect("create parent");

    let mut child1 = PersistentAgent {
        workspace_id: "ws-1".into(),
        harness: "codex".into(),
        state: "running".into(),
        parent_agent_id: Some(parent.id.clone()),
        ..Default::default()
    };
    repo.create(&mut child1).expect("create child1");

    let mut child2 = PersistentAgent {
        workspace_id: "ws-1".into(),
        harness: "claude-code".into(),
        state: "idle".into(),
        parent_agent_id: Some(parent.id.clone()),
        ..Default::default()
    };
    repo.create(&mut child2).expect("create child2");

    let children = repo
        .list(PersistentAgentFilter {
            parent_agent_id: Some(parent.id.clone()),
            ..Default::default()
        })
        .expect("list children");
    assert_eq!(children.len(), 2);

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_limit() {
    let (db, path) = open_db("pa-list-limit");
    let repo = PersistentAgentRepository::new(&db);

    for i in 0..5 {
        let mut agent = PersistentAgent {
            workspace_id: "ws-1".into(),
            harness: "claude-code".into(),
            state: "idle".into(),
            ..Default::default()
        };
        agent.id = format!("agent-{i}");
        repo.create(&mut agent).expect("create");
    }

    let limited = repo
        .list(PersistentAgentFilter {
            limit: 2,
            ..Default::default()
        })
        .expect("list limit 2");
    assert_eq!(limited.len(), 2);

    let _ = std::fs::remove_file(path);
}

#[test]
fn count_by_state() {
    let (db, path) = open_db("pa-count");
    let repo = PersistentAgentRepository::new(&db);

    for state in &["idle", "idle", "running", "stopped"] {
        let mut agent = PersistentAgent {
            workspace_id: "ws-1".into(),
            harness: "claude-code".into(),
            state: state.to_string(),
            ..Default::default()
        };
        repo.create(&mut agent).expect("create");
    }

    assert_eq!(repo.count_by_state("idle").expect("count idle"), 2);
    assert_eq!(repo.count_by_state("running").expect("count running"), 1);
    assert_eq!(repo.count_by_state("stopped").expect("count stopped"), 1);
    assert_eq!(repo.count_by_state("failed").expect("count failed"), 0);

    let _ = std::fs::remove_file(path);
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_nanos(),
        Err(err) => panic!("clock before epoch: {err}"),
    };
    let suffix = uuid::Uuid::new_v4();
    std::env::temp_dir().join(format!("forge-db-{prefix}-{nanos}-{suffix}.sqlite"))
}
