#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_db::port_repository::{PortRepository, DEFAULT_PORT_RANGE_END, DEFAULT_PORT_RANGE_START};
use forge_db::{Config, Db, DbError};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn temp_db_path(tag: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    std::env::temp_dir().join(format!(
        "forge-db-port-repo-{tag}-{nanos}-{}.sqlite",
        std::process::id()
    ))
}

fn open_migrated(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let mut db = match Db::open(Config::new(&path)) {
        Ok(db) => db,
        Err(e) => panic!("open db: {e}"),
    };
    match db.migrate_up() {
        Ok(_) => {}
        Err(e) => panic!("migrate: {e}"),
    }
    (db, path)
}

/// Insert a test node and return its ID.
fn insert_node(db: &Db, name: &str) -> String {
    let id = format!("node-{name}");
    db.conn()
        .execute(
            "INSERT INTO nodes (id, name, ssh_backend, status, is_local, created_at, updated_at)
             VALUES (?1, ?2, 'auto', 'unknown', 1, datetime('now'), datetime('now'))",
            rusqlite::params![id, name],
        )
        .expect("insert test node");
    id
}

/// Insert a test workspace and return its ID.
fn insert_workspace(db: &Db, node_id: &str, name: &str) -> String {
    let id = format!("ws-{name}");
    let tmux = format!("forge-test-{name}");
    db.conn()
        .execute(
            "INSERT INTO workspaces (id, node_id, name, repo_path, tmux_session, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, '/tmp/test', ?4, 'active', datetime('now'), datetime('now'))",
            rusqlite::params![id, node_id, name, tmux],
        )
        .expect("insert test workspace");
    id
}

/// Insert a test agent and return its ID.
fn insert_agent(db: &Db, workspace_id: &str, name: &str) -> String {
    let id = format!("agent-{name}");
    db.conn()
        .execute(
            "INSERT INTO agents (id, workspace_id, type, tmux_pane, state, created_at, updated_at)
             VALUES (?1, ?2, 'opencode', 'forge-test:0.1', 'idle', datetime('now'), datetime('now'))",
            rusqlite::params![id, workspace_id],
        )
        .expect("insert test agent");
    id
}

// ---------------------------------------------------------------------------
// Allocate tests
// ---------------------------------------------------------------------------

#[test]
fn allocate_returns_first_port_in_range() {
    let (db, path) = open_migrated("alloc-first");
    let node_id = insert_node(&db, "alloc-first");
    let ws_id = insert_workspace(&db, &node_id, "alloc-first");
    let agent_id = insert_agent(&db, &ws_id, "alloc-first");
    let repo = PortRepository::new(&db);

    let port = match repo.allocate(&node_id, &agent_id, "test allocation") {
        Ok(p) => p,
        Err(e) => panic!("allocate: {e}"),
    };

    assert!(
        (DEFAULT_PORT_RANGE_START..=DEFAULT_PORT_RANGE_END).contains(&port),
        "port {port} outside expected range {DEFAULT_PORT_RANGE_START}-{DEFAULT_PORT_RANGE_END}"
    );
    assert_eq!(
        port, DEFAULT_PORT_RANGE_START,
        "first allocation should get the first port in range"
    );

    // Verify allocation can be retrieved
    let alloc = match repo.get_by_agent(&agent_id) {
        Ok(a) => a,
        Err(e) => panic!("get_by_agent: {e}"),
    };
    assert_eq!(alloc.port, port);
    assert_eq!(alloc.agent_id.as_deref(), Some(agent_id.as_str()));

    let _ = std::fs::remove_file(path);
}

#[test]
fn allocate_multiple_returns_sequential_ports() {
    let (db, path) = open_migrated("alloc-multi");
    let node_id = insert_node(&db, "alloc-multi");
    let repo = PortRepository::new(&db);

    let port1 = match repo.allocate(&node_id, "", "first") {
        Ok(p) => p,
        Err(e) => panic!("first allocate: {e}"),
    };

    let port2 = match repo.allocate(&node_id, "", "second") {
        Ok(p) => p,
        Err(e) => panic!("second allocate: {e}"),
    };

    let port3 = match repo.allocate(&node_id, "", "third") {
        Ok(p) => p,
        Err(e) => panic!("third allocate: {e}"),
    };

    assert_eq!(port2, port1 + 1, "ports should be sequential");
    assert_eq!(port3, port2 + 1, "ports should be sequential");

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// AllocateSpecific tests
// ---------------------------------------------------------------------------

#[test]
fn allocate_specific_works() {
    let (db, path) = open_migrated("alloc-specific");
    let node_id = insert_node(&db, "alloc-specific");
    let repo = PortRepository::new(&db);

    let specific_port = 17500;
    match repo.allocate_specific(&node_id, specific_port, "", "specific test") {
        Ok(()) => {}
        Err(e) => panic!("allocate_specific: {e}"),
    }

    // Try to allocate same port again - should fail
    let err = repo.allocate_specific(&node_id, specific_port, "", "duplicate");
    assert!(
        matches!(err, Err(DbError::PortAlreadyAllocated)),
        "expected PortAlreadyAllocated, got {err:?}"
    );

    // Try to allocate port outside range - should fail
    let err = repo.allocate_specific(&node_id, 10000, "", "out of range");
    assert!(
        matches!(err, Err(DbError::Validation(_))),
        "expected Validation error for out-of-range port, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Release tests
// ---------------------------------------------------------------------------

#[test]
fn release_frees_port_for_reallocation() {
    let (db, path) = open_migrated("release");
    let node_id = insert_node(&db, "release");
    let repo = PortRepository::new(&db);

    let port = match repo.allocate(&node_id, "", "to release") {
        Ok(p) => p,
        Err(e) => panic!("allocate: {e}"),
    };

    match repo.release(&node_id, port) {
        Ok(()) => {}
        Err(e) => panic!("release: {e}"),
    }

    // Releasing again should fail
    let err = repo.release(&node_id, port);
    assert!(
        matches!(err, Err(DbError::PortNotAllocated)),
        "expected PortNotAllocated, got {err:?}"
    );

    // Port should be available for reallocation
    let port2 = match repo.allocate(&node_id, "", "reuse") {
        Ok(p) => p,
        Err(e) => panic!("allocate after release: {e}"),
    };
    assert_eq!(port2, port, "expected to reuse released port");

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// ReleaseByAgent tests
// ---------------------------------------------------------------------------

#[test]
fn release_by_agent_removes_all_agent_ports() {
    let (db, path) = open_migrated("release-agent");
    let node_id = insert_node(&db, "release-agent");
    let ws_id = insert_workspace(&db, &node_id, "release-agent");
    let agent_id = insert_agent(&db, &ws_id, "release-agent");
    let repo = PortRepository::new(&db);

    match repo.allocate(&node_id, &agent_id, "agent port 1") {
        Ok(_) => {}
        Err(e) => panic!("first allocate: {e}"),
    }

    let count = match repo.release_by_agent(&agent_id) {
        Ok(c) => c,
        Err(e) => panic!("release_by_agent: {e}"),
    };
    assert_eq!(count, 1, "expected 1 released");

    // Verify no active allocation for agent
    let err = repo.get_by_agent(&agent_id);
    assert!(
        matches!(err, Err(DbError::PortNotAllocated)),
        "expected PortNotAllocated, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// ListActiveByNode tests
// ---------------------------------------------------------------------------

#[test]
fn list_active_by_node_excludes_released() {
    let (db, path) = open_migrated("list-active");
    let node_id = insert_node(&db, "list-active");
    let repo = PortRepository::new(&db);

    let port1 = match repo.allocate(&node_id, "", "first") {
        Ok(p) => p,
        Err(e) => panic!("first allocate: {e}"),
    };
    let port2 = match repo.allocate(&node_id, "", "second") {
        Ok(p) => p,
        Err(e) => panic!("second allocate: {e}"),
    };
    let port3 = match repo.allocate(&node_id, "", "third") {
        Ok(p) => p,
        Err(e) => panic!("third allocate: {e}"),
    };

    // Release one
    match repo.release(&node_id, port2) {
        Ok(()) => {}
        Err(e) => panic!("release: {e}"),
    }

    let active = match repo.list_active_by_node(&node_id) {
        Ok(a) => a,
        Err(e) => panic!("list_active_by_node: {e}"),
    };
    assert_eq!(active.len(), 2, "expected 2 active allocations");

    let ports: Vec<i32> = active.iter().map(|a| a.port).collect();
    assert!(ports.contains(&port1), "port1 should be active");
    assert!(ports.contains(&port3), "port3 should be active");
    assert!(!ports.contains(&port2), "port2 should not be active");

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// IsPortAvailable tests
// ---------------------------------------------------------------------------

#[test]
fn is_port_available_reflects_allocation_state() {
    let (db, path) = open_migrated("is-avail");
    let node_id = insert_node(&db, "is-avail");
    let repo = PortRepository::new(&db);

    let test_port = 17100;

    // Port should be available initially
    let available = match repo.is_port_available(&node_id, test_port) {
        Ok(a) => a,
        Err(e) => panic!("is_port_available: {e}"),
    };
    assert!(available, "expected port to be available");

    // Allocate it
    match repo.allocate_specific(&node_id, test_port, "", "test") {
        Ok(()) => {}
        Err(e) => panic!("allocate_specific: {e}"),
    }

    // Port should not be available
    let available = match repo.is_port_available(&node_id, test_port) {
        Ok(a) => a,
        Err(e) => panic!("is_port_available: {e}"),
    };
    assert!(!available, "expected port to be unavailable");

    // Release it
    match repo.release(&node_id, test_port) {
        Ok(()) => {}
        Err(e) => panic!("release: {e}"),
    }

    // Port should be available again
    let available = match repo.is_port_available(&node_id, test_port) {
        Ok(a) => a,
        Err(e) => panic!("is_port_available: {e}"),
    };
    assert!(available, "expected port to be available after release");

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// CountActiveByNode tests
// ---------------------------------------------------------------------------

#[test]
fn count_active_by_node_tracks_allocations() {
    let (db, path) = open_migrated("count-active");
    let node_id = insert_node(&db, "count-active");
    let repo = PortRepository::new(&db);

    // Initially zero
    let count = match repo.count_active_by_node(&node_id) {
        Ok(c) => c,
        Err(e) => panic!("count_active_by_node: {e}"),
    };
    assert_eq!(count, 0, "expected 0 initially");

    // Allocate some ports
    match repo.allocate(&node_id, "", "first") {
        Ok(_) => {}
        Err(e) => panic!("first allocate: {e}"),
    }
    let port2 = match repo.allocate(&node_id, "", "second") {
        Ok(p) => p,
        Err(e) => panic!("second allocate: {e}"),
    };
    match repo.allocate(&node_id, "", "third") {
        Ok(_) => {}
        Err(e) => panic!("third allocate: {e}"),
    }

    let count = match repo.count_active_by_node(&node_id) {
        Ok(c) => c,
        Err(e) => panic!("count_active_by_node: {e}"),
    };
    assert_eq!(count, 3, "expected 3");

    // Release one
    match repo.release(&node_id, port2) {
        Ok(()) => {}
        Err(e) => panic!("release: {e}"),
    }

    let count = match repo.count_active_by_node(&node_id) {
        Ok(c) => c,
        Err(e) => panic!("count_active_by_node: {e}"),
    };
    assert_eq!(count, 2, "expected 2 after release");

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// CleanupExpired tests
// ---------------------------------------------------------------------------

#[test]
fn cleanup_expired_removes_orphaned_allocations() {
    let (db, path) = open_migrated("cleanup");
    let node_id = insert_node(&db, "cleanup");
    let ws_id = insert_workspace(&db, &node_id, "cleanup");
    let agent_id = insert_agent(&db, &ws_id, "cleanup");
    let repo = PortRepository::new(&db);

    // Allocate a port to agent
    let port = match repo.allocate(&node_id, &agent_id, "agent port") {
        Ok(p) => p,
        Err(e) => panic!("allocate: {e}"),
    };

    // Also allocate a port without agent
    match repo.allocate(&node_id, "", "no agent") {
        Ok(_) => {}
        Err(e) => panic!("allocate: {e}"),
    }

    // Cleanup should not release anything yet (agent still exists)
    let cleaned = match repo.cleanup_expired() {
        Ok(c) => c,
        Err(e) => panic!("cleanup_expired: {e}"),
    };
    assert_eq!(cleaned, 0, "expected 0 cleaned");

    // Delete the agent - ON DELETE CASCADE automatically removes the port allocation
    db.conn()
        .execute(
            "DELETE FROM agents WHERE id = ?1",
            rusqlite::params![agent_id],
        )
        .expect("delete agent");

    // Port should be available because CASCADE deleted the allocation
    let available = match repo.is_port_available(&node_id, port) {
        Ok(a) => a,
        Err(e) => panic!("is_port_available: {e}"),
    };
    assert!(
        available,
        "expected port to be available after agent deletion (cascade)"
    );

    // CleanupExpired should have nothing to clean (cascade already handled it)
    let cleaned = match repo.cleanup_expired() {
        Ok(c) => c,
        Err(e) => panic!("cleanup_expired: {e}"),
    };
    assert_eq!(cleaned, 0, "expected 0 cleaned (cascade already handled)");

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// CustomRange tests
// ---------------------------------------------------------------------------

#[test]
fn custom_range_allocates_from_start_and_exhausts() {
    let (db, path) = open_migrated("custom-range");
    let node_id = insert_node(&db, "custom-range");
    let repo = PortRepository::with_range(&db, 20000, 20010);

    let port = match repo.allocate(&node_id, "", "custom range") {
        Ok(p) => p,
        Err(e) => panic!("allocate: {e}"),
    };
    assert_eq!(port, 20000, "expected port 20000");

    // Fill up the range
    for i in 1..=10 {
        match repo.allocate(&node_id, "", "fill") {
            Ok(_) => {}
            Err(e) => panic!("allocate {i}: {e}"),
        }
    }

    // Next allocation should fail - range exhausted
    let err = repo.allocate(&node_id, "", "overflow");
    assert!(
        matches!(err, Err(DbError::NoAvailablePorts)),
        "expected NoAvailablePorts, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// NodeIsolation tests
// ---------------------------------------------------------------------------

#[test]
fn node_isolation_allows_same_port_on_different_nodes() {
    let (db, path) = open_migrated("node-isolation");
    let node1_id = insert_node(&db, "node-iso-1");
    let node2_id = insert_node(&db, "node-iso-2");
    let repo = PortRepository::new(&db);

    let specific_port = 17500;

    match repo.allocate_specific(&node1_id, specific_port, "", "node1") {
        Ok(()) => {}
        Err(e) => panic!("allocate_specific on node1: {e}"),
    }

    match repo.allocate_specific(&node2_id, specific_port, "", "node2") {
        Ok(()) => {}
        Err(e) => panic!("allocate_specific on node2: {e}"),
    }

    let count1 = match repo.count_active_by_node(&node1_id) {
        Ok(c) => c,
        Err(e) => panic!("count node1: {e}"),
    };
    let count2 = match repo.count_active_by_node(&node2_id) {
        Ok(c) => c,
        Err(e) => panic!("count node2: {e}"),
    };

    assert_eq!(count1, 1, "expected 1 allocation on node1");
    assert_eq!(count2, 1, "expected 1 allocation on node2");

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// GetByNodeAndPort tests
// ---------------------------------------------------------------------------

#[test]
fn get_by_node_and_port_works() {
    let (db, path) = open_migrated("get-node-port");
    let node_id = insert_node(&db, "get-node-port");
    let repo = PortRepository::new(&db);

    let specific_port = 17200;
    match repo.allocate_specific(&node_id, specific_port, "", "test") {
        Ok(()) => {}
        Err(e) => panic!("allocate_specific: {e}"),
    }

    let alloc = match repo.get_by_node_and_port(&node_id, specific_port) {
        Ok(a) => a,
        Err(e) => panic!("get_by_node_and_port: {e}"),
    };
    assert_eq!(alloc.port, specific_port);
    assert_eq!(alloc.node_id, node_id);
    assert_eq!(alloc.reason, "test");

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_by_node_and_port_not_found() {
    let (db, path) = open_migrated("get-node-port-404");
    let node_id = insert_node(&db, "get-node-port-404");
    let repo = PortRepository::new(&db);

    let err = repo.get_by_node_and_port(&node_id, 17999);
    assert!(
        matches!(err, Err(DbError::PortNotAllocated)),
        "expected PortNotAllocated, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}
