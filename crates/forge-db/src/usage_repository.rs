//! Usage repository — `usage_records` and `daily_usage_cache` parity.

use std::collections::HashMap;

use rusqlite::{params, params_from_iter, types::Value, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

/// A single usage row, mirroring Go `models.UsageRecord`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageRecord {
    pub id: String,
    pub account_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cost_cents: i64,
    pub request_count: i64,
    pub recorded_at: String,
    pub metadata: Option<HashMap<String, String>>,
}

/// Aggregated usage, mirroring Go `models.UsageSummary`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageSummary {
    pub account_id: String,
    pub provider: String,
    pub period: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub total_cost_cents: i64,
    pub request_count: i64,
    pub record_count: i64,
}

/// Daily aggregate row, mirroring Go `models.DailyUsage`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DailyUsage {
    pub date: String,
    pub account_id: String,
    pub provider: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cost_cents: i64,
    pub request_count: i64,
}

/// Query filters for usage record listing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageQuery {
    pub account_id: Option<String>,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: i64,
}

fn now_rfc3339() -> String {
    let now = std::time::SystemTime::now();
    let duration = match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => std::time::Duration::from_secs(0),
    };
    let secs = duration.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_civil(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_civil(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn nullable_string(value: &str) -> Option<&str> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn push_time_filters(
    query: &mut String,
    args: &mut Vec<Value>,
    since: Option<&str>,
    until: Option<&str>,
) {
    if let Some(since) = since {
        query.push_str(" AND recorded_at >= ?");
        args.push(Value::from(since.to_string()));
    }
    if let Some(until) = until {
        query.push_str(" AND recorded_at < ?");
        args.push(Value::from(until.to_string()));
    }
}

fn scan_usage_record_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageRecord> {
    let metadata_json: Option<String> = row.get(12)?;
    let metadata = match metadata_json {
        Some(data) => serde_json::from_str::<HashMap<String, String>>(&data).ok(),
        None => None,
    };

    Ok(UsageRecord {
        id: row.get(0)?,
        account_id: row.get(1)?,
        agent_id: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
        session_id: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        provider: row.get(4)?,
        model: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        input_tokens: row.get(6)?,
        output_tokens: row.get(7)?,
        total_tokens: row.get(8)?,
        cost_cents: row.get(9)?,
        request_count: row.get(10)?,
        recorded_at: row.get(11)?,
        metadata,
    })
}

/// Repository for usage record persistence.
pub struct UsageRepository<'a> {
    db: &'a Db,
}

impl<'a> UsageRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Insert a new usage record.
    pub fn create(&self, record: &mut UsageRecord) -> Result<(), DbError> {
        if record.account_id.trim().is_empty() || record.provider.trim().is_empty() {
            return Err(DbError::InvalidUsageRecord);
        }
        if record.id.is_empty() {
            record.id = Uuid::new_v4().to_string();
        }
        if record.recorded_at.is_empty() {
            record.recorded_at = now_rfc3339();
        }
        if record.total_tokens == 0 {
            record.total_tokens = record.input_tokens + record.output_tokens;
        }
        if record.request_count == 0 {
            record.request_count = 1;
        }

        let metadata_json: Option<String> = match &record.metadata {
            Some(meta) => Some(serde_json::to_string(meta).map_err(|err| {
                DbError::Validation(format!("failed to marshal metadata: {err}"))
            })?),
            None => None,
        };

        self.db.conn().execute(
            "INSERT INTO usage_records (
                id, account_id, agent_id, session_id, provider, model,
                input_tokens, output_tokens, total_tokens, cost_cents,
                request_count, recorded_at, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.id,
                record.account_id,
                nullable_string(&record.agent_id),
                nullable_string(&record.session_id),
                record.provider,
                nullable_string(&record.model),
                record.input_tokens,
                record.output_tokens,
                record.total_tokens,
                record.cost_cents,
                record.request_count,
                record.recorded_at,
                metadata_json,
            ],
        )?;
        Ok(())
    }

    /// Retrieve by ID.
    pub fn get(&self, id: &str) -> Result<UsageRecord, DbError> {
        let row = self
            .db
            .conn()
            .query_row(
                "SELECT id, account_id, agent_id, session_id, provider, model,
                    input_tokens, output_tokens, total_tokens, cost_cents,
                    request_count, recorded_at, metadata_json
                 FROM usage_records WHERE id = ?1",
                params![id],
                scan_usage_record_row,
            )
            .optional()?;
        row.ok_or(DbError::UsageRecordNotFound)
    }

    /// Query using optional filters.
    pub fn query(&self, q: UsageQuery) -> Result<Vec<UsageRecord>, DbError> {
        let limit = if q.limit <= 0 { 100 } else { q.limit };
        let mut query = String::from(
            "SELECT id, account_id, agent_id, session_id, provider, model,
                input_tokens, output_tokens, total_tokens, cost_cents,
                request_count, recorded_at, metadata_json
             FROM usage_records WHERE 1=1",
        );
        let mut args: Vec<Value> = Vec::new();

        if let Some(account_id) = q.account_id {
            query.push_str(" AND account_id = ?");
            args.push(Value::from(account_id));
        }
        if let Some(agent_id) = q.agent_id {
            query.push_str(" AND agent_id = ?");
            args.push(Value::from(agent_id));
        }
        if let Some(session_id) = q.session_id {
            query.push_str(" AND session_id = ?");
            args.push(Value::from(session_id));
        }
        if let Some(provider) = q.provider {
            query.push_str(" AND provider = ?");
            args.push(Value::from(provider));
        }
        if let Some(model) = q.model {
            query.push_str(" AND model = ?");
            args.push(Value::from(model));
        }
        push_time_filters(
            &mut query,
            &mut args,
            q.since.as_deref(),
            q.until.as_deref(),
        );

        query.push_str(" ORDER BY recorded_at DESC LIMIT ?");
        args.push(Value::from(limit));

        let mut stmt = self.db.conn().prepare(&query)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), scan_usage_record_row)?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Delete by ID.
    pub fn delete(&self, id: &str) -> Result<(), DbError> {
        let rows = self
            .db
            .conn()
            .execute("DELETE FROM usage_records WHERE id = ?1", params![id])?;
        if rows == 0 {
            return Err(DbError::UsageRecordNotFound);
        }
        Ok(())
    }

    /// Aggregate usage for one account.
    pub fn summarize_by_account(
        &self,
        account_id: &str,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<UsageSummary, DbError> {
        let mut query = String::from(
            "SELECT
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(cost_cents), 0),
                COALESCE(SUM(request_count), 0),
                COUNT(*)
             FROM usage_records WHERE account_id = ?",
        );
        let mut args = vec![Value::from(account_id.to_string())];
        push_time_filters(&mut query, &mut args, since, until);

        let (input, output, total, cost, requests, count): (i64, i64, i64, i64, i64, i64) = self
            .db
            .conn()
            .query_row(&query, params_from_iter(args.iter()), |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?;

        Ok(UsageSummary {
            account_id: account_id.to_string(),
            provider: String::new(),
            period: "custom".to_string(),
            period_start: since.map(ToString::to_string),
            period_end: until.map(ToString::to_string),
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            total_cost_cents: cost,
            request_count: requests,
            record_count: count,
        })
    }

    /// Aggregate usage for one provider.
    pub fn summarize_by_provider(
        &self,
        provider: &str,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<UsageSummary, DbError> {
        let mut query = String::from(
            "SELECT
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(cost_cents), 0),
                COALESCE(SUM(request_count), 0),
                COUNT(*)
             FROM usage_records WHERE provider = ?",
        );
        let mut args = vec![Value::from(provider.to_string())];
        push_time_filters(&mut query, &mut args, since, until);

        let (input, output, total, cost, requests, count): (i64, i64, i64, i64, i64, i64) = self
            .db
            .conn()
            .query_row(&query, params_from_iter(args.iter()), |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?;

        Ok(UsageSummary {
            account_id: String::new(),
            provider: provider.to_string(),
            period: "custom".to_string(),
            period_start: since.map(ToString::to_string),
            period_end: until.map(ToString::to_string),
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            total_cost_cents: cost,
            request_count: requests,
            record_count: count,
        })
    }

    /// Aggregate usage across all accounts.
    pub fn summarize_all(
        &self,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<UsageSummary, DbError> {
        let mut query = String::from(
            "SELECT
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(cost_cents), 0),
                COALESCE(SUM(request_count), 0),
                COUNT(*)
             FROM usage_records WHERE 1=1",
        );
        let mut args = Vec::new();
        push_time_filters(&mut query, &mut args, since, until);

        let (input, output, total, cost, requests, count): (i64, i64, i64, i64, i64, i64) = self
            .db
            .conn()
            .query_row(&query, params_from_iter(args.iter()), |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?;

        Ok(UsageSummary {
            account_id: String::new(),
            provider: String::new(),
            period: "all".to_string(),
            period_start: since.map(ToString::to_string),
            period_end: until.map(ToString::to_string),
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            total_cost_cents: cost,
            request_count: requests,
            record_count: count,
        })
    }

    /// Aggregate by day for one account.
    pub fn get_daily_usage(
        &self,
        account_id: &str,
        since: &str,
        until: &str,
        limit: i64,
    ) -> Result<Vec<DailyUsage>, DbError> {
        let limit = if limit <= 0 { 30 } else { limit };

        let mut stmt = self.db.conn().prepare(
            "SELECT
                date(recorded_at) as date,
                provider,
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(cost_cents), 0),
                COALESCE(SUM(request_count), 0)
             FROM usage_records
             WHERE account_id = ?1 AND recorded_at >= ?2 AND recorded_at < ?3
             GROUP BY date(recorded_at), provider
             ORDER BY date DESC
             LIMIT ?4",
        )?;

        let rows = stmt.query_map(params![account_id, since, until, limit], |row| {
            Ok(DailyUsage {
                date: row.get(0)?,
                account_id: account_id.to_string(),
                provider: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                total_tokens: row.get(4)?,
                cost_cents: row.get(5)?,
                request_count: row.get(6)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Return top accounts ordered by total token usage.
    pub fn get_top_accounts_by_usage(
        &self,
        since: Option<&str>,
        until: Option<&str>,
        limit: i64,
    ) -> Result<Vec<UsageSummary>, DbError> {
        let limit = if limit <= 0 { 10 } else { limit };
        let mut query = String::from(
            "SELECT
                account_id,
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(cost_cents), 0),
                COALESCE(SUM(request_count), 0),
                COUNT(*)
             FROM usage_records WHERE 1=1",
        );
        let mut args = Vec::new();
        push_time_filters(&mut query, &mut args, since, until);
        query.push_str(" GROUP BY account_id ORDER BY total_tokens DESC LIMIT ?");
        args.push(Value::from(limit));

        let mut stmt = self.db.conn().prepare(&query)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |row| {
            Ok(UsageSummary {
                account_id: row.get(0)?,
                provider: String::new(),
                period: "custom".to_string(),
                period_start: since.map(ToString::to_string),
                period_end: until.map(ToString::to_string),
                input_tokens: row.get(1)?,
                output_tokens: row.get(2)?,
                total_tokens: row.get(3)?,
                total_cost_cents: row.get(4)?,
                request_count: row.get(5)?,
                record_count: row.get(6)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Refresh one day/provider cache row.
    pub fn update_daily_cache(
        &self,
        account_id: &str,
        date: &str,
        provider: &str,
    ) -> Result<(), DbError> {
        self.db.conn().execute(
            "INSERT OR REPLACE INTO daily_usage_cache (
                account_id, date, provider,
                input_tokens, output_tokens, total_tokens,
                cost_cents, request_count, record_count, updated_at
            )
            SELECT
                account_id,
                date(recorded_at) as date,
                provider,
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(cost_cents), 0),
                COALESCE(SUM(request_count), 0),
                COUNT(*),
                datetime('now')
            FROM usage_records
            WHERE account_id = ?1 AND date(recorded_at) = ?2 AND provider = ?3
            GROUP BY account_id, date(recorded_at), provider",
            params![account_id, date, provider],
        )?;
        Ok(())
    }

    /// Delete old rows, capped by limit.
    pub fn delete_older_than(&self, before: &str, limit: i64) -> Result<i64, DbError> {
        let limit = if limit <= 0 { 1000 } else { limit };
        let rows = self.db.conn().execute(
            "DELETE FROM usage_records WHERE id IN (
                SELECT id FROM usage_records WHERE recorded_at < ?1 ORDER BY recorded_at LIMIT ?2
            )",
            params![before, limit],
        )?;
        Ok(rows as i64)
    }
}
