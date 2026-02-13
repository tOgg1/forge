//! Budget guardrails model: scope budgets, threshold actions, projections, and persistence.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

pub const BUDGET_GUARDRAIL_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetScopeKind {
    Loop,
    Cluster,
    Fleet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetScope {
    pub kind: BudgetScopeKind,
    pub id: String,
}

impl BudgetScope {
    #[must_use]
    pub fn loop_scope(loop_id: &str) -> Self {
        Self {
            kind: BudgetScopeKind::Loop,
            id: normalize_scope_id(loop_id),
        }
    }

    #[must_use]
    pub fn cluster_scope(cluster_id: &str) -> Self {
        Self {
            kind: BudgetScopeKind::Cluster,
            id: normalize_scope_id(cluster_id),
        }
    }

    #[must_use]
    pub fn fleet_scope() -> Self {
        Self {
            kind: BudgetScopeKind::Fleet,
            id: "fleet".to_owned(),
        }
    }

    #[must_use]
    pub fn key(&self) -> String {
        format!(
            "{}:{}",
            scope_kind_key(self.kind),
            normalize_scope_id(&self.id)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BudgetState {
    Healthy,
    Warn,
    Pause,
    HardKill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardrailAction {
    None,
    Warn,
    Pause,
    HardKill,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetThresholds {
    pub warn_ratio: f64,
    pub pause_ratio: f64,
    pub hard_kill_ratio: f64,
}

impl Default for BudgetThresholds {
    fn default() -> Self {
        Self {
            warn_ratio: 0.80,
            pause_ratio: 0.95,
            hard_kill_ratio: 1.00,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetPolicy {
    pub token_budget: Option<u64>,
    pub cost_budget_usd: Option<f64>,
    pub thresholds: BudgetThresholds,
    pub auto_pause: bool,
    pub auto_kill: bool,
}

impl Default for BudgetPolicy {
    fn default() -> Self {
        Self {
            token_budget: None,
            cost_budget_usd: None,
            thresholds: BudgetThresholds::default(),
            auto_pause: true,
            auto_kill: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetUsageTotals {
    pub tokens_used: u64,
    pub cost_used_usd: f64,
    pub tasks_completed: u64,
    pub updated_at_ms: i64,
}

impl Default for BudgetUsageTotals {
    fn default() -> Self {
        Self {
            tokens_used: 0,
            cost_used_usd: 0.0,
            tasks_completed: 0,
            updated_at_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetLedgerEntry {
    pub scope: BudgetScope,
    pub policy: BudgetPolicy,
    pub totals: BudgetUsageTotals,
    pub extension_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetLedgerStore {
    pub schema_version: u32,
    pub entries: Vec<BudgetLedgerEntry>,
}

impl Default for BudgetLedgerStore {
    fn default() -> Self {
        Self {
            schema_version: BUDGET_GUARDRAIL_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

impl BudgetLedgerStore {
    #[must_use]
    pub fn entry(&self, scope: &BudgetScope) -> Option<&BudgetLedgerEntry> {
        let key = scope.key();
        self.entries.iter().find(|entry| entry.scope.key() == key)
    }

    #[must_use]
    pub fn entry_mut(&mut self, scope: &BudgetScope) -> Option<&mut BudgetLedgerEntry> {
        let key = scope.key();
        self.entries
            .iter_mut()
            .find(|entry| entry.scope.key() == key)
    }

    pub fn upsert_entry(&mut self, entry: BudgetLedgerEntry) {
        let key = entry.scope.key();
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.scope.key() == key)
        {
            *existing = normalize_entry(entry);
        } else {
            self.entries.push(normalize_entry(entry));
            self.entries.sort_by_key(|item| item.scope.key());
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetLedgerLoadOutcome {
    pub store: BudgetLedgerStore,
    pub migrated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetUsageSample {
    pub timestamp_ms: i64,
    pub total_tokens_used: u64,
    pub total_cost_used_usd: f64,
    pub total_tasks_completed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetProjection {
    pub tokens_burn_rate_per_hour: f64,
    pub cost_burn_rate_usd_per_hour: f64,
    pub token_exhaustion_seconds: Option<u64>,
    pub cost_exhaustion_seconds: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetEfficiency {
    pub cost_per_task_usd: f64,
    pub tokens_per_task: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetGuardrailDecision {
    pub state: BudgetState,
    pub action: GuardrailAction,
    pub token_ratio: Option<f64>,
    pub cost_ratio: Option<f64>,
    pub projection: BudgetProjection,
    pub efficiency: BudgetEfficiency,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetExtensionPolicy {
    pub token_extension_ratio: f64,
    pub cost_extension_ratio: f64,
    pub min_token_extension: u64,
    pub min_cost_extension_usd: f64,
}

impl Default for BudgetExtensionPolicy {
    fn default() -> Self {
        Self {
            token_extension_ratio: 0.20,
            cost_extension_ratio: 0.20,
            min_token_extension: 100,
            min_cost_extension_usd: 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetExtensionOutcome {
    pub token_added: u64,
    pub cost_added_usd: f64,
    pub extension_count: u32,
}

pub fn record_budget_usage(
    entry: &mut BudgetLedgerEntry,
    delta_tokens: u64,
    delta_cost_usd: f64,
    delta_tasks_completed: u64,
    timestamp_ms: i64,
) {
    entry.totals.tokens_used = entry.totals.tokens_used.saturating_add(delta_tokens);
    entry.totals.cost_used_usd = (entry.totals.cost_used_usd + delta_cost_usd.max(0.0)).max(0.0);
    entry.totals.tasks_completed = entry
        .totals
        .tasks_completed
        .saturating_add(delta_tasks_completed);
    entry.totals.updated_at_ms = timestamp_ms.max(entry.totals.updated_at_ms);
}

#[must_use]
pub fn evaluate_budget_guardrails(
    entry: &BudgetLedgerEntry,
    samples: &[BudgetUsageSample],
) -> BudgetGuardrailDecision {
    let entry = normalize_entry(entry.clone());
    let totals = effective_totals(&entry, samples);
    let projection = project_budget_exhaustion(&entry, &totals, samples);

    let token_ratio = entry
        .policy
        .token_budget
        .filter(|budget| *budget > 0)
        .map(|budget| totals.tokens_used as f64 / budget as f64);
    let cost_ratio = entry
        .policy
        .cost_budget_usd
        .filter(|budget| *budget > 0.0)
        .map(|budget| totals.cost_used_usd / budget);
    let token_state = ratio_state(token_ratio, entry.policy.thresholds);
    let cost_state = ratio_state(cost_ratio, entry.policy.thresholds);
    let state = token_state.max(cost_state);
    let action = action_for_state(state, &entry.policy);

    let mut messages = Vec::new();
    if let Some(ratio) = token_ratio {
        messages.push(format!("token budget {:.0}% used", ratio * 100.0));
    }
    if let Some(ratio) = cost_ratio {
        messages.push(format!("cost budget {:.0}% used", ratio * 100.0));
    }
    if matches!(state, BudgetState::Pause | BudgetState::HardKill) {
        messages.push(format!("guardrail state {}", state_label(state)));
    }

    let efficiency = if totals.tasks_completed == 0 {
        BudgetEfficiency {
            cost_per_task_usd: 0.0,
            tokens_per_task: 0.0,
        }
    } else {
        BudgetEfficiency {
            cost_per_task_usd: totals.cost_used_usd / totals.tasks_completed as f64,
            tokens_per_task: totals.tokens_used as f64 / totals.tasks_completed as f64,
        }
    };

    BudgetGuardrailDecision {
        state,
        action,
        token_ratio,
        cost_ratio,
        projection,
        efficiency,
        messages,
    }
}

#[must_use]
pub fn apply_one_key_budget_extension(
    entry: &mut BudgetLedgerEntry,
    policy: &BudgetExtensionPolicy,
) -> BudgetExtensionOutcome {
    let policy = normalize_extension_policy(*policy);
    let mut token_added = 0u64;
    let mut cost_added = 0.0;

    if let Some(current) = entry.policy.token_budget {
        token_added = proportional_token_extension(
            current,
            policy.token_extension_ratio,
            policy.min_token_extension,
        );
        entry.policy.token_budget = Some(current.saturating_add(token_added));
    } else if policy.min_token_extension > 0 {
        token_added = policy.min_token_extension;
        entry.policy.token_budget = Some(entry.totals.tokens_used.saturating_add(token_added));
    }

    if let Some(current) = entry.policy.cost_budget_usd {
        cost_added = proportional_cost_extension(
            current,
            policy.cost_extension_ratio,
            policy.min_cost_extension_usd,
        );
        entry.policy.cost_budget_usd = Some((current + cost_added).max(0.0));
    } else if policy.min_cost_extension_usd > 0.0 {
        cost_added = policy.min_cost_extension_usd;
        entry.policy.cost_budget_usd = Some((entry.totals.cost_used_usd + cost_added).max(0.0));
    }

    entry.extension_count = entry.extension_count.saturating_add(1);
    BudgetExtensionOutcome {
        token_added,
        cost_added_usd: cost_added,
        extension_count: entry.extension_count,
    }
}

#[must_use]
pub fn persist_budget_ledger(store: &BudgetLedgerStore) -> String {
    let mut warnings = Vec::new();
    let normalized = normalize_store(store.clone(), &mut warnings);
    let mut root = Map::new();
    root.insert(
        "schema_version".to_owned(),
        Value::from(BUDGET_GUARDRAIL_SCHEMA_VERSION),
    );
    root.insert(
        "entries".to_owned(),
        Value::Array(
            normalized
                .entries
                .iter()
                .map(|entry| {
                    let mut item = Map::new();
                    item.insert(
                        "scope_kind".to_owned(),
                        Value::from(scope_kind_key(entry.scope.kind)),
                    );
                    item.insert("scope_id".to_owned(), Value::from(entry.scope.id.clone()));

                    let mut policy = Map::new();
                    match entry.policy.token_budget {
                        Some(value) => {
                            policy.insert("token_budget".to_owned(), Value::from(value));
                        }
                        None => {
                            policy.insert("token_budget".to_owned(), Value::Null);
                        }
                    }
                    match entry.policy.cost_budget_usd {
                        Some(value) => {
                            policy.insert("cost_budget_usd".to_owned(), Value::from(value));
                        }
                        None => {
                            policy.insert("cost_budget_usd".to_owned(), Value::Null);
                        }
                    }
                    policy.insert(
                        "auto_pause".to_owned(),
                        Value::from(entry.policy.auto_pause),
                    );
                    policy.insert("auto_kill".to_owned(), Value::from(entry.policy.auto_kill));

                    let mut thresholds = Map::new();
                    thresholds.insert(
                        "warn_ratio".to_owned(),
                        Value::from(entry.policy.thresholds.warn_ratio),
                    );
                    thresholds.insert(
                        "pause_ratio".to_owned(),
                        Value::from(entry.policy.thresholds.pause_ratio),
                    );
                    thresholds.insert(
                        "hard_kill_ratio".to_owned(),
                        Value::from(entry.policy.thresholds.hard_kill_ratio),
                    );
                    policy.insert("thresholds".to_owned(), Value::Object(thresholds));
                    item.insert("policy".to_owned(), Value::Object(policy));

                    let mut totals = Map::new();
                    totals.insert(
                        "tokens_used".to_owned(),
                        Value::from(entry.totals.tokens_used),
                    );
                    totals.insert(
                        "cost_used_usd".to_owned(),
                        Value::from(entry.totals.cost_used_usd),
                    );
                    totals.insert(
                        "tasks_completed".to_owned(),
                        Value::from(entry.totals.tasks_completed),
                    );
                    totals.insert(
                        "updated_at_ms".to_owned(),
                        Value::from(entry.totals.updated_at_ms),
                    );
                    item.insert("totals".to_owned(), Value::Object(totals));
                    item.insert(
                        "extension_count".to_owned(),
                        Value::from(entry.extension_count),
                    );
                    Value::Object(item)
                })
                .collect(),
        ),
    );

    serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_else(|_| "{}".to_owned())
}

#[must_use]
pub fn restore_budget_ledger(raw: &str) -> BudgetLedgerLoadOutcome {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return BudgetLedgerLoadOutcome {
            store: BudgetLedgerStore::default(),
            migrated: false,
            warnings: Vec::new(),
        };
    }

    let parsed = match serde_json::from_str::<Value>(trimmed) {
        Ok(parsed) => parsed,
        Err(err) => {
            return BudgetLedgerLoadOutcome {
                store: BudgetLedgerStore::default(),
                migrated: false,
                warnings: vec![format!("invalid budget ledger json ({err})")],
            };
        }
    };
    let Some(root) = parsed.as_object() else {
        return BudgetLedgerLoadOutcome {
            store: BudgetLedgerStore::default(),
            migrated: false,
            warnings: vec!["budget ledger must be an object".to_owned()],
        };
    };

    let schema_version = root
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(BUDGET_GUARDRAIL_SCHEMA_VERSION as u64) as u32;
    let mut warnings = Vec::new();
    if schema_version != BUDGET_GUARDRAIL_SCHEMA_VERSION {
        warnings.push(format!(
            "unknown schema_version={schema_version}; parsed as v{BUDGET_GUARDRAIL_SCHEMA_VERSION}"
        ));
    }

    let mut entries = Vec::new();
    if let Some(items) = root.get("entries").and_then(Value::as_array) {
        for item in items {
            if let Some(entry) = parse_entry(item, &mut warnings) {
                entries.push(entry);
            }
        }
    }

    BudgetLedgerLoadOutcome {
        store: normalize_store(
            BudgetLedgerStore {
                schema_version: BUDGET_GUARDRAIL_SCHEMA_VERSION,
                entries,
            },
            &mut warnings,
        ),
        migrated: schema_version != BUDGET_GUARDRAIL_SCHEMA_VERSION,
        warnings,
    }
}

fn parse_entry(value: &Value, warnings: &mut Vec<String>) -> Option<BudgetLedgerEntry> {
    let Some(obj) = value.as_object() else {
        warnings.push("ignored malformed budget entry (not object)".to_owned());
        return None;
    };

    let kind = obj
        .get("scope_kind")
        .and_then(Value::as_str)
        .and_then(parse_scope_kind)
        .unwrap_or(BudgetScopeKind::Loop);
    let id = obj
        .get("scope_id")
        .and_then(Value::as_str)
        .map(normalize_scope_id)
        .unwrap_or_default();
    if id.is_empty() && kind != BudgetScopeKind::Fleet {
        warnings.push("ignored budget entry with empty scope_id".to_owned());
        return None;
    }

    let scope = if kind == BudgetScopeKind::Fleet {
        BudgetScope::fleet_scope()
    } else {
        BudgetScope { kind, id }
    };

    let policy_obj = obj.get("policy").and_then(Value::as_object);
    let mut policy = BudgetPolicy::default();
    if let Some(policy_obj) = policy_obj {
        policy.token_budget = policy_obj
            .get("token_budget")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0);
        policy.cost_budget_usd = policy_obj
            .get("cost_budget_usd")
            .and_then(Value::as_f64)
            .filter(|value| *value > 0.0);
        policy.auto_pause = policy_obj
            .get("auto_pause")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        policy.auto_kill = policy_obj
            .get("auto_kill")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        if let Some(thresholds) = policy_obj.get("thresholds").and_then(Value::as_object) {
            let warn = thresholds
                .get("warn_ratio")
                .and_then(Value::as_f64)
                .unwrap_or(0.80);
            let pause = thresholds
                .get("pause_ratio")
                .and_then(Value::as_f64)
                .unwrap_or(0.95);
            let hard = thresholds
                .get("hard_kill_ratio")
                .and_then(Value::as_f64)
                .unwrap_or(1.0);
            policy.thresholds = normalize_thresholds(BudgetThresholds {
                warn_ratio: warn,
                pause_ratio: pause,
                hard_kill_ratio: hard,
            });
        }
    }

    let totals_obj = obj.get("totals").and_then(Value::as_object);
    let mut totals = BudgetUsageTotals::default();
    if let Some(totals_obj) = totals_obj {
        totals.tokens_used = totals_obj
            .get("tokens_used")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        totals.cost_used_usd = totals_obj
            .get("cost_used_usd")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .max(0.0);
        totals.tasks_completed = totals_obj
            .get("tasks_completed")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        totals.updated_at_ms = totals_obj
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
    }

    Some(normalize_entry(BudgetLedgerEntry {
        scope,
        policy,
        totals,
        extension_count: obj
            .get("extension_count")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    }))
}

fn normalize_store(mut store: BudgetLedgerStore, warnings: &mut Vec<String>) -> BudgetLedgerStore {
    store.schema_version = BUDGET_GUARDRAIL_SCHEMA_VERSION;
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::with_capacity(store.entries.len());
    for entry in store.entries.drain(..).map(normalize_entry) {
        let key = entry.scope.key();
        if seen.contains(&key) {
            warnings.push(format!("duplicate budget scope ignored ({key})"));
            continue;
        }
        seen.insert(key);
        deduped.push(entry);
    }
    deduped.sort_by_key(|entry| entry.scope.key());
    store.entries = deduped;
    store
}

fn normalize_entry(mut entry: BudgetLedgerEntry) -> BudgetLedgerEntry {
    entry.scope = match entry.scope.kind {
        BudgetScopeKind::Fleet => BudgetScope::fleet_scope(),
        _ => BudgetScope {
            kind: entry.scope.kind,
            id: normalize_scope_id(&entry.scope.id),
        },
    };
    entry.policy.thresholds = normalize_thresholds(entry.policy.thresholds);
    entry.policy.token_budget = entry.policy.token_budget.filter(|value| *value > 0);
    entry.policy.cost_budget_usd = entry.policy.cost_budget_usd.filter(|value| *value > 0.0);
    entry.totals.cost_used_usd = entry.totals.cost_used_usd.max(0.0);
    entry
}

fn effective_totals(entry: &BudgetLedgerEntry, samples: &[BudgetUsageSample]) -> BudgetUsageTotals {
    let mut latest = entry.totals;
    for sample in samples {
        if sample.timestamp_ms >= latest.updated_at_ms {
            latest.tokens_used = sample.total_tokens_used;
            latest.cost_used_usd = sample.total_cost_used_usd.max(0.0);
            latest.tasks_completed = sample.total_tasks_completed;
            latest.updated_at_ms = sample.timestamp_ms;
        }
    }
    latest
}

fn project_budget_exhaustion(
    entry: &BudgetLedgerEntry,
    totals: &BudgetUsageTotals,
    samples: &[BudgetUsageSample],
) -> BudgetProjection {
    let (token_rate, cost_rate) = burn_rates(samples);
    BudgetProjection {
        tokens_burn_rate_per_hour: token_rate,
        cost_burn_rate_usd_per_hour: cost_rate,
        token_exhaustion_seconds: projected_exhaustion_seconds(
            entry.policy.token_budget.map(|value| value as f64),
            totals.tokens_used as f64,
            token_rate,
        ),
        cost_exhaustion_seconds: projected_exhaustion_seconds(
            entry.policy.cost_budget_usd,
            totals.cost_used_usd,
            cost_rate,
        ),
    }
}

fn burn_rates(samples: &[BudgetUsageSample]) -> (f64, f64) {
    if samples.len() < 2 {
        return (0.0, 0.0);
    }
    let mut ordered = samples.to_vec();
    ordered.sort_by_key(|sample| sample.timestamp_ms);
    let first = ordered.first().copied().unwrap_or(BudgetUsageSample {
        timestamp_ms: 0,
        total_tokens_used: 0,
        total_cost_used_usd: 0.0,
        total_tasks_completed: 0,
    });
    let last = ordered.last().copied().unwrap_or(first);
    let elapsed_ms = last.timestamp_ms.saturating_sub(first.timestamp_ms);
    if elapsed_ms <= 0 {
        return (0.0, 0.0);
    }

    let elapsed_hours = elapsed_ms as f64 / 3_600_000.0;
    if elapsed_hours <= 0.0 {
        return (0.0, 0.0);
    }

    let token_delta = last
        .total_tokens_used
        .saturating_sub(first.total_tokens_used) as f64;
    let cost_delta = (last.total_cost_used_usd - first.total_cost_used_usd).max(0.0);
    (token_delta / elapsed_hours, cost_delta / elapsed_hours)
}

fn projected_exhaustion_seconds(
    budget_limit: Option<f64>,
    used: f64,
    burn_rate_per_hour: f64,
) -> Option<u64> {
    let limit = budget_limit?;
    if used >= limit {
        return Some(0);
    }
    if burn_rate_per_hour <= 0.0 {
        return None;
    }
    let remaining = (limit - used).max(0.0);
    let hours = remaining / burn_rate_per_hour;
    if hours.is_finite() && hours >= 0.0 {
        Some((hours * 3600.0).ceil() as u64)
    } else {
        None
    }
}

fn ratio_state(ratio: Option<f64>, thresholds: BudgetThresholds) -> BudgetState {
    let Some(ratio) = ratio else {
        return BudgetState::Healthy;
    };
    let ratio = ratio.max(0.0);
    if ratio >= thresholds.hard_kill_ratio {
        BudgetState::HardKill
    } else if ratio >= thresholds.pause_ratio {
        BudgetState::Pause
    } else if ratio >= thresholds.warn_ratio {
        BudgetState::Warn
    } else {
        BudgetState::Healthy
    }
}

fn action_for_state(state: BudgetState, policy: &BudgetPolicy) -> GuardrailAction {
    match state {
        BudgetState::HardKill if policy.auto_kill => GuardrailAction::HardKill,
        BudgetState::HardKill | BudgetState::Pause if policy.auto_pause => GuardrailAction::Pause,
        BudgetState::Warn => GuardrailAction::Warn,
        _ => GuardrailAction::None,
    }
}

fn proportional_token_extension(current: u64, ratio: f64, min_add: u64) -> u64 {
    let ratio_add = (current as f64 * ratio).ceil() as u64;
    ratio_add.max(min_add)
}

fn proportional_cost_extension(current: f64, ratio: f64, min_add: f64) -> f64 {
    let ratio_add = current * ratio;
    ratio_add.max(min_add).max(0.0)
}

fn normalize_extension_policy(mut policy: BudgetExtensionPolicy) -> BudgetExtensionPolicy {
    policy.token_extension_ratio = policy.token_extension_ratio.max(0.0);
    policy.cost_extension_ratio = policy.cost_extension_ratio.max(0.0);
    policy.min_cost_extension_usd = policy.min_cost_extension_usd.max(0.0);
    policy
}

fn normalize_thresholds(mut thresholds: BudgetThresholds) -> BudgetThresholds {
    thresholds.warn_ratio = thresholds.warn_ratio.clamp(0.0, 1.0);
    thresholds.pause_ratio = thresholds.pause_ratio.max(thresholds.warn_ratio).min(1.0);
    thresholds.hard_kill_ratio = thresholds
        .hard_kill_ratio
        .max(thresholds.pause_ratio)
        .min(5.0);
    thresholds
}

fn parse_scope_kind(raw: &str) -> Option<BudgetScopeKind> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "loop" => Some(BudgetScopeKind::Loop),
        "cluster" => Some(BudgetScopeKind::Cluster),
        "fleet" => Some(BudgetScopeKind::Fleet),
        _ => None,
    }
}

fn scope_kind_key(kind: BudgetScopeKind) -> &'static str {
    match kind {
        BudgetScopeKind::Loop => "loop",
        BudgetScopeKind::Cluster => "cluster",
        BudgetScopeKind::Fleet => "fleet",
    }
}

fn normalize_scope_id(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn state_label(state: BudgetState) -> &'static str {
    match state {
        BudgetState::Healthy => "healthy",
        BudgetState::Warn => "warn",
        BudgetState::Pause => "pause",
        BudgetState::HardKill => "hard-kill",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_one_key_budget_extension, evaluate_budget_guardrails, persist_budget_ledger,
        record_budget_usage, restore_budget_ledger, BudgetExtensionPolicy, BudgetLedgerEntry,
        BudgetLedgerStore, BudgetPolicy, BudgetScope, BudgetState, BudgetUsageSample,
        BudgetUsageTotals, GuardrailAction,
    };

    fn sample_entry() -> BudgetLedgerEntry {
        BudgetLedgerEntry {
            scope: BudgetScope::loop_scope("loop-a"),
            policy: BudgetPolicy {
                token_budget: Some(1_000),
                cost_budget_usd: Some(100.0),
                ..BudgetPolicy::default()
            },
            totals: BudgetUsageTotals {
                tokens_used: 850,
                cost_used_usd: 60.0,
                tasks_completed: 12,
                updated_at_ms: 3_600_000,
            },
            extension_count: 0,
        }
    }

    #[test]
    fn transitions_warn_pause_hard_kill_by_thresholds() {
        let mut entry = sample_entry();
        let warn = evaluate_budget_guardrails(&entry, &[]);
        assert_eq!(warn.state, BudgetState::Warn);
        assert_eq!(warn.action, GuardrailAction::Warn);

        entry.totals.tokens_used = 960;
        let pause = evaluate_budget_guardrails(&entry, &[]);
        assert_eq!(pause.state, BudgetState::Pause);
        assert_eq!(pause.action, GuardrailAction::Pause);

        entry.totals.tokens_used = 1_050;
        let hard_kill = evaluate_budget_guardrails(&entry, &[]);
        assert_eq!(hard_kill.state, BudgetState::HardKill);
        assert_eq!(hard_kill.action, GuardrailAction::HardKill);
    }

    #[test]
    fn calculates_projection_and_efficiency_metrics() {
        let entry = BudgetLedgerEntry {
            totals: BudgetUsageTotals {
                tokens_used: 400,
                cost_used_usd: 40.0,
                tasks_completed: 10,
                updated_at_ms: 3_600_000,
            },
            ..sample_entry()
        };
        let samples = vec![
            BudgetUsageSample {
                timestamp_ms: 0,
                total_tokens_used: 200,
                total_cost_used_usd: 20.0,
                total_tasks_completed: 5,
            },
            BudgetUsageSample {
                timestamp_ms: 3_600_000,
                total_tokens_used: 400,
                total_cost_used_usd: 40.0,
                total_tasks_completed: 10,
            },
        ];

        let decision = evaluate_budget_guardrails(&entry, &samples);
        assert!((decision.projection.tokens_burn_rate_per_hour - 200.0).abs() < 1e-6);
        assert!((decision.projection.cost_burn_rate_usd_per_hour - 20.0).abs() < 1e-6);
        assert_eq!(decision.projection.token_exhaustion_seconds, Some(10_800));
        assert_eq!(decision.projection.cost_exhaustion_seconds, Some(10_800));
        assert!((decision.efficiency.cost_per_task_usd - 4.0).abs() < 1e-6);
        assert!((decision.efficiency.tokens_per_task - 40.0).abs() < 1e-6);
    }

    #[test]
    fn one_key_extension_increases_budgets() {
        let mut entry = sample_entry();
        let outcome = apply_one_key_budget_extension(&mut entry, &BudgetExtensionPolicy::default());
        assert_eq!(outcome.token_added, 200);
        assert!((outcome.cost_added_usd - 20.0).abs() < 1e-6);
        assert_eq!(entry.policy.token_budget, Some(1_200));
        assert_eq!(entry.policy.cost_budget_usd, Some(120.0));
        assert_eq!(outcome.extension_count, 1);
    }

    #[test]
    fn one_key_extension_bootstraps_missing_budgets() {
        let mut entry = sample_entry();
        entry.policy.token_budget = None;
        entry.policy.cost_budget_usd = None;
        entry.totals.tokens_used = 420;
        entry.totals.cost_used_usd = 18.0;

        let outcome = apply_one_key_budget_extension(
            &mut entry,
            &BudgetExtensionPolicy {
                token_extension_ratio: 0.3,
                cost_extension_ratio: 0.2,
                min_token_extension: 80,
                min_cost_extension_usd: 4.0,
            },
        );
        assert_eq!(outcome.token_added, 80);
        assert!((outcome.cost_added_usd - 4.0).abs() < 1e-6);
        assert_eq!(entry.policy.token_budget, Some(500));
        assert_eq!(entry.policy.cost_budget_usd, Some(22.0));
    }

    #[test]
    fn persists_and_restores_ledger_state() {
        let mut entry = sample_entry();
        record_budget_usage(&mut entry, 50, 2.5, 2, 4_000_000);
        let store = BudgetLedgerStore {
            schema_version: super::BUDGET_GUARDRAIL_SCHEMA_VERSION,
            entries: vec![entry],
        };
        let json = persist_budget_ledger(&store);
        let restored = restore_budget_ledger(&json);

        assert!(!restored.migrated);
        assert!(restored.warnings.is_empty());
        assert_eq!(restored.store.entries.len(), 1);
        let restored_entry = &restored.store.entries[0];
        assert_eq!(restored_entry.scope.key(), "loop:loop-a");
        assert_eq!(restored_entry.totals.tokens_used, 900);
        assert!((restored_entry.totals.cost_used_usd - 62.5).abs() < 1e-6);
    }

    #[test]
    fn restore_invalid_json_falls_back_to_defaults() {
        let restored = restore_budget_ledger("{oops");
        assert_eq!(restored.store.entries.len(), 0);
        assert_eq!(restored.warnings.len(), 1);
    }

    #[test]
    fn restore_deduplicates_scopes() {
        let raw = r#"{
          "schema_version": 1,
          "entries": [
            {"scope_kind":"loop","scope_id":"loop-a","policy":{"token_budget":1000},"totals":{"tokens_used":100}},
            {"scope_kind":"loop","scope_id":"loop-a","policy":{"token_budget":1200},"totals":{"tokens_used":120}}
          ]
        }"#;
        let restored = restore_budget_ledger(raw);
        assert_eq!(restored.store.entries.len(), 1);
        assert_eq!(restored.warnings.len(), 1);
    }
}
