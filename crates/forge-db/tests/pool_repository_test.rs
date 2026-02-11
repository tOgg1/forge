use forge_db::pool_repository::{Pool, PoolMember, PoolRepository};
use forge_db::profile_repository::{Profile, ProfileRepository};
use forge_db::{Config, Db, DbError};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(tag: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    std::env::temp_dir().join(format!(
        "forge-db-pool-repo-{tag}-{nanos}-{}.sqlite",
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

fn sample_pool(name: &str) -> Pool {
    Pool {
        name: name.to_string(),
        strategy: "round_robin".to_string(),
        ..Pool::default()
    }
}

fn sample_profile(name: &str) -> Profile {
    Profile {
        name: name.to_string(),
        harness: "pi".to_string(),
        command_template: "pi -p \"{prompt}\"".to_string(),
        max_concurrency: 1,
        prompt_mode: "path".to_string(),
        ..Profile::default()
    }
}

// -- Create tests -----------------------------------------------------------

#[test]
fn create_assigns_id_and_timestamps() {
    let (db, path) = open_migrated("create-ids");
    let repo = PoolRepository::new(&db);
    let mut p = sample_pool("test-pool");
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    assert!(!p.id.is_empty(), "id should be generated");
    assert!(!p.created_at.is_empty());
    assert!(!p.updated_at.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_preserves_given_id() {
    let (db, path) = open_migrated("create-given-id");
    let repo = PoolRepository::new(&db);
    let mut p = sample_pool("given-id-pool");
    p.id = "my-custom-pool-id".to_string();
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    assert_eq!(p.id, "my-custom-pool-id");

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_duplicate_name_returns_already_exists() {
    let (db, path) = open_migrated("create-dup");
    let repo = PoolRepository::new(&db);
    let mut p1 = sample_pool("dup-pool");
    match repo.create(&mut p1) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    let mut p2 = sample_pool("dup-pool");
    let err = repo.create(&mut p2);
    assert!(
        matches!(err, Err(DbError::PoolAlreadyExists)),
        "expected PoolAlreadyExists, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_validation_requires_name() {
    let (db, path) = open_migrated("create-no-name");
    let repo = PoolRepository::new(&db);
    let mut p = Pool::default();
    let err = repo.create(&mut p);
    assert!(matches!(err, Err(DbError::Validation(_))));

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_defaults_strategy_to_round_robin() {
    let (db, path) = open_migrated("create-default-strat");
    let repo = PoolRepository::new(&db);
    let mut p = Pool {
        name: "default-strat".to_string(),
        ..Pool::default()
    };
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    let fetched = match repo.get(&p.id) {
        Ok(f) => f,
        Err(e) => panic!("get: {e}"),
    };
    assert_eq!(fetched.strategy, "round_robin");

    let _ = std::fs::remove_file(path);
}

// -- Get tests --------------------------------------------------------------

#[test]
fn get_by_id() {
    let (db, path) = open_migrated("get-id");
    let repo = PoolRepository::new(&db);
    let mut p = sample_pool("get-pool");
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    let fetched = match repo.get(&p.id) {
        Ok(f) => f,
        Err(e) => panic!("get: {e}"),
    };
    assert_eq!(fetched.id, p.id);
    assert_eq!(fetched.name, "get-pool");
    assert_eq!(fetched.strategy, "round_robin");
    assert!(!fetched.is_default);

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_not_found() {
    let (db, path) = open_migrated("get-404");
    let repo = PoolRepository::new(&db);
    let err = repo.get("nonexistent");
    assert!(
        matches!(err, Err(DbError::PoolNotFound)),
        "expected PoolNotFound, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_by_name_works() {
    let (db, path) = open_migrated("get-name");
    let repo = PoolRepository::new(&db);
    let mut p = sample_pool("named-pool");
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    let fetched = match repo.get_by_name("named-pool") {
        Ok(f) => f,
        Err(e) => panic!("get_by_name: {e}"),
    };
    assert_eq!(fetched.id, p.id);

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_by_name_not_found() {
    let (db, path) = open_migrated("get-name-404");
    let repo = PoolRepository::new(&db);
    let err = repo.get_by_name("nope");
    assert!(matches!(err, Err(DbError::PoolNotFound)));

    let _ = std::fs::remove_file(path);
}

// -- GetDefault tests -------------------------------------------------------

#[test]
fn get_default_not_found_when_none_set() {
    let (db, path) = open_migrated("get-default-none");
    let repo = PoolRepository::new(&db);

    let mut p = sample_pool("non-default");
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    let err = repo.get_default();
    assert!(matches!(err, Err(DbError::PoolNotFound)));

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_default_returns_default_pool() {
    let (db, path) = open_migrated("get-default");
    let repo = PoolRepository::new(&db);

    let mut p = sample_pool("default-pool");
    p.is_default = true;
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    let fetched = match repo.get_default() {
        Ok(f) => f,
        Err(e) => panic!("get_default: {e}"),
    };
    assert_eq!(fetched.id, p.id);
    assert!(fetched.is_default);

    let _ = std::fs::remove_file(path);
}

// -- List tests -------------------------------------------------------------

#[test]
fn list_empty() {
    let (db, path) = open_migrated("list-empty");
    let repo = PoolRepository::new(&db);
    let pools = match repo.list() {
        Ok(l) => l,
        Err(e) => panic!("list: {e}"),
    };
    assert!(pools.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_returns_ordered_by_name() {
    let (db, path) = open_migrated("list-order");
    let repo = PoolRepository::new(&db);

    let mut p1 = sample_pool("charlie");
    match repo.create(&mut p1) {
        Ok(()) => {}
        Err(e) => panic!("create p1: {e}"),
    }
    let mut p2 = sample_pool("alpha");
    match repo.create(&mut p2) {
        Ok(()) => {}
        Err(e) => panic!("create p2: {e}"),
    }
    let mut p3 = sample_pool("bravo");
    match repo.create(&mut p3) {
        Ok(()) => {}
        Err(e) => panic!("create p3: {e}"),
    }

    let pools = match repo.list() {
        Ok(l) => l,
        Err(e) => panic!("list: {e}"),
    };
    assert_eq!(pools.len(), 3);
    assert_eq!(pools[0].name, "alpha");
    assert_eq!(pools[1].name, "bravo");
    assert_eq!(pools[2].name, "charlie");

    let _ = std::fs::remove_file(path);
}

// -- Update tests -----------------------------------------------------------

#[test]
fn update_changes_fields() {
    let (db, path) = open_migrated("update");
    let repo = PoolRepository::new(&db);
    let mut p = sample_pool("upd-pool");
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    p.strategy = "lru".to_string();
    p.is_default = true;
    match repo.update(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("update: {e}"),
    }

    let fetched = match repo.get(&p.id) {
        Ok(f) => f,
        Err(e) => panic!("get: {e}"),
    };
    assert_eq!(fetched.strategy, "lru");
    assert!(fetched.is_default);

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_not_found() {
    let (db, path) = open_migrated("update-404");
    let repo = PoolRepository::new(&db);
    let mut p = sample_pool("phantom");
    p.id = "no-such-id".to_string();
    let err = repo.update(&mut p);
    assert!(matches!(err, Err(DbError::PoolNotFound)));

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_refreshes_updated_at() {
    let (db, path) = open_migrated("update-ts");
    let repo = PoolRepository::new(&db);
    let mut p = sample_pool("ts-pool");
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }
    let original_updated = p.updated_at.clone();

    std::thread::sleep(std::time::Duration::from_millis(1100));

    p.strategy = "lru".to_string();
    match repo.update(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("update: {e}"),
    }

    assert_ne!(
        p.updated_at, original_updated,
        "updated_at should change after update"
    );

    let _ = std::fs::remove_file(path);
}

// -- Delete tests -----------------------------------------------------------

#[test]
fn delete_removes_pool() {
    let (db, path) = open_migrated("delete");
    let repo = PoolRepository::new(&db);
    let mut p = sample_pool("del-pool");
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    match repo.delete(&p.id) {
        Ok(()) => {}
        Err(e) => panic!("delete: {e}"),
    }

    let err = repo.get(&p.id);
    assert!(matches!(err, Err(DbError::PoolNotFound)));

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_not_found() {
    let (db, path) = open_migrated("delete-404");
    let repo = PoolRepository::new(&db);
    let err = repo.delete("no-such-id");
    assert!(matches!(err, Err(DbError::PoolNotFound)));

    let _ = std::fs::remove_file(path);
}

// -- SetDefault tests -------------------------------------------------------

#[test]
fn set_default_clears_others() {
    let (db, path) = open_migrated("set-default");
    let repo = PoolRepository::new(&db);

    let mut p1 = sample_pool("pool-a");
    p1.is_default = true;
    match repo.create(&mut p1) {
        Ok(()) => {}
        Err(e) => panic!("create p1: {e}"),
    }

    let mut p2 = sample_pool("pool-b");
    match repo.create(&mut p2) {
        Ok(()) => {}
        Err(e) => panic!("create p2: {e}"),
    }

    match repo.set_default(&p2.id) {
        Ok(()) => {}
        Err(e) => panic!("set_default: {e}"),
    }

    let fetched1 = match repo.get(&p1.id) {
        Ok(f) => f,
        Err(e) => panic!("get p1: {e}"),
    };
    assert!(!fetched1.is_default, "p1 should no longer be default");

    let fetched2 = match repo.get(&p2.id) {
        Ok(f) => f,
        Err(e) => panic!("get p2: {e}"),
    };
    assert!(fetched2.is_default, "p2 should be default");

    let default = match repo.get_default() {
        Ok(f) => f,
        Err(e) => panic!("get_default: {e}"),
    };
    assert_eq!(default.id, p2.id);

    let _ = std::fs::remove_file(path);
}

#[test]
fn set_default_not_found() {
    let (db, path) = open_migrated("set-default-404");
    let repo = PoolRepository::new(&db);
    let err = repo.set_default("nonexistent");
    assert!(matches!(err, Err(DbError::PoolNotFound)));

    let _ = std::fs::remove_file(path);
}

// -- Member tests -----------------------------------------------------------

#[test]
fn add_and_list_members() {
    let (db, path) = open_migrated("members");
    let pool_repo = PoolRepository::new(&db);
    let profile_repo = ProfileRepository::new(&db);

    let mut profile = sample_profile("pi-default");
    match profile_repo.create(&mut profile) {
        Ok(()) => {}
        Err(e) => panic!("create profile: {e}"),
    }

    let mut pool = sample_pool("member-pool");
    match pool_repo.create(&mut pool) {
        Ok(()) => {}
        Err(e) => panic!("create pool: {e}"),
    }

    let mut member = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile.id.clone(),
        weight: 2,
        position: 0,
        ..PoolMember::default()
    };
    match pool_repo.add_member(&mut member) {
        Ok(()) => {}
        Err(e) => panic!("add_member: {e}"),
    }

    assert!(!member.id.is_empty(), "member id should be generated");
    assert!(!member.created_at.is_empty());
    assert_eq!(member.weight, 2);

    let members = match pool_repo.list_members(&pool.id) {
        Ok(m) => m,
        Err(e) => panic!("list_members: {e}"),
    };
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].profile_id, profile.id);
    assert_eq!(members[0].weight, 2);

    let _ = std::fs::remove_file(path);
}

#[test]
fn add_member_defaults_weight_to_1() {
    let (db, path) = open_migrated("member-weight");
    let pool_repo = PoolRepository::new(&db);
    let profile_repo = ProfileRepository::new(&db);

    let mut profile = sample_profile("pi-weight");
    match profile_repo.create(&mut profile) {
        Ok(()) => {}
        Err(e) => panic!("create profile: {e}"),
    }

    let mut pool = sample_pool("weight-pool");
    match pool_repo.create(&mut pool) {
        Ok(()) => {}
        Err(e) => panic!("create pool: {e}"),
    }

    let mut member = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile.id.clone(),
        ..PoolMember::default()
    };
    match pool_repo.add_member(&mut member) {
        Ok(()) => {}
        Err(e) => panic!("add_member: {e}"),
    }

    assert_eq!(member.weight, 1, "weight should default to 1");

    let _ = std::fs::remove_file(path);
}

#[test]
fn add_member_duplicate_returns_already_exists() {
    let (db, path) = open_migrated("member-dup");
    let pool_repo = PoolRepository::new(&db);
    let profile_repo = ProfileRepository::new(&db);

    let mut profile = sample_profile("pi-dup");
    match profile_repo.create(&mut profile) {
        Ok(()) => {}
        Err(e) => panic!("create profile: {e}"),
    }

    let mut pool = sample_pool("dup-member-pool");
    match pool_repo.create(&mut pool) {
        Ok(()) => {}
        Err(e) => panic!("create pool: {e}"),
    }

    let mut m1 = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile.id.clone(),
        ..PoolMember::default()
    };
    match pool_repo.add_member(&mut m1) {
        Ok(()) => {}
        Err(e) => panic!("add_member: {e}"),
    }

    let mut m2 = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile.id.clone(),
        ..PoolMember::default()
    };
    let err = pool_repo.add_member(&mut m2);
    assert!(
        matches!(err, Err(DbError::PoolAlreadyExists)),
        "expected PoolAlreadyExists, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn remove_member() {
    let (db, path) = open_migrated("remove-member");
    let pool_repo = PoolRepository::new(&db);
    let profile_repo = ProfileRepository::new(&db);

    let mut profile = sample_profile("pi-remove");
    match profile_repo.create(&mut profile) {
        Ok(()) => {}
        Err(e) => panic!("create profile: {e}"),
    }

    let mut pool = sample_pool("remove-pool");
    match pool_repo.create(&mut pool) {
        Ok(()) => {}
        Err(e) => panic!("create pool: {e}"),
    }

    let mut member = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile.id.clone(),
        ..PoolMember::default()
    };
    match pool_repo.add_member(&mut member) {
        Ok(()) => {}
        Err(e) => panic!("add_member: {e}"),
    }

    match pool_repo.remove_member(&pool.id, &profile.id) {
        Ok(()) => {}
        Err(e) => panic!("remove_member: {e}"),
    }

    let members = match pool_repo.list_members(&pool.id) {
        Ok(m) => m,
        Err(e) => panic!("list_members: {e}"),
    };
    assert!(members.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn remove_member_not_found() {
    let (db, path) = open_migrated("remove-member-404");
    let pool_repo = PoolRepository::new(&db);
    let err = pool_repo.remove_member("no-pool", "no-profile");
    assert!(matches!(err, Err(DbError::PoolNotFound)));

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_members_empty() {
    let (db, path) = open_migrated("list-members-empty");
    let pool_repo = PoolRepository::new(&db);

    let mut pool = sample_pool("empty-members");
    match pool_repo.create(&mut pool) {
        Ok(()) => {}
        Err(e) => panic!("create pool: {e}"),
    }

    let members = match pool_repo.list_members(&pool.id) {
        Ok(m) => m,
        Err(e) => panic!("list_members: {e}"),
    };
    assert!(members.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_members_ordered_by_position_then_created_at() {
    let (db, path) = open_migrated("members-order");
    let pool_repo = PoolRepository::new(&db);
    let profile_repo = ProfileRepository::new(&db);

    let mut profile1 = sample_profile("pi-order-1");
    match profile_repo.create(&mut profile1) {
        Ok(()) => {}
        Err(e) => panic!("create profile1: {e}"),
    }
    let mut profile2 = sample_profile("pi-order-2");
    match profile_repo.create(&mut profile2) {
        Ok(()) => {}
        Err(e) => panic!("create profile2: {e}"),
    }
    let mut profile3 = sample_profile("pi-order-3");
    match profile_repo.create(&mut profile3) {
        Ok(()) => {}
        Err(e) => panic!("create profile3: {e}"),
    }

    let mut pool = sample_pool("order-pool");
    match pool_repo.create(&mut pool) {
        Ok(()) => {}
        Err(e) => panic!("create pool: {e}"),
    }

    // Add members with different positions
    let mut m1 = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile1.id.clone(),
        position: 2,
        ..PoolMember::default()
    };
    match pool_repo.add_member(&mut m1) {
        Ok(()) => {}
        Err(e) => panic!("add m1: {e}"),
    }

    let mut m2 = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile2.id.clone(),
        position: 0,
        ..PoolMember::default()
    };
    match pool_repo.add_member(&mut m2) {
        Ok(()) => {}
        Err(e) => panic!("add m2: {e}"),
    }

    let mut m3 = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile3.id.clone(),
        position: 1,
        ..PoolMember::default()
    };
    match pool_repo.add_member(&mut m3) {
        Ok(()) => {}
        Err(e) => panic!("add m3: {e}"),
    }

    let members = match pool_repo.list_members(&pool.id) {
        Ok(m) => m,
        Err(e) => panic!("list_members: {e}"),
    };
    assert_eq!(members.len(), 3);
    assert_eq!(members[0].profile_id, profile2.id, "position 0 first");
    assert_eq!(members[1].profile_id, profile3.id, "position 1 second");
    assert_eq!(members[2].profile_id, profile1.id, "position 2 third");

    let _ = std::fs::remove_file(path);
}

// -- Cascade tests ----------------------------------------------------------

#[test]
fn delete_pool_cascades_to_members() {
    let (db, path) = open_migrated("cascade-members");
    let pool_repo = PoolRepository::new(&db);
    let profile_repo = ProfileRepository::new(&db);

    let mut profile = sample_profile("pi-cascade");
    match profile_repo.create(&mut profile) {
        Ok(()) => {}
        Err(e) => panic!("create profile: {e}"),
    }

    let mut pool = sample_pool("cascade-pool");
    match pool_repo.create(&mut pool) {
        Ok(()) => {}
        Err(e) => panic!("create pool: {e}"),
    }

    let mut member = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile.id.clone(),
        ..PoolMember::default()
    };
    match pool_repo.add_member(&mut member) {
        Ok(()) => {}
        Err(e) => panic!("add_member: {e}"),
    }

    match pool_repo.delete(&pool.id) {
        Ok(()) => {}
        Err(e) => panic!("delete pool: {e}"),
    }

    // Members should be cascaded
    let count: i64 = match db.conn().query_row(
        "SELECT COUNT(1) FROM pool_members WHERE pool_id = ?1",
        rusqlite::params![pool.id],
        |row| row.get(0),
    ) {
        Ok(c) => c,
        Err(e) => panic!("count members: {e}"),
    };
    assert_eq!(count, 0, "pool members should be cascade deleted");

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_profile_cascades_to_pool_members() {
    let (db, path) = open_migrated("cascade-profile");
    let pool_repo = PoolRepository::new(&db);
    let profile_repo = ProfileRepository::new(&db);

    let mut profile = sample_profile("pi-profile-cascade");
    match profile_repo.create(&mut profile) {
        Ok(()) => {}
        Err(e) => panic!("create profile: {e}"),
    }

    let mut pool = sample_pool("profile-cascade-pool");
    match pool_repo.create(&mut pool) {
        Ok(()) => {}
        Err(e) => panic!("create pool: {e}"),
    }

    let mut member = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile.id.clone(),
        ..PoolMember::default()
    };
    match pool_repo.add_member(&mut member) {
        Ok(()) => {}
        Err(e) => panic!("add_member: {e}"),
    }

    match profile_repo.delete(&profile.id) {
        Ok(()) => {}
        Err(e) => panic!("delete profile: {e}"),
    }

    let members = match pool_repo.list_members(&pool.id) {
        Ok(m) => m,
        Err(e) => panic!("list_members: {e}"),
    };
    assert!(
        members.is_empty(),
        "pool members should cascade when profile deleted"
    );

    let _ = std::fs::remove_file(path);
}

// -- Metadata roundtrip tests -----------------------------------------------

#[test]
fn metadata_roundtrip() {
    let (db, path) = open_migrated("metadata");
    let repo = PoolRepository::new(&db);
    let mut p = sample_pool("metadata-pool");
    let mut meta = HashMap::new();
    meta.insert("region".into(), serde_json::json!("us-east"));
    meta.insert("tier".into(), serde_json::json!(1));
    p.metadata = Some(meta);
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    let fetched = match repo.get(&p.id) {
        Ok(f) => f,
        Err(e) => panic!("get: {e}"),
    };
    let m = match fetched.metadata.as_ref() {
        Some(m) => m,
        None => panic!("metadata should be Some"),
    };
    assert_eq!(m.get("region"), Some(&serde_json::json!("us-east")));
    assert_eq!(m.get("tier"), Some(&serde_json::json!(1)));

    let _ = std::fs::remove_file(path);
}

#[test]
fn null_metadata_roundtrip() {
    let (db, path) = open_migrated("null-metadata");
    let repo = PoolRepository::new(&db);
    let mut p = sample_pool("null-meta-pool");
    match repo.create(&mut p) {
        Ok(()) => {}
        Err(e) => panic!("create: {e}"),
    }

    let fetched = match repo.get(&p.id) {
        Ok(f) => f,
        Err(e) => panic!("get: {e}"),
    };
    assert!(fetched.metadata.is_none());

    let _ = std::fs::remove_file(path);
}

// -- Full parity integration test (mirrors Go TestPoolRepository) -----------

#[test]
fn full_parity_create_default_members() {
    let (db, path) = open_migrated("full-parity");
    let pool_repo = PoolRepository::new(&db);
    let profile_repo = ProfileRepository::new(&db);

    let mut profile = sample_profile("pi-parity");
    match profile_repo.create(&mut profile) {
        Ok(()) => {}
        Err(e) => panic!("create profile: {e}"),
    }

    let mut pool = sample_pool("default");
    match pool_repo.create(&mut pool) {
        Ok(()) => {}
        Err(e) => panic!("create pool: {e}"),
    }

    let mut member = PoolMember {
        pool_id: pool.id.clone(),
        profile_id: profile.id.clone(),
        weight: 2,
        ..PoolMember::default()
    };
    match pool_repo.add_member(&mut member) {
        Ok(()) => {}
        Err(e) => panic!("add_member: {e}"),
    }

    let members = match pool_repo.list_members(&pool.id) {
        Ok(m) => m,
        Err(e) => panic!("list_members: {e}"),
    };
    assert_eq!(members.len(), 1);

    match pool_repo.set_default(&pool.id) {
        Ok(()) => {}
        Err(e) => panic!("set_default: {e}"),
    }

    let updated = match pool_repo.get(&pool.id) {
        Ok(f) => f,
        Err(e) => panic!("get: {e}"),
    };
    assert!(updated.is_default, "expected pool to be default");

    let _ = std::fs::remove_file(path);
}
