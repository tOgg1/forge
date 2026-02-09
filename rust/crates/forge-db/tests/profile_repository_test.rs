//! Profile repository integration tests — Go parity coverage.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::profile_repository::{Profile, ProfileRepository};
use forge_db::{Config, Db, DbError};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn temp_db_path(prefix: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-profile-{prefix}-{nanos}-{}-{suffix}.sqlite",
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

fn make_profile(name: &str, harness: &str, cmd: &str) -> Profile {
    Profile {
        name: name.to_string(),
        harness: harness.to_string(),
        command_template: cmd.to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Go parity: TestProfileRepository_CreateGetUpdate
// ---------------------------------------------------------------------------

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

    if let Err(err) = repo.create(&mut profile) {
        panic!("create: {err}");
    }
    assert!(!profile.id.is_empty(), "ID should be assigned");
    assert!(!profile.created_at.is_empty(), "created_at should be set");
    assert!(!profile.updated_at.is_empty(), "updated_at should be set");

    // Get by ID
    let fetched = match repo.get(&profile.id) {
        Ok(p) => p,
        Err(err) => panic!("get by id: {err}"),
    };
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

    if let Err(err) = repo.update(&mut updated) {
        panic!("update: {err}");
    }

    // GetByName
    let refetched = match repo.get_by_name("pi-test") {
        Ok(p) => p,
        Err(err) => panic!("get by name: {err}"),
    };
    assert_eq!(refetched.model, "claude-opus");
    assert!(refetched.cooldown_until.is_some(), "cooldown should be set");
    assert_eq!(
        refetched.cooldown_until,
        Some(cooldown),
        "cooldown value mismatch"
    );

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Create behavior
// ---------------------------------------------------------------------------

#[test]
fn create_assigns_uuid() {
    let (db, path) = setup_db("create-uuid");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("auto-id", "claude", "claude run");
    assert!(p.id.is_empty());

    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }
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

    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }
    assert_eq!(p.prompt_mode, "env");

    let fetched = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get: {err}"),
    };
    assert_eq!(fetched.prompt_mode, "env");

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_sets_timestamps() {
    let (db, path) = setup_db("timestamps");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("ts-test", "pi", "pi run");
    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    assert!(!p.created_at.is_empty());
    assert!(!p.updated_at.is_empty());
    assert!(p.created_at.ends_with('Z'), "should be RFC3339 UTC");
    assert!(p.updated_at.ends_with('Z'), "should be RFC3339 UTC");

    let _ = std::fs::remove_file(path);
}

#[test]
fn provided_id_is_preserved() {
    let (db, path) = setup_db("custom-id");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("custom-id-test", "pi", "pi run");
    p.id = "my-custom-id-123".to_string();

    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }
    assert_eq!(p.id, "my-custom-id-123");

    let fetched = match repo.get("my-custom-id-123") {
        Ok(p) => p,
        Err(err) => panic!("get: {err}"),
    };
    assert_eq!(fetched.name, "custom-id-test");

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Unique constraint
// ---------------------------------------------------------------------------

#[test]
fn duplicate_name_returns_already_exists() {
    let (db, path) = setup_db("dup-name");
    let repo = ProfileRepository::new(&db);

    let mut p1 = make_profile("dup", "pi", "pi run");
    if let Err(err) = repo.create(&mut p1) {
        panic!("create first: {err}");
    }

    let mut p2 = make_profile("dup", "claude", "claude run");
    let result = repo.create(&mut p2);
    assert!(
        matches!(result, Err(DbError::ProfileAlreadyExists)),
        "expected ProfileAlreadyExists, got: {result:?}"
    );

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Not-found errors
// ---------------------------------------------------------------------------

#[test]
fn get_nonexistent_returns_not_found() {
    let (db, path) = setup_db("get-missing");
    let repo = ProfileRepository::new(&db);

    let result = repo.get("nonexistent-id");
    assert!(
        matches!(result, Err(DbError::ProfileNotFound)),
        "expected ProfileNotFound, got: {result:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_by_name_nonexistent_returns_not_found() {
    let (db, path) = setup_db("getbyname-missing");
    let repo = ProfileRepository::new(&db);

    let result = repo.get_by_name("no-such-name");
    assert!(
        matches!(result, Err(DbError::ProfileNotFound)),
        "expected ProfileNotFound, got: {result:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_nonexistent_returns_not_found() {
    let (db, path) = setup_db("update-missing");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("ghost", "pi", "pi run");
    p.id = "nonexistent-id".to_string();

    let result = repo.update(&mut p);
    assert!(
        matches!(result, Err(DbError::ProfileNotFound)),
        "expected ProfileNotFound, got: {result:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_nonexistent_returns_not_found() {
    let (db, path) = setup_db("delete-missing");
    let repo = ProfileRepository::new(&db);

    let result = repo.delete("nonexistent-id");
    assert!(
        matches!(result, Err(DbError::ProfileNotFound)),
        "expected ProfileNotFound, got: {result:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn set_cooldown_nonexistent_returns_not_found() {
    let (db, path) = setup_db("cooldown-missing");
    let repo = ProfileRepository::new(&db);

    let result = repo.set_cooldown("nonexistent-id", Some("2026-02-09T19:00:00Z"));
    assert!(
        matches!(result, Err(DbError::ProfileNotFound)),
        "expected ProfileNotFound, got: {result:?}"
    );

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

#[test]
fn list_returns_profiles_ordered_by_name() {
    let (db, path) = setup_db("list-order");
    let repo = ProfileRepository::new(&db);

    let mut pa = make_profile("charlie", "pi", "pi run");
    let mut pb = make_profile("alpha", "claude", "claude run");
    let mut pc = make_profile("bravo", "codex", "codex run");

    if let Err(err) = repo.create(&mut pa) {
        panic!("create charlie: {err}");
    }
    if let Err(err) = repo.create(&mut pb) {
        panic!("create alpha: {err}");
    }
    if let Err(err) = repo.create(&mut pc) {
        panic!("create bravo: {err}");
    }

    let all = match repo.list() {
        Ok(v) => v,
        Err(err) => panic!("list: {err}"),
    };
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

    let all = match repo.list() {
        Ok(v) => v,
        Err(err) => panic!("list: {err}"),
    };
    assert!(all.is_empty());

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Update behavior
// ---------------------------------------------------------------------------

#[test]
fn update_preserves_id_and_created_at() {
    let (db, path) = setup_db("update-preserve");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("preserve-test", "pi", "pi run");
    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    let original_id = p.id.clone();
    let original_created = p.created_at.clone();

    p.name = "preserve-test-updated".to_string();
    if let Err(err) = repo.update(&mut p) {
        panic!("update: {err}");
    }

    let fetched = match repo.get(&original_id) {
        Ok(p) => p,
        Err(err) => panic!("get: {err}"),
    };
    assert_eq!(fetched.id, original_id);
    assert_eq!(fetched.created_at, original_created);
    assert_eq!(fetched.name, "preserve-test-updated");

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_bumps_updated_at() {
    let (db, path) = setup_db("update-ts");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("ts-bump", "pi", "pi run");
    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    let original_updated = p.updated_at.clone();

    // Sleep briefly to ensure timestamp differs
    std::thread::sleep(std::time::Duration::from_millis(1100));

    p.model = "new-model".to_string();
    if let Err(err) = repo.update(&mut p) {
        panic!("update: {err}");
    }

    assert_ne!(p.updated_at, original_updated, "updated_at should change");

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_changes_extra_args_and_env() {
    let (db, path) = setup_db("update-json");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("json-update", "pi", "pi run");
    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    // Add extra_args and env
    p.extra_args = vec!["--flag".to_string()];
    let mut env = HashMap::new();
    env.insert("KEY".to_string(), "val".to_string());
    p.env = env;
    if let Err(err) = repo.update(&mut p) {
        panic!("update add: {err}");
    }

    let f1 = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get after add: {err}"),
    };
    assert_eq!(f1.extra_args, vec!["--flag"]);
    assert_eq!(f1.env.get("KEY").map(String::as_str), Some("val"));

    // Clear extra_args and env
    p.extra_args = Vec::new();
    p.env = HashMap::new();
    if let Err(err) = repo.update(&mut p) {
        panic!("update clear: {err}");
    }

    let f2 = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get after clear: {err}"),
    };
    assert!(f2.extra_args.is_empty());
    assert!(f2.env.is_empty());

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[test]
fn delete_removes_profile() {
    let (db, path) = setup_db("delete");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("to-delete", "pi", "pi run");
    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    if let Err(err) = repo.delete(&p.id) {
        panic!("delete: {err}");
    }

    let result = repo.get(&p.id);
    assert!(
        matches!(result, Err(DbError::ProfileNotFound)),
        "expected ProfileNotFound after delete, got: {result:?}"
    );

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Cooldown
// ---------------------------------------------------------------------------

#[test]
fn set_cooldown_sets_and_clears() {
    let (db, path) = setup_db("cooldown");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("cooldown-test", "claude", "claude run");
    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    // Set cooldown
    let cooldown = "2026-02-09T19:00:00Z";
    if let Err(err) = repo.set_cooldown(&p.id, Some(cooldown)) {
        panic!("set cooldown: {err}");
    }

    let fetched = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get after set: {err}"),
    };
    assert_eq!(fetched.cooldown_until.as_deref(), Some(cooldown));

    // Clear cooldown
    if let Err(err) = repo.set_cooldown(&p.id, None) {
        panic!("clear cooldown: {err}");
    }

    let fetched2 = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get after clear: {err}"),
    };
    assert!(fetched2.cooldown_until.is_none());

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// JSON field roundtrips
// ---------------------------------------------------------------------------

#[test]
fn extra_args_json_roundtrip() {
    let (db, path) = setup_db("extra-args");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("args-test", "codex", "codex run");
    p.extra_args = vec!["--verbose".to_string(), "--timeout=30".to_string()];

    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    let fetched = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get: {err}"),
    };
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

    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    let fetched = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get: {err}"),
    };
    assert_eq!(fetched.env.len(), 2);
    assert_eq!(
        fetched.env.get("API_KEY").map(String::as_str),
        Some("secret123")
    );
    assert_eq!(
        fetched.env.get("REGION").map(String::as_str),
        Some("us-east")
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn empty_extra_args_and_env_stored_as_null() {
    let (db, path) = setup_db("empty-json");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("null-json", "pi", "pi run");
    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    let fetched = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get: {err}"),
    };
    assert!(fetched.extra_args.is_empty());
    assert!(fetched.env.is_empty());

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Nullable string fields
// ---------------------------------------------------------------------------

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

    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    let fetched = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get: {err}"),
    };
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
    if let Err(err) = repo.create(&mut p) {
        panic!("create: {err}");
    }

    let fetched = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get: {err}"),
    };
    assert_eq!(fetched.auth_kind, "");
    assert_eq!(fetched.auth_home, "");
    assert_eq!(fetched.model, "");

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[test]
fn validation_rejects_empty_name() {
    let (db, path) = setup_db("validate-name");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("", "pi", "pi run");
    let result = repo.create(&mut p);

    match result {
        Err(DbError::Validation(msg)) => {
            assert!(msg.contains("name"), "error should mention name: {msg}");
        }
        other => panic!("expected Validation error, got: {other:?}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_rejects_empty_command_template() {
    let (db, path) = setup_db("validate-cmd");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("valid-name", "pi", "");
    let result = repo.create(&mut p);

    match result {
        Err(DbError::Validation(msg)) => {
            assert!(
                msg.contains("command_template"),
                "error should mention command_template: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other:?}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_rejects_negative_max_concurrency() {
    let (db, path) = setup_db("validate-concurrency");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("neg-conc", "pi", "pi run");
    p.max_concurrency = -1;

    let result = repo.create(&mut p);

    match result {
        Err(DbError::Validation(msg)) => {
            assert!(
                msg.contains("max_concurrency"),
                "error should mention max_concurrency: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other:?}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_rejects_invalid_harness() {
    let (db, path) = setup_db("validate-harness");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("bad-harness", "invalid-harness", "run");
    let result = repo.create(&mut p);

    match result {
        Err(DbError::Validation(msg)) => {
            assert!(
                msg.contains("harness"),
                "error should mention harness: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other:?}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_rejects_invalid_prompt_mode() {
    let (db, path) = setup_db("validate-pm");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("bad-pm", "pi", "pi run");
    p.prompt_mode = "invalid-mode".to_string();

    let result = repo.create(&mut p);

    match result {
        Err(DbError::Validation(msg)) => {
            assert!(
                msg.contains("prompt_mode"),
                "error should mention prompt_mode: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other:?}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_allows_empty_harness() {
    let (db, path) = setup_db("allow-empty-harness");
    let repo = ProfileRepository::new(&db);

    // Go allows empty harness (switch case "": ok)
    let mut p = make_profile("empty-harness", "", "run cmd");
    if let Err(err) = repo.create(&mut p) {
        panic!("create with empty harness should succeed: {err}");
    }

    let fetched = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get: {err}"),
    };
    assert_eq!(fetched.harness, "");

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Enum coverage
// ---------------------------------------------------------------------------

#[test]
fn all_harness_values_accepted() {
    let (db, path) = setup_db("all-harnesses");
    let repo = ProfileRepository::new(&db);

    let harnesses = ["pi", "opencode", "codex", "claude", "droid"];
    for (i, h) in harnesses.iter().enumerate() {
        let mut p = make_profile(&format!("harness-{i}"), h, "run cmd");
        if let Err(err) = repo.create(&mut p) {
            panic!("create with harness {h} failed: {err}");
        }

        let fetched = match repo.get(&p.id) {
            Ok(p) => p,
            Err(err) => panic!("get harness {h}: {err}"),
        };
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
        if let Err(err) = repo.create(&mut p) {
            panic!("create with prompt_mode {m} failed: {err}");
        }

        let fetched = match repo.get(&p.id) {
            Ok(p) => p,
            Err(err) => panic!("get prompt_mode {m}: {err}"),
        };
        assert_eq!(fetched.prompt_mode, *m);
    }

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Max concurrency edge case
// ---------------------------------------------------------------------------

#[test]
fn max_concurrency_zero_is_valid() {
    let (db, path) = setup_db("zero-concurrency");
    let repo = ProfileRepository::new(&db);

    let mut p = make_profile("zero-conc", "pi", "pi run");
    p.max_concurrency = 0;
    if let Err(err) = repo.create(&mut p) {
        panic!("create with max_concurrency=0: {err}");
    }

    let fetched = match repo.get(&p.id) {
        Ok(p) => p,
        Err(err) => panic!("get: {err}"),
    };
    assert_eq!(fetched.max_concurrency, 0);

    let _ = std::fs::remove_file(path);
}
