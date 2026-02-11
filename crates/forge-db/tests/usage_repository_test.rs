use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::usage_repository::{UsageQuery, UsageRecord, UsageRepository};
use forge_db::{Config, Db, DbError};
use rusqlite::params;

fn temp_db_path(tag: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    std::env::temp_dir().join(format!(
        "forge-db-usage-repo-{tag}-{nanos}-{}.sqlite",
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

fn insert_account(db: &Db, id: &str, provider: &str, profile_name: &str) {
    if let Err(err) = db.conn().execute(
        "INSERT INTO accounts (id, provider, profile_name, credential_ref, is_active)
         VALUES (?1, ?2, ?3, ?4, 1)",
        params![id, provider, profile_name, format!("cred-{profile_name}")],
    ) {
        panic!("insert account: {err}");
    }
}

#[test]
fn create_get_and_defaults_parity() {
    let (db, path) = open_migrated("create-get");
    insert_account(&db, "acct-1", "anthropic", "profile-1");
    let repo = UsageRepository::new(&db);

    let mut record = UsageRecord {
        account_id: "acct-1".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-opus".to_string(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_cents: 15,
        ..UsageRecord::default()
    };

    if let Err(err) = repo.create(&mut record) {
        panic!("create: {err}");
    }
    assert!(!record.id.is_empty(), "id should be generated");
    assert_eq!(record.total_tokens, 1500);
    assert_eq!(record.request_count, 1);

    let got = match repo.get(&record.id) {
        Ok(value) => value,
        Err(err) => panic!("get: {err}"),
    };
    assert_eq!(got.account_id, "acct-1");
    assert_eq!(got.model, "claude-3-opus");
    assert_eq!(got.input_tokens, 1000);

    let _ = std::fs::remove_file(path);
}

#[test]
fn query_filters_account_provider_and_since() {
    let (db, path) = open_migrated("query");
    insert_account(&db, "acct-1", "anthropic", "profile-a");
    insert_account(&db, "acct-2", "openai", "profile-b");
    let repo = UsageRepository::new(&db);

    let mut r1 = UsageRecord {
        account_id: "acct-1".to_string(),
        provider: "anthropic".to_string(),
        input_tokens: 100,
        recorded_at: "2026-01-10T09:00:00Z".to_string(),
        ..UsageRecord::default()
    };
    let mut r2 = UsageRecord {
        account_id: "acct-1".to_string(),
        provider: "anthropic".to_string(),
        input_tokens: 200,
        recorded_at: "2026-01-10T10:00:00Z".to_string(),
        ..UsageRecord::default()
    };
    let mut r3 = UsageRecord {
        account_id: "acct-2".to_string(),
        provider: "openai".to_string(),
        input_tokens: 300,
        recorded_at: "2026-01-10T11:00:00Z".to_string(),
        ..UsageRecord::default()
    };

    if let Err(err) = repo.create(&mut r1) {
        panic!("create r1: {err}");
    }
    if let Err(err) = repo.create(&mut r2) {
        panic!("create r2: {err}");
    }
    if let Err(err) = repo.create(&mut r3) {
        panic!("create r3: {err}");
    }

    let by_account = match repo.query(UsageQuery {
        account_id: Some("acct-1".to_string()),
        ..UsageQuery::default()
    }) {
        Ok(rows) => rows,
        Err(err) => panic!("query by account: {err}"),
    };
    assert_eq!(by_account.len(), 2);

    let by_provider = match repo.query(UsageQuery {
        provider: Some("anthropic".to_string()),
        ..UsageQuery::default()
    }) {
        Ok(rows) => rows,
        Err(err) => panic!("query by provider: {err}"),
    };
    assert_eq!(by_provider.len(), 2);

    let by_since = match repo.query(UsageQuery {
        since: Some("2026-01-10T09:30:00Z".to_string()),
        ..UsageQuery::default()
    }) {
        Ok(rows) => rows,
        Err(err) => panic!("query by since: {err}"),
    };
    assert_eq!(by_since.len(), 2);

    let _ = std::fs::remove_file(path);
}

#[test]
fn summarize_by_account_and_all_match_expected_totals() {
    let (db, path) = open_migrated("summary");
    insert_account(&db, "acct-1", "anthropic", "profile-a");
    insert_account(&db, "acct-2", "openai", "profile-b");
    let repo = UsageRepository::new(&db);

    let mut rows = [
        UsageRecord {
            account_id: "acct-1".to_string(),
            provider: "anthropic".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            cost_cents: 5,
            request_count: 1,
            recorded_at: "2026-01-10T09:00:00Z".to_string(),
            ..UsageRecord::default()
        },
        UsageRecord {
            account_id: "acct-1".to_string(),
            provider: "anthropic".to_string(),
            input_tokens: 200,
            output_tokens: 25,
            cost_cents: 7,
            request_count: 2,
            recorded_at: "2026-01-10T10:00:00Z".to_string(),
            ..UsageRecord::default()
        },
        UsageRecord {
            account_id: "acct-2".to_string(),
            provider: "openai".to_string(),
            input_tokens: 300,
            output_tokens: 30,
            cost_cents: 20,
            request_count: 1,
            recorded_at: "2026-01-10T11:00:00Z".to_string(),
            ..UsageRecord::default()
        },
    ];
    for row in &mut rows {
        if let Err(err) = repo.create(row) {
            panic!("create row: {err}");
        }
    }

    let account_summary = match repo.summarize_by_account("acct-1", None, None) {
        Ok(summary) => summary,
        Err(err) => panic!("summarize_by_account: {err}"),
    };
    assert_eq!(account_summary.input_tokens, 300);
    assert_eq!(account_summary.output_tokens, 75);
    assert_eq!(account_summary.total_tokens, 375);
    assert_eq!(account_summary.total_cost_cents, 12);
    assert_eq!(account_summary.request_count, 3);
    assert_eq!(account_summary.record_count, 2);

    let all_summary = match repo.summarize_all(None, None) {
        Ok(summary) => summary,
        Err(err) => panic!("summarize_all: {err}"),
    };
    assert_eq!(all_summary.input_tokens, 600);
    assert_eq!(all_summary.total_cost_cents, 32);
    assert_eq!(all_summary.record_count, 3);

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_daily_usage_groups_by_date() {
    let (db, path) = open_migrated("daily");
    insert_account(&db, "acct-1", "anthropic", "profile-a");
    let repo = UsageRepository::new(&db);

    let mut today_a = UsageRecord {
        account_id: "acct-1".to_string(),
        provider: "anthropic".to_string(),
        input_tokens: 100,
        recorded_at: "2026-01-10T09:00:00Z".to_string(),
        ..UsageRecord::default()
    };
    let mut today_b = UsageRecord {
        account_id: "acct-1".to_string(),
        provider: "anthropic".to_string(),
        input_tokens: 200,
        recorded_at: "2026-01-10T10:00:00Z".to_string(),
        ..UsageRecord::default()
    };
    let mut yesterday = UsageRecord {
        account_id: "acct-1".to_string(),
        provider: "anthropic".to_string(),
        input_tokens: 300,
        recorded_at: "2026-01-09T08:00:00Z".to_string(),
        ..UsageRecord::default()
    };

    if let Err(err) = repo.create(&mut today_a) {
        panic!("create today_a: {err}");
    }
    if let Err(err) = repo.create(&mut today_b) {
        panic!("create today_b: {err}");
    }
    if let Err(err) = repo.create(&mut yesterday) {
        panic!("create yesterday: {err}");
    }

    let daily =
        match repo.get_daily_usage("acct-1", "2026-01-08T00:00:00Z", "2026-01-11T00:00:00Z", 30) {
            Ok(rows) => rows,
            Err(err) => panic!("get_daily_usage: {err}"),
        };
    assert_eq!(daily.len(), 2);

    let mut today_tokens = 0i64;
    for row in &daily {
        if row.date == "2026-01-10" {
            today_tokens = row.input_tokens;
        }
    }
    assert_eq!(today_tokens, 300);

    let _ = std::fs::remove_file(path);
}

#[test]
fn top_accounts_delete_and_cache_update_work() {
    let (db, path) = open_migrated("top-delete-cache");
    insert_account(&db, "acct-a", "anthropic", "profile-a");
    insert_account(&db, "acct-b", "anthropic", "profile-b");
    insert_account(&db, "acct-c", "anthropic", "profile-c");
    let repo = UsageRepository::new(&db);

    let mut a = UsageRecord {
        account_id: "acct-a".to_string(),
        provider: "anthropic".to_string(),
        total_tokens: 1000,
        recorded_at: "2026-01-10T08:00:00Z".to_string(),
        ..UsageRecord::default()
    };
    let mut b = UsageRecord {
        account_id: "acct-b".to_string(),
        provider: "anthropic".to_string(),
        total_tokens: 500,
        recorded_at: "2026-01-10T09:00:00Z".to_string(),
        ..UsageRecord::default()
    };
    let mut c = UsageRecord {
        account_id: "acct-c".to_string(),
        provider: "anthropic".to_string(),
        total_tokens: 2000,
        input_tokens: 1200,
        output_tokens: 800,
        request_count: 2,
        cost_cents: 33,
        recorded_at: "2026-01-10T10:00:00Z".to_string(),
        ..UsageRecord::default()
    };

    if let Err(err) = repo.create(&mut a) {
        panic!("create a: {err}");
    }
    if let Err(err) = repo.create(&mut b) {
        panic!("create b: {err}");
    }
    if let Err(err) = repo.create(&mut c) {
        panic!("create c: {err}");
    }

    let top = match repo.get_top_accounts_by_usage(None, None, 10) {
        Ok(rows) => rows,
        Err(err) => panic!("get_top_accounts_by_usage: {err}"),
    };
    assert_eq!(top.len(), 3);
    assert_eq!(top[0].total_tokens, 2000);
    assert_eq!(top[2].total_tokens, 500);

    if let Err(err) = repo.update_daily_cache("acct-c", "2026-01-10", "anthropic") {
        panic!("update_daily_cache: {err}");
    }

    let cache_row: (i64, i64, i64, i64, i64) = match db.conn().query_row(
        "SELECT input_tokens, output_tokens, total_tokens, cost_cents, record_count
         FROM daily_usage_cache
         WHERE account_id = ?1 AND date = ?2 AND provider = ?3",
        params!["acct-c", "2026-01-10", "anthropic"],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    ) {
        Ok(value) => value,
        Err(err) => panic!("query cache row: {err}"),
    };
    assert_eq!(cache_row.0, 1200);
    assert_eq!(cache_row.1, 800);
    assert_eq!(cache_row.2, 2000);
    assert_eq!(cache_row.3, 33);
    assert_eq!(cache_row.4, 1);

    let deleted = match repo.delete_older_than("2026-01-10T08:30:00Z", 100) {
        Ok(count) => count,
        Err(err) => panic!("delete_older_than: {err}"),
    };
    assert_eq!(deleted, 1);

    if let Err(err) = repo.delete(&b.id) {
        panic!("delete: {err}");
    }
    let deleted_lookup = repo.get(&b.id);
    assert!(
        matches!(deleted_lookup, Err(DbError::UsageRecordNotFound)),
        "expected UsageRecordNotFound after delete, got {deleted_lookup:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_validation_rejects_missing_required_fields() {
    let (db, path) = open_migrated("validation");
    let repo = UsageRepository::new(&db);

    let mut missing_account = UsageRecord {
        provider: "anthropic".to_string(),
        ..UsageRecord::default()
    };
    let err = repo.create(&mut missing_account);
    assert!(
        matches!(err, Err(DbError::InvalidUsageRecord)),
        "expected InvalidUsageRecord, got {err:?}"
    );

    let mut missing_provider = UsageRecord {
        account_id: "acct-1".to_string(),
        ..UsageRecord::default()
    };
    let err = repo.create(&mut missing_provider);
    assert!(
        matches!(err, Err(DbError::InvalidUsageRecord)),
        "expected InvalidUsageRecord, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}
