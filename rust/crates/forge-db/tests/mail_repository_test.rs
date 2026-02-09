#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::mail_repository::{MailMessage, MailRepository, MailThread, RecipientType};
use forge_db::{Config, Db, DbError};
use rusqlite::params;

fn temp_db_path(tag: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    std::env::temp_dir().join(format!(
        "forge-db-mail-repo-{tag}-{nanos}-{}.sqlite",
        std::process::id()
    ))
}

fn open_migrated(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let mut db = match Db::open(Config::new(&path)) {
        Ok(db) => db,
        Err(err) => panic!("open db: {err}"),
    };
    match db.migrate_up() {
        Ok(_) => {}
        Err(err) => panic!("migrate_up: {err}"),
    }
    (db, path)
}

fn insert_workspace_graph(db: &Db, node_id: &str, workspace_id: &str, agent_id: &str) {
    if let Err(err) = db.conn().execute(
        "INSERT INTO nodes (id, name, status, is_local) VALUES (?1, ?2, 'online', 1)",
        params![node_id, format!("node-{node_id}")],
    ) {
        panic!("insert node: {err}");
    }
    if let Err(err) = db.conn().execute(
        "INSERT INTO workspaces (id, name, node_id, repo_path, tmux_session, status)
         VALUES (?1, ?2, ?3, '/repo', 'sess', 'active')",
        params![workspace_id, format!("ws-{workspace_id}"), node_id],
    ) {
        panic!("insert workspace: {err}");
    }
    if let Err(err) = db.conn().execute(
        "INSERT INTO agents (id, workspace_id, type, tmux_pane, state, state_confidence)
         VALUES (?1, ?2, 'codex', '1.1', 'idle', 'high')",
        params![agent_id, workspace_id],
    ) {
        panic!("insert agent: {err}");
    }
}

#[test]
fn create_thread_and_messages_roundtrip() {
    let (db, path) = open_migrated("roundtrip");
    insert_workspace_graph(&db, "node-1", "ws-1", "agent-1");
    let repo = MailRepository::new(&db);

    let mut thread = MailThread {
        workspace_id: "ws-1".to_string(),
        subject: "review".to_string(),
        ..MailThread::default()
    };
    if let Err(err) = repo.create_thread(&mut thread) {
        panic!("create_thread: {err}");
    }
    assert!(!thread.id.is_empty());

    let mut msg = MailMessage {
        thread_id: thread.id.clone(),
        sender_agent_id: Some("agent-1".to_string()),
        recipient_type: RecipientType::Workspace,
        recipient_id: Some("ws-1".to_string()),
        subject: Some("Need input".to_string()),
        body: "Can you review this?".to_string(),
        importance: "high".to_string(),
        ack_required: true,
        ..MailMessage::default()
    };
    if let Err(err) = repo.create_message(&mut msg) {
        panic!("create_message: {err}");
    }
    assert!(!msg.id.is_empty());

    let got_thread = match repo.get_thread(&thread.id) {
        Ok(value) => value,
        Err(err) => panic!("get_thread: {err}"),
    };
    assert_eq!(got_thread.workspace_id, "ws-1");
    assert_eq!(got_thread.subject, "review");

    let got_msg = match repo.get_message(&msg.id) {
        Ok(value) => value,
        Err(err) => panic!("get_message: {err}"),
    };
    assert_eq!(got_msg.body, "Can you review this?");
    assert!(got_msg.ack_required);

    let threads = match repo.list_threads_by_workspace("ws-1") {
        Ok(values) => values,
        Err(err) => panic!("list_threads_by_workspace: {err}"),
    };
    assert_eq!(threads.len(), 1);

    let thread_msgs = match repo.list_messages_by_thread(&thread.id) {
        Ok(values) => values,
        Err(err) => panic!("list_messages_by_thread: {err}"),
    };
    assert_eq!(thread_msgs.len(), 1);

    // Check unread messages via inbox filter.
    let unread = match repo.list_inbox("workspace", Some("ws-1"), true, 10) {
        Ok(values) => values,
        Err(err) => panic!("list_inbox unread: {err}"),
    };
    assert_eq!(unread.len(), 1);

    if let Err(err) = repo.mark_read(&msg.id) {
        panic!("mark_read: {err}");
    }
    if let Err(err) = repo.mark_acked(&msg.id) {
        panic!("mark_acked: {err}");
    }

    let unread_after = match repo.list_inbox("workspace", Some("ws-1"), true, 10) {
        Ok(values) => values,
        Err(err) => panic!("list_inbox unread after: {err}"),
    };
    assert!(unread_after.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn broadcast_and_validation_paths() {
    let (db, path) = open_migrated("validation");
    insert_workspace_graph(&db, "node-2", "ws-2", "agent-2");
    let repo = MailRepository::new(&db);

    let mut thread = MailThread {
        workspace_id: "ws-2".to_string(),
        subject: "broadcast".to_string(),
        ..MailThread::default()
    };
    if let Err(err) = repo.create_thread(&mut thread) {
        panic!("create_thread: {err}");
    }

    let mut broadcast = MailMessage {
        thread_id: thread.id.clone(),
        recipient_type: RecipientType::Broadcast,
        recipient_id: None,
        body: "team update".to_string(),
        ..MailMessage::default()
    };
    if let Err(err) = repo.create_message(&mut broadcast) {
        panic!("create broadcast: {err}");
    }

    // Verify broadcast message was created.
    let got = match repo.get_message(&broadcast.id) {
        Ok(value) => value,
        Err(err) => panic!("get broadcast: {err}"),
    };
    assert_eq!(got.body, "team update");
    assert_eq!(got.recipient_type, RecipientType::Broadcast);

    let _ = std::fs::remove_file(path);
}

#[test]
fn not_found_paths() {
    let (db, path) = open_migrated("not-found");
    let repo = MailRepository::new(&db);

    let thread_err = repo.get_thread("missing-thread");
    assert!(
        matches!(thread_err, Err(DbError::MailThreadNotFound)),
        "expected MailThreadNotFound, got {thread_err:?}"
    );

    let msg_err = repo.get_message("missing-message");
    assert!(
        matches!(msg_err, Err(DbError::MailMessageNotFound)),
        "expected MailMessageNotFound, got {msg_err:?}"
    );

    let read_err = repo.mark_read("missing-message");
    assert!(
        matches!(read_err, Err(DbError::MailMessageNotFound)),
        "expected MailMessageNotFound from mark_read, got {read_err:?}"
    );

    let ack_err = repo.mark_acked("missing-message");
    assert!(
        matches!(ack_err, Err(DbError::MailMessageNotFound)),
        "expected MailMessageNotFound from mark_acked, got {ack_err:?}"
    );

    let _ = std::fs::remove_file(path);
}
