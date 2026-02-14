use std::collections::BTreeMap;
#[cfg(test)]
use std::env;
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterKind {
    Linear,
    Slack,
}

impl AdapterKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Slack => "slack",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalTask {
    pub external_id: String,
    pub task_type: String,
    pub title: String,
    pub body: String,
    pub repo: String,
    pub priority: i64,
    pub tags: Vec<String>,
}

pub trait ExternalAdapter {
    fn kind(&self) -> AdapterKind;
    fn enabled(&self) -> bool;
    fn poll_tasks(&self) -> Result<Vec<ExternalTask>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdapterRuntimeConfig {
    pub linear_enabled: bool,
    pub slack_enabled: bool,
}

impl AdapterRuntimeConfig {
    #[must_use]
    pub fn from_map(values: &BTreeMap<String, String>) -> Self {
        Self {
            linear_enabled: parse_bool(values.get("linear_enabled")),
            slack_enabled: parse_bool(values.get("slack_enabled")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearStubAdapter {
    pub enabled: bool,
    pub tasks: Vec<ExternalTask>,
}

impl ExternalAdapter for LinearStubAdapter {
    fn kind(&self) -> AdapterKind {
        AdapterKind::Linear
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn poll_tasks(&self) -> Result<Vec<ExternalTask>, String> {
        Ok(self.tasks.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackStubAdapter {
    pub enabled: bool,
    pub tasks: Vec<ExternalTask>,
}

impl ExternalAdapter for SlackStubAdapter {
    fn kind(&self) -> AdapterKind {
        AdapterKind::Slack
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn poll_tasks(&self) -> Result<Vec<ExternalTask>, String> {
        Ok(self.tasks.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterIngestResult {
    pub adapter: String,
    pub enabled: bool,
    pub polled: usize,
    pub ingested: usize,
    pub skipped: usize,
}

pub fn build_stub_adapters(
    config: &AdapterRuntimeConfig,
    linear_tasks: Vec<ExternalTask>,
    slack_tasks: Vec<ExternalTask>,
) -> Vec<Box<dyn ExternalAdapter>> {
    vec![
        Box::new(LinearStubAdapter {
            enabled: config.linear_enabled,
            tasks: linear_tasks,
        }),
        Box::new(SlackStubAdapter {
            enabled: config.slack_enabled,
            tasks: slack_tasks,
        }),
    ]
}

pub fn ingest_adapter_tasks(
    adapter: &dyn ExternalAdapter,
    db: &forge_db::Db,
    team_reference: &str,
) -> Result<AdapterIngestResult, String> {
    let adapter_name = adapter.kind().as_str().to_string();
    if !adapter.enabled() {
        return Ok(AdapterIngestResult {
            adapter: adapter_name,
            enabled: false,
            polled: 0,
            ingested: 0,
            skipped: 0,
        });
    }

    let service = forge_db::team_repository::TeamService::new(db);
    let team = service
        .show_team(team_reference.trim())
        .map_err(|err| format!("resolve team {team_reference:?}: {err}"))?;
    let tasks = adapter.poll_tasks()?;

    let repo = forge_db::team_task_repository::TeamTaskRepository::new(db);
    let mut ingested = 0usize;
    let mut skipped = 0usize;
    for task in &tasks {
        if task.task_type.trim().is_empty() || task.title.trim().is_empty() {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let payload = serde_json::json!({
            "type": task.task_type.trim(),
            "title": task.title.trim(),
            "body": task.body.trim(),
            "repo": task.repo.trim(),
            "tags": task.tags,
            "external_id": task.external_id.trim(),
            "adapter": adapter_name,
        });
        let mut team_task = forge_db::team_task_repository::TeamTask {
            id: String::new(),
            team_id: team.id.clone(),
            payload_json: serde_json::to_string(&payload)
                .map_err(|err| format!("serialize adapter payload: {err}"))?,
            status: forge_db::team_task_repository::TeamTaskStatus::Queued
                .as_str()
                .to_string(),
            priority: task.priority.max(0),
            assigned_agent_id: String::new(),
            submitted_at: String::new(),
            assigned_at: None,
            started_at: None,
            finished_at: None,
            updated_at: String::new(),
        };
        repo.submit(&mut team_task)
            .map_err(|err| format!("submit team task: {err}"))?;
        ingested = ingested.saturating_add(1);
    }

    Ok(AdapterIngestResult {
        adapter: adapter_name,
        enabled: true,
        polled: tasks.len(),
        ingested,
        skipped,
    })
}

pub fn ingest_enabled_adapters(
    adapters: &[Box<dyn ExternalAdapter>],
    db: &forge_db::Db,
    team_reference: &str,
) -> Result<Vec<AdapterIngestResult>, String> {
    let mut results = Vec::new();
    for adapter in adapters {
        results.push(ingest_adapter_tasks(adapter.as_ref(), db, team_reference)?);
    }
    Ok(results)
}

fn parse_bool(value: Option<&String>) -> bool {
    value.is_some_and(|raw| {
        let normalized = raw.trim().to_ascii_lowercase();
        matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
    })
}

#[cfg(test)]
fn temp_db_path(tag: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    env::temp_dir().join(format!("forge-adapter-test-{tag}-{nanos}.db"))
}

#[cfg(test)]
mod tests {
    use super::{
        build_stub_adapters, ingest_adapter_tasks, ingest_enabled_adapters, AdapterRuntimeConfig,
        ExternalTask, LinearStubAdapter,
    };
    use std::collections::BTreeMap;

    fn seed_team(db: &forge_db::Db, name: &str) -> forge_db::team_repository::Team {
        match forge_db::team_repository::TeamService::new(db).create_team(name, "", "", 60) {
            Ok(team) => team,
            Err(err) => panic!("create team: {err}"),
        }
    }

    fn open_db(path: &std::path::Path) -> forge_db::Db {
        let mut db = match forge_db::Db::open(forge_db::Config::new(path)) {
            Ok(db) => db,
            Err(err) => panic!("open db: {err}"),
        };
        if let Err(err) = db.migrate_up() {
            panic!("migrate up: {err}");
        }
        db
    }

    fn sample_external_task(id: &str) -> ExternalTask {
        ExternalTask {
            external_id: id.to_string(),
            task_type: "incident".to_string(),
            title: format!("Investigate {id}"),
            body: "adapter body".to_string(),
            repo: "forge".to_string(),
            priority: 20,
            tags: vec!["adapter".to_string()],
        }
    }

    #[test]
    fn disabled_adapter_does_not_emit_tasks() {
        let db_path = super::temp_db_path("disabled");
        let db = open_db(&db_path);
        let team = seed_team(&db, "ops");

        let adapter = LinearStubAdapter {
            enabled: false,
            tasks: vec![sample_external_task("lin-1")],
        };
        let result = match ingest_adapter_tasks(&adapter, &db, "ops") {
            Ok(result) => result,
            Err(err) => panic!("ingest: {err}"),
        };
        assert!(!result.enabled);
        assert_eq!(result.ingested, 0);

        let repo = forge_db::team_task_repository::TeamTaskRepository::new(&db);
        let tasks = repo
            .list(&forge_db::team_task_repository::TeamTaskFilter {
                team_id: team.id,
                ..forge_db::team_task_repository::TeamTaskFilter::default()
            })
            .unwrap_or_else(|err| panic!("list team tasks: {err}"));
        assert!(tasks.is_empty());
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn linear_stub_can_emit_task_into_team_inbox() {
        let db_path = super::temp_db_path("linear-ingest");
        let db = open_db(&db_path);
        let team = seed_team(&db, "ops");

        let adapter = LinearStubAdapter {
            enabled: true,
            tasks: vec![sample_external_task("lin-2")],
        };
        let result = match ingest_adapter_tasks(&adapter, &db, &team.id) {
            Ok(result) => result,
            Err(err) => panic!("ingest: {err}"),
        };
        assert!(result.enabled);
        assert_eq!(result.polled, 1);
        assert_eq!(result.ingested, 1);

        let repo = forge_db::team_task_repository::TeamTaskRepository::new(&db);
        let tasks = repo
            .list(&forge_db::team_task_repository::TeamTaskFilter {
                team_id: team.id.clone(),
                ..forge_db::team_task_repository::TeamTaskFilter::default()
            })
            .unwrap_or_else(|err| panic!("list team tasks: {err}"));
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].payload_json.contains("\"adapter\":\"linear\""));
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn config_wiring_controls_enabled_adapters() {
        let mut values = BTreeMap::new();
        values.insert("linear_enabled".to_string(), "true".to_string());
        values.insert("slack_enabled".to_string(), "false".to_string());
        let config = AdapterRuntimeConfig::from_map(&values);
        let adapters = build_stub_adapters(
            &config,
            vec![sample_external_task("lin-3")],
            vec![sample_external_task("slack-1")],
        );
        assert_eq!(adapters.len(), 2);
        assert!(adapters[0].enabled());
        assert!(!adapters[1].enabled());
    }

    #[test]
    fn ingest_enabled_adapters_collects_per_adapter_results() {
        let db_path = super::temp_db_path("multi-ingest");
        let db = open_db(&db_path);
        let team = seed_team(&db, "ops");

        let config = AdapterRuntimeConfig {
            linear_enabled: true,
            slack_enabled: true,
        };
        let adapters = build_stub_adapters(
            &config,
            vec![sample_external_task("lin-4")],
            vec![sample_external_task("slack-4")],
        );
        let results = match ingest_enabled_adapters(&adapters, &db, &team.id) {
            Ok(results) => results,
            Err(err) => panic!("ingest adapters: {err}"),
        };
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].ingested, 1);
        assert_eq!(results[1].ingested, 1);
        let _ = std::fs::remove_file(db_path);
    }
}
