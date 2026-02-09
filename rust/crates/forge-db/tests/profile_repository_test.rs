//! Profile repository integration tests — Go parity coverage.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::profile_repository::{Profile, ProfileRepository};
use forge_db::{Config, Db, DbError};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn temp_db_path(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "forge-db-profile-{tag}-{nanos}-{}.sqlite",
        std::process::id()
    ))
}

fn setup_db(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let mut db = Db::open(Config::new(&path)).expect("open db");
    db.migrate_up().expect("migrate up");
    (db, path)
}

fn make_profile(name: &str, harness: &str, cmd: &str) -> Profile {
    Profile {
        name: name.to_string(),
        harness: harness.to_string(),
        command_template: cmd.to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Mirrors Go TestProfileRepository_CreateGetUpdate.
#[test]
fn create_get_update_roundtrip() {
    let (db, path) = setup_db("create-get-update");

    let repo = ProfileRepository::new(&db);

    let mut profile = Profile {
        name: "pi-test".to_string(),
        harness: "pi".to_string(),
        command_template: r#"pi -p "{prompt}""#.to_string(),
        max_concurrency: 2,
        prompt_mode: "path".to_string(),
        ..Default::default()
    };

    repo.create(&mut profile).expect("create");
    assert!(!profile.id.is_empty(), "ID should be assigned");
    assert!(!profile.created_at.is_empty(), "created_at should be set");
    assert!(!profile.updated_at.is_empty(), "updated_at should be set");

    // Get by ID
    let fetched = repo.get(&profile.id).expect("get by id");
    assert_eq!(fetched.name, "pi-test");
    assert_eq!(fetched.harness, "pi");
    assert_eq!(fetched.prompt_mode, "path");
    assert_eq!(fetched.max_concurrency, 2);
    assert_eq!(fetched.command_template, r#"pi -p "{prompt}""#);

    // Update with model + cooldown
    let mut updated = fetched;
    updated.model = "claude-opus".to_string();
    let cooldown = "2026-02-09T18:00:00Z".to_string();
    updated.cooldown_until = Some(cooldown.clone());

    repo.update(&mut updated).expect("update");

    // GetByName
    let refetched = repo.get_by_name("pi-test").expect("get by name");
    assert_eq!(refetched.model, "claude-opus");
    assert!(refetched.cooldown_until.is_some(), "cooldown should be set");
    assert_eq!(refetched.cooldown_until.unwrap(), cooldown);

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_assigns_uuid() {
    let (db, path) = setup_db("create-uuid");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("auto-id", "claude", "claude run");
    assert!(p.id.is_empty());

    repo.create(&mut p).expect("create");
    assert!(!p.id.is_empty());
    assert!(p.id.len() == 36, "should be UUID format: {}", p.id);

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_defaults_prompt_mode_to_env() {
    let (db, path) = setup_db("default-prompt-mode");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("default-pm", "codex", "codex run");
    p.prompt_mode = String::new();

    repo.create(&mut p).expect("create");
    assert_eq!(p.prompt_mode, "env");

    let fetched = repo.get(&p.id).expect("get");
    assert_eq!(fetched.prompt_mode, "env");

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_sets_timestamps() {
    let (db, path) = setup_db("timestamps");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("ts-test", "pi", "pi run");
    repo.create(&mut p).expect("create");

    assert!(!p.created_at.is_empty());
    assert!(!p.updated_at.is_empty());
    assert!(p.created_at.ends_with('Z'), "should be RFC3339 UTC");
    assert!(p.updated_at.ends_with('Z'), "should be RFC3339 UTC");

    let _ = std::fs::remove_file(path);
}

#[test]
fn duplicate_name_returns_already_exists() {
    let (db, path) = setup_db("dup-name");
    let repo = ProfileRepository::new(&db);

    let mut p1 = make_profile("dup", "pi", "pi run");
    repo.create(&mut p1).expect("create first");

    let mut p2 = make_profile("dup", "claude", "claude run");
    let err = repo.create(&mut p2).unwrap_err();

    assert!(
        matches!(err, DbError::ProfileAlreadyExists),
        "expected ProfileAlreadyExists, got: {err}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_nonexistent_returns_not_found() {
    let (db, path) = setup_db("get-missing");
    let repo = ProfileRepository::new(&db);

    let err = repo.get("nonexistent-id").unwrap_err();
    assert!(
        matches!(err, DbError::ProfileNotFound),
        "expected ProfileNotFound, got: {err}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_by_name_nonexistent_returns_not_found() {
    let (db, path) = setup_db("getbyname-missing");
    let repo = ProfileRepository::new(&db);

    let err = repo.get_by_name("no-such-name").unwrap_err();
    assert!(
        matches!(err, DbError::ProfileNotFound),
        "expected ProfileNotFound, got: {err}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_returns_profiles_ordered_by_name() {
    let (db, path) = setup_db("list-order");
    let repo = ProfileRepository::new(&db);

    let mut pa = make_profile("charlie", "pi", "pi run");
    let mut pb = make_profile("alpha", "claude", "claude run");
    let mut pc = make_profile("bravo", "codex", "codex run");

    repo.create(&mut pa).expect("create charlie");
    repo.create(&mut pb).expect("create alpha");
    repo.create(&mut pc).expect("create bravo");

    let all = repo.list().expect("list");
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].name, "alpha");
    assert_eq!(all[1].name, "bravo");
    assert_eq!(all[2].name, "charlie");

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_empty_returns_empty_vec() {
    let (db, path) = setup_db("list-empty");
    let repo = ProfileRepository::new(&db);

    let all = repo.list().expect("list");
    assert!(all.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_nonexistent_returns_not_found() {
    let (db, path) = setup_db("update-missing");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("ghost", "pi", "pi run");
    p.id = "nonexistent-id".to_string();

    let err = repo.update(&mut p).unwrap_err();
    assert!(
        matches!(err, DbError::ProfileNotFound),
        "expected ProfileNotFound, got: {err}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_preserves_id_and_created_at() {
    let (db, path) = setup_db("update-preserve");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("preserve-test", "pi", "pi run");
    repo.create(&mut p).expect("create");

    let original_id = p.id.clone();
    let original_created = p.created_at.clone();

    p.name = "preserve-test-updated".to_string();
    repo.update(&mut p).expect("update");

    let fetched = repo.get(&original_id).expect("get");
    assert_eq!(fetched.id, original_id);
    assert_eq!(fetched.created_at, original_created);
    assert_eq!(fetched.name, "preserve-test-updated");

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_removes_profile() {
    let (db, path) = setup_db("delete");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("to-delete", "pi", "pi run");
    repo.create(&mut p).expect("create");

    repo.delete(&p.id).expect("delete");

    let err = repo.get(&p.id).unwrap_err();
    assert!(matches!(err, DbError::ProfileNotFound));

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_nonexistent_returns_not_found() {
    let (db, path) = setup_db("delete-missing");
    let repo = ProfileRepository::new(&db);

    let err = repo.delete("nonexistent-id").unwrap_err();
    assert!(
        matches!(err, DbError::ProfileNotFound),
        "expected ProfileNotFound, got: {err}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn set_cooldown_sets_and_clears() {
    let (db, path) = setup_db("cooldown");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("cooldown-test", "claude", "claude run");
    repo.create(&mut p).expect("create");

    // Set cooldown
    let cooldown = "2026-02-09T19:00:00Z";
    repo.set_cooldown(&p.id, Some(cooldown))
        .expect("set cooldown");

    let fetched = repo.get(&p.id).expect("get after set");
    assert_eq!(fetched.cooldown_until.as_deref(), Some(cooldown));

    // Clear cooldown
    repo.set_cooldown(&p.id, None).expect("clear cooldown");

    let fetched2 = repo.get(&p.id).expect("get after clear");
    assert!(fetched2.cooldown_until.is_none());

    let _ = std::fs::remove_file(path);
}

#[test]
fn set_cooldown_nonexistent_returns_not_found() {
    let (db, path) = setup_db("cooldown-missing");
    let repo = ProfileRepository::new(&db);

    let err = repo
        .set_cooldown("nonexistent-id", Some("2026-02-09T19:00:00Z"))
        .unwrap_err();
    assert!(
        matches!(err, DbError::ProfileNotFound),
        "expected ProfileNotFound, got: {err}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn extra_args_json_roundtrip() {
    let (db, path) = setup_db("extra-args");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("args-test", "codex", "codex run");
    p.extra_args = vec!["--verbose".to_string(), "--timeout=30".to_string()];

    repo.create(&mut p).expect("create");

    let fetched = repo.get(&p.id).expect("get");
    assert_eq!(fetched.extra_args, vec!["--verbose", "--timeout=30"]);

    let _ = std::fs::remove_file(path);
}

#[test]
fn env_json_roundtrip() {
    let (db, path) = setup_db("env-json");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("env-test", "pi", "pi run");
    let mut env = HashMap::new();
    env.insert("API_KEY".to_string(), "secret123".to_string());
    env.insert("REGION".to_string(), "us-east".to_string());
    p.env = env;

    repo.create(&mut p).expect("create");

    let fetched = repo.get(&p.id).expect("get");
    assert_eq!(fetched.env.len(), 2);
    assert_eq!(fetched.env.get("API_KEY").unwrap(), "secret123");
    assert_eq!(fetched.env.get("REGION").unwrap(), "us-east");

    let _ = std::fs::remove_file(path);
}

#[test]
fn empty_extra_args_and_env_stored_as_null() {
    let (db, path) = setup_db("empty-json");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("null-json", "pi", "pi run");
    repo.create(&mut p).expect("create");

    let fetched = repo.get(&p.id).expect("get");
    assert!(fetched.extra_args.is_empty());
    assert!(fetched.env.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn nullable_fields_roundtrip() {
    let (db, path) = setup_db("nullable");
    let repo = ProfileRepository::new(&db);

    let mut p = Profile {
        name: "nullable-test".to_string(),
        harness: "claude".to_string(),
        command_template: "claude run".to_string(),
        auth_kind: "oauth".to_string(),
        auth_home: "/home/test/.auth".to_string(),
        model: "claude-sonnet".to_string(),
        ..Default::default()
    };

    repo.create(&mut p).expect("create");

    let fetched = repo.get(&p.id).expect("get");
    assert_eq!(fetched.auth_kind, "oauth");
    assert_eq!(fetched.auth_home, "/home/test/.auth");
    assert_eq!(fetched.model, "claude-sonnet");

    let _ = std::fs::remove_file(path);
}

#[test]
fn empty_nullable_fields_read_as_empty_string() {
    let (db, path) = setup_db("empty-nullable");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("empty-nullable-test", "pi", "pi run");
    repo.create(&mut p).expect("create");

    let fetched = repo.get(&p.id).expect("get");
    assert_eq!(fetched.auth_kind, "");
    assert_eq!(fetched.auth_home, "");
    assert_eq!(fetched.model, "");

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_rejects_empty_name() {
    let (db, path) = setup_db("validate-name");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("", "pi", "pi run");
    let err = repo.create(&mut p).unwrap_err();

    match err {
        DbError::Validation(msg) => {
            assert!(msg.contains("name"), "error should mention name: {msg}");
        }
        other => panic!("expected Validation error, got: {other}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_rejects_empty_command_template() {
    let (db, path) = setup_db("validate-cmd");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("valid-name", "pi", "");
    let err = repo.create(&mut p).unwrap_err();

    match err {
        DbError::Validation(msg) => {
            assert!(
                msg.contains("command_template"),
                "error should mention command_template: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_rejects_negative_max_concurrency() {
    let (db, path) = setup_db("validate-concurrency");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("neg-conc", "pi", "pi run");
    p.max_concurrency = -1;

    let err = repo.create(&mut p).unwrap_err();

    match err {
        DbError::Validation(msg) => {
            assert!(
                msg.contains("max_concurrency"),
                "error should mention max_concurrency: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_rejects_invalid_harness() {
    let (db, path) = setup_db("validate-harness");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("bad-harness", "invalid-harness", "run");
    let err = repo.create(&mut p).unwrap_err();

    match err {
        DbError::Validation(msg) => {
            assert!(
                msg.contains("harness"),
                "error should mention harness: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_rejects_invalid_prompt_mode() {
    let (db, path) = setup_db("validate-pm");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("bad-pm", "pi", "pi run");
    p.prompt_mode = "invalid-mode".to_string();

    let err = repo.create(&mut p).unwrap_err();

    match err {
        DbError::Validation(msg) => {
            assert!(
                msg.contains("prompt_mode"),
                "error should mention prompt_mode: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_allows_empty_harness() {
    let (db, path) = setup_db("allow-empty-harness");
    let repo = ProfileRepository::new(&db);

    // Go allows empty harness (switch case "": ok)
    let mut p = make_profile("empty-harness", "", "run cmd");
    repo.create(&mut p)
        .expect("create with empty harness should succeed");

    let fetched = repo.get(&p.id).expect("get");
    assert_eq!(fetched.harness, "");

    let _ = std::fs::remove_file(path);
}

#[test]
fn all_harness_values_accepted() {
    let (db, path) = setup_db("all-harnesses");
    let repo = ProfileRepository::new(&db);

    let harnesses = ["pi", "opencode", "codex", "claude", "droid"];
    for (i, h) in harnesses.iter().enumerate() {
        let mut p = make_profile(&format!("harness-{i}"), h, "run cmd");
        repo.create(&mut p)
            .unwrap_or_else(|e| panic!("create with harness {h} failed: {e}"));

        let fetched = repo.get(&p.id).expect("get");
        assert_eq!(fetched.harness, *h);
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn all_prompt_mode_values_accepted() {
    let (db, path) = setup_db("all-prompt-modes");
    let repo = ProfileRepository::new(&db);

    let modes = ["env", "stdin", "path"];
    for (i, m) in modes.iter().enumerate() {
        let mut p = make_profile(&format!("pm-{i}"), "pi", "pi run");
        p.prompt_mode = m.to_string();
        repo.create(&mut p)
            .unwrap_or_else(|e| panic!("create with prompt_mode {m} failed: {e}"));

        let fetched = repo.get(&p.id).expect("get");
        assert_eq!(fetched.prompt_mode, *m);
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_changes_extra_args_and_env() {
    let (db, path) = setup_db("update-json");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("json-update", "pi", "pi run");
    repo.create(&mut p).expect("create");

    // Add extra_args and env
    p.extra_args = vec!["--flag".to_string()];
    let mut env = HashMap::new();
    env.insert("KEY".to_string(), "val".to_string());
    p.env = env;
    repo.update(&mut p).expect("update add");

    let f1 = repo.get(&p.id).expect("get after add");
    assert_eq!(f1.extra_args, vec!["--flag"]);
    assert_eq!(f1.env.get("KEY").unwrap(), "val");

    // Clear extra_args and env
    p.extra_args = Vec::new();
    p.env = HashMap::new();
    repo.update(&mut p).expect("update clear");

    let f2 = repo.get(&p.id).expect("get after clear");
    assert!(f2.extra_args.is_empty());
    assert!(f2.env.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_bumps_updated_at() {
    let (db, path) = setup_db("update-ts");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("ts-bump", "pi", "pi run");
    repo.create(&mut p).expect("create");

    let original_updated = p.updated_at.clone();

    // Sleep briefly to ensure timestamp differs
    std::thread::sleep(std::time::Duration::from_millis(1100));

    p.model = "new-model".to_string();
    repo.update(&mut p).expect("update");

    assert_ne!(p.updated_at, original_updated, "updated_at should change");

    let _ = std::fs::remove_file(path);
}

#[test]
fn max_concurrency_zero_is_valid() {
    let (db, path) = setup_db("zero-concurrency");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("zero-conc", "pi", "pi run");
    p.max_concurrency = 0;
    repo.create(&mut p).expect("create with max_concurrency=0");

    let fetched = repo.get(&p.id).expect("get");
    assert_eq!(fetched.max_concurrency, 0);

    let _ = std::fs::remove_file(path);
}

#[test]
fn provided_id_is_preserved() {
    let (db, path) = setup_db("custom-id");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("custom-id-test", "pi", "pi run");
    p.id = "my-custom-id-123".to_string();

    repo.create(&mut p).expect("create");
    assert_eq!(p.id, "my-custom-id-123");

    let fetched = repo.get("my-custom-id-123").expect("get");
    assert_eq!(fetched.name, "custom-id-test");

    let _ = std::fs::remove_file(path);
}
