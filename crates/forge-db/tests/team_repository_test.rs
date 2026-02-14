//! Team repository integration tests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::team_repository::{TeamRepository, TeamRole, TeamService};
use forge_db::{Config, Db, DbError};

fn temp_db_path(prefix: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-team-{prefix}-{nanos}-{}-{suffix}.sqlite",
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

#[test]
fn create_list_show_delete_team_roundtrip() {
    let (db, path) = setup_db("service-roundtrip");
    let service = TeamService::new(&db);

    let team = match service.create_team(
        "ops-alpha",
        r#"{"critical":"leader","fallback":"member"}"#,
        "agent-lead",
        45,
    ) {
        Ok(team) => team,
        Err(err) => panic!("create_team failed: {err}"),
    };

    assert!(!team.id.is_empty());
    assert_eq!(team.name, "ops-alpha");
    assert_eq!(team.heartbeat_interval_seconds, 45);
    assert!(team.delegation_rules_json.contains("critical"));
    assert!(!team.created_at.is_empty());

    let listed = match service.list_teams() {
        Ok(listed) => listed,
        Err(err) => panic!("list_teams failed: {err}"),
    };
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "ops-alpha");

    let shown = match service.show_team("ops-alpha") {
        Ok(team) => team,
        Err(err) => panic!("show_team failed: {err}"),
    };
    assert_eq!(shown.id, team.id);
    assert_eq!(shown.default_assignee, "agent-lead");

    if let Err(err) = service.delete_team("ops-alpha") {
        panic!("delete_team failed: {err}");
    }
    let listed = match service.list_teams() {
        Ok(listed) => listed,
        Err(err) => panic!("list_teams after delete failed: {err}"),
    };
    assert!(listed.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn team_config_validation_rejects_bad_inputs() {
    let (db, path) = setup_db("validation");
    let service = TeamService::new(&db);

    let zero_heartbeat = service.create_team("ops-a", "{}", "agent", 0);
    assert!(
        matches!(zero_heartbeat, Err(DbError::Validation(_))),
        "expected validation error, got: {zero_heartbeat:?}"
    );

    let bad_json = service.create_team("ops-b", "{not-json}", "agent", 30);
    assert!(
        matches!(bad_json, Err(DbError::Validation(_))),
        "expected validation error, got: {bad_json:?}"
    );

    let bad_shape = service.create_team("ops-c", r#"["array-not-object"]"#, "agent", 30);
    assert!(
        matches!(bad_shape, Err(DbError::Validation(_))),
        "expected validation error, got: {bad_shape:?}"
    );

    let bad_assignee = service.create_team("ops-d", "{}", "bad assignee", 30);
    assert!(
        matches!(bad_assignee, Err(DbError::Validation(_))),
        "expected validation error, got: {bad_assignee:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn members_store_roles_and_support_add_remove_list() {
    let (db, path) = setup_db("members");
    let service = TeamService::new(&db);

    let team = match service.create_team("ops-beta", "{}", "agent-lead", 30) {
        Ok(team) => team,
        Err(err) => panic!("create_team failed: {err}"),
    };

    let leader = match service.add_member("ops-beta", "agent-lead", TeamRole::Leader) {
        Ok(member) => member,
        Err(err) => panic!("add leader failed: {err}"),
    };
    assert_eq!(leader.team_id, team.id);
    assert_eq!(leader.role, "leader");

    let member = match service.add_member("ops-beta", "agent-worker", TeamRole::Member) {
        Ok(member) => member,
        Err(err) => panic!("add member failed: {err}"),
    };
    assert_eq!(member.role, "member");

    let listed = match service.list_members("ops-beta") {
        Ok(listed) => listed,
        Err(err) => panic!("list_members failed: {err}"),
    };
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].role, "leader");
    assert_eq!(listed[1].role, "member");

    let duplicate = service.add_member("ops-beta", "agent-worker", TeamRole::Member);
    assert!(
        matches!(duplicate, Err(DbError::TeamMemberAlreadyExists)),
        "expected TeamMemberAlreadyExists, got: {duplicate:?}"
    );

    let repo = TeamRepository::new(&db);
    if let Err(err) = repo.remove_member(&team.id, "agent-worker") {
        panic!("remove_member failed: {err}");
    }
    let missing_remove = repo.remove_member(&team.id, "agent-worker");
    assert!(
        matches!(missing_remove, Err(DbError::TeamMemberNotFound)),
        "expected TeamMemberNotFound, got: {missing_remove:?}"
    );

    let listed = match service.list_members("ops-beta") {
        Ok(listed) => listed,
        Err(err) => panic!("list_members after remove failed: {err}"),
    };
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].agent_id, "agent-lead");

    let _ = std::fs::remove_file(path);
}

#[test]
fn team_name_uniqueness_and_lookup_errors() {
    let (db, path) = setup_db("uniqueness");
    let repo = TeamRepository::new(&db);
    let service = TeamService::new(&db);

    let first = service.create_team("ops-gamma", "{}", "", 20);
    assert!(first.is_ok());
    let duplicate = service.create_team("ops-gamma", "{}", "", 20);
    assert!(
        matches!(duplicate, Err(DbError::TeamAlreadyExists)),
        "expected TeamAlreadyExists, got: {duplicate:?}"
    );

    let missing = repo.get_team_by_name("missing-team");
    assert!(
        matches!(missing, Err(DbError::TeamNotFound)),
        "expected TeamNotFound, got: {missing:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_team_validates_and_persists_config() {
    let (db, path) = setup_db("update");
    let repo = TeamRepository::new(&db);
    let service = TeamService::new(&db);

    let mut team = match service.create_team("ops-delta", r#"{"route":"default"}"#, "", 25) {
        Ok(team) => team,
        Err(err) => panic!("create_team failed: {err}"),
    };

    team.default_assignee = "agent-z".to_owned();
    team.heartbeat_interval_seconds = 90;
    team.delegation_rules_json = r#"{"route":"leader","priority":"critical"}"#.to_owned();
    if let Err(err) = repo.update_team(&mut team) {
        panic!("update_team failed: {err}");
    }

    let fetched = match repo.get_team(&team.id) {
        Ok(team) => team,
        Err(err) => panic!("get_team failed: {err}"),
    };
    assert_eq!(fetched.default_assignee, "agent-z");
    assert_eq!(fetched.heartbeat_interval_seconds, 90);
    assert!(fetched.delegation_rules_json.contains("critical"));

    team.heartbeat_interval_seconds = -1;
    let invalid = repo.update_team(&mut team);
    assert!(
        matches!(invalid, Err(DbError::Validation(_))),
        "expected validation error, got: {invalid:?}"
    );

    let _ = std::fs::remove_file(path);
}
