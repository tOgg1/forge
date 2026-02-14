//! Team task repository integration tests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::team_repository::TeamService;
use forge_db::team_task_repository::{TeamTaskFilter, TeamTaskRepository, TeamTaskService};
use forge_db::{Config, Db, DbError};

fn temp_db_path(prefix: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-team-task-{prefix}-{nanos}-{}-{suffix}.sqlite",
        std::process::id(),
    ))
}

fn setup_db(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let mut db = match Db::open(Config::new(&path)) {
        Ok(db) => db,
        Err(err) => panic!("open db: {err}"),
    };
    if let Err(err) = db.migrate_up() {
        panic!("migrate_up: {err}");
    }
    (db, path)
}

fn create_team(service: &TeamService<'_>, name: &str) -> String {
    match service.create_team(name, "{}", "", 30) {
        Ok(team) => team.id,
        Err(err) => panic!("create team failed: {err}"),
    }
}

#[test]
fn submit_and_list_queue_orders_by_priority() {
    let (db, path) = setup_db("submit-list");
    let team_service = TeamService::new(&db);
    let team_id = create_team(&team_service, "ops-q");
    let service = TeamTaskService::new(&db);

    let low = service
        .submit(&team_id, r#"{"type":"build","title":"low"}"#, 300)
        .unwrap_or_else(|err| panic!("submit low: {err}"));
    let high = service
        .submit(&team_id, r#"{"type":"build","title":"high"}"#, 10)
        .unwrap_or_else(|err| panic!("submit high: {err}"));
    let medium = service
        .submit(&team_id, r#"{"type":"build","title":"medium"}"#, 100)
        .unwrap_or_else(|err| panic!("submit medium: {err}"));

    let queue = service
        .list_queue(&team_id, 10)
        .unwrap_or_else(|err| panic!("list_queue: {err}"));
    assert_eq!(queue.len(), 3);
    assert_eq!(queue[0].id, high.id);
    assert_eq!(queue[1].id, medium.id);
    assert_eq!(queue[2].id, low.id);

    let _ = std::fs::remove_file(path);
}

#[test]
fn submit_validates_payload_schema() {
    let (db, path) = setup_db("payload-validation");
    let team_service = TeamService::new(&db);
    let team_id = create_team(&team_service, "ops-schema");
    let service = TeamTaskService::new(&db);

    let bad = service.submit(&team_id, r#"{"type":"build"}"#, 50);
    assert!(
        matches!(bad, Err(DbError::Validation(_))),
        "expected validation error, got: {bad:?}"
    );

    let bad = service.submit(&team_id, r#"["nope"]"#, 50);
    assert!(
        matches!(bad, Err(DbError::Validation(_))),
        "expected validation error, got: {bad:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn assign_reassign_complete_and_event_audit_work() {
    let (db, path) = setup_db("lifecycle-complete");
    let team_service = TeamService::new(&db);
    let team_id = create_team(&team_service, "ops-life");
    let service = TeamTaskService::new(&db);
    let repo = TeamTaskRepository::new(&db);

    let task = service
        .submit(&team_id, r#"{"type":"triage","title":"incident"}"#, 20)
        .unwrap_or_else(|err| panic!("submit: {err}"));

    let assigned = service
        .assign(&task.id, "agent-a", Some("scheduler"))
        .unwrap_or_else(|err| panic!("assign: {err}"));
    assert_eq!(assigned.status, "assigned");
    assert_eq!(assigned.assigned_agent_id, "agent-a");

    let reassigned = service
        .reassign(&task.id, "agent-b", Some("scheduler"))
        .unwrap_or_else(|err| panic!("reassign: {err}"));
    assert_eq!(reassigned.status, "assigned");
    assert_eq!(reassigned.assigned_agent_id, "agent-b");

    let done = service
        .complete(&task.id, Some("agent-b"), Some("resolved"))
        .unwrap_or_else(|err| panic!("complete: {err}"));
    assert_eq!(done.status, "done");
    assert!(done.finished_at.is_some());

    let invalid = service.fail(&task.id, Some("agent-b"), Some("should fail"));
    assert!(
        matches!(invalid, Err(DbError::Validation(_))),
        "expected validation error, got: {invalid:?}"
    );

    let events = repo
        .list_events(&task.id)
        .unwrap_or_else(|err| panic!("list_events: {err}"));
    let event_types = events
        .iter()
        .map(|event| event.event_type.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        event_types,
        vec!["submitted", "assigned", "reassigned", "completed"]
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn fail_transition_and_filters_work() {
    let (db, path) = setup_db("lifecycle-fail");
    let team_service = TeamService::new(&db);
    let team_id = create_team(&team_service, "ops-fail");
    let service = TeamTaskService::new(&db);
    let repo = TeamTaskRepository::new(&db);

    let task = service
        .submit(&team_id, r#"{"type":"repair","title":"db lock"}"#, 50)
        .unwrap_or_else(|err| panic!("submit: {err}"));
    let task = service
        .assign(&task.id, "agent-z", Some("scheduler"))
        .unwrap_or_else(|err| panic!("assign: {err}"));
    let failed = service
        .fail(&task.id, Some("agent-z"), Some("panic"))
        .unwrap_or_else(|err| panic!("fail: {err}"));
    assert_eq!(failed.status, "failed");
    assert!(failed.finished_at.is_some());

    let filtered = repo
        .list(&TeamTaskFilter {
            team_id: team_id.clone(),
            statuses: vec!["failed".to_owned()],
            assigned_agent_id: "agent-z".to_owned(),
            limit: 20,
        })
        .unwrap_or_else(|err| panic!("list filtered: {err}"));
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, failed.id);

    let _ = std::fs::remove_file(path);
}

#[test]
fn state_persists_across_reopen() {
    let path = temp_db_path("persist");
    {
        let mut db = Db::open(Config::new(&path)).unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate_up: {err}"));
        let team_service = TeamService::new(&db);
        let team_id = create_team(&team_service, "ops-persist");
        let task_service = TeamTaskService::new(&db);
        task_service
            .submit(&team_id, r#"{"type":"ship","title":"release"}"#, 5)
            .unwrap_or_else(|err| panic!("submit: {err}"));
    }

    {
        let db = Db::open(Config::new(&path)).unwrap_or_else(|err| panic!("reopen db: {err}"));
        let repo = TeamTaskRepository::new(&db);
        let listed = repo
            .list(&TeamTaskFilter {
                team_id: TeamService::new(&db)
                    .show_team("ops-persist")
                    .unwrap_or_else(|err| panic!("show_team: {err}"))
                    .id,
                statuses: vec!["queued".to_owned()],
                assigned_agent_id: String::new(),
                limit: 10,
            })
            .unwrap_or_else(|err| panic!("list after reopen: {err}"));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, "queued");
    }

    let _ = std::fs::remove_file(path);
}
