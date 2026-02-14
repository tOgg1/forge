//! Watchpoint engine core for conditional metric alerts.

use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum WatchValue {
    Number(f64),
    Text(String),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchComparator {
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Equal,
    Contains,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WatchCondition {
    pub metric: String,
    pub comparator: WatchComparator,
    pub threshold: WatchValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchConditionMode {
    All,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WatchpointDefinition {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub mode: WatchConditionMode,
    pub severity: WatchSeverity,
    pub cooldown_s: u64,
    pub conditions: Vec<WatchCondition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WatchpointRuntimeState {
    pub active: bool,
    pub last_triggered_epoch_s: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WatchSample {
    pub observed_at_epoch_s: i64,
    pub metrics: HashMap<String, WatchValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchpointTrigger {
    pub watchpoint_id: String,
    pub watchpoint_name: String,
    pub severity: WatchSeverity,
    pub message: String,
}

#[must_use]
pub fn evaluate_watchpoints(
    definitions: &[WatchpointDefinition],
    runtime_state: &mut HashMap<String, WatchpointRuntimeState>,
    sample: &WatchSample,
) -> Vec<WatchpointTrigger> {
    let mut triggers = Vec::new();

    for definition in definitions {
        let state = runtime_state.entry(definition.id.clone()).or_default();
        if !definition.enabled || definition.conditions.is_empty() {
            state.active = false;
            continue;
        }

        let mut matches = 0usize;
        for condition in &definition.conditions {
            let Some(observed) = sample.metrics.get(condition.metric.as_str()) else {
                continue;
            };
            if compare_watch_value(observed, &condition.threshold, condition.comparator) {
                matches += 1;
            }
        }

        let satisfied = match definition.mode {
            WatchConditionMode::All => matches == definition.conditions.len(),
            WatchConditionMode::Any => matches > 0,
        };

        state.active = satisfied;
        if !satisfied {
            continue;
        }

        let cooldown_ready = state
            .last_triggered_epoch_s
            .map(|last| {
                sample.observed_at_epoch_s.saturating_sub(last) >= definition.cooldown_s as i64
            })
            .unwrap_or(true);
        if !cooldown_ready {
            continue;
        }

        state.last_triggered_epoch_s = Some(sample.observed_at_epoch_s);
        triggers.push(WatchpointTrigger {
            watchpoint_id: definition.id.clone(),
            watchpoint_name: definition.name.clone(),
            severity: definition.severity,
            message: format!(
                "{} triggered (matched {}/{})",
                definition.name,
                matches,
                definition.conditions.len()
            ),
        });
    }

    triggers
}

fn compare_watch_value(left: &WatchValue, right: &WatchValue, comparator: WatchComparator) -> bool {
    match (left, right) {
        (WatchValue::Number(lhs), WatchValue::Number(rhs)) => match comparator {
            WatchComparator::GreaterThan => lhs > rhs,
            WatchComparator::GreaterThanOrEqual => lhs >= rhs,
            WatchComparator::LessThan => lhs < rhs,
            WatchComparator::LessThanOrEqual => lhs <= rhs,
            WatchComparator::Equal => (lhs - rhs).abs() < f64::EPSILON,
            WatchComparator::Contains => false,
        },
        (WatchValue::Text(lhs), WatchValue::Text(rhs)) => match comparator {
            WatchComparator::Contains => {
                lhs.to_ascii_lowercase().contains(&rhs.to_ascii_lowercase())
            }
            WatchComparator::Equal => lhs.eq_ignore_ascii_case(rhs),
            _ => false,
        },
        (WatchValue::Bool(lhs), WatchValue::Bool(rhs)) => match comparator {
            WatchComparator::Equal => lhs == rhs,
            _ => false,
        },
        _ => false,
    }
}

#[must_use]
pub fn persist_watchpoints(definitions: &[WatchpointDefinition]) -> String {
    let array = definitions
        .iter()
        .map(watchpoint_to_value)
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&Value::Array(array)).unwrap_or_else(|_| "[]".to_owned())
}

pub fn restore_watchpoints(raw: &str) -> Result<Vec<WatchpointDefinition>, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|err| format!("decode watchpoints: {err}"))?;
    let Some(entries) = value.as_array() else {
        return Err("decode watchpoints: root is not an array".to_owned());
    };
    entries
        .iter()
        .map(watchpoint_from_value)
        .collect::<Result<Vec<_>, _>>()
}

fn watchpoint_to_value(definition: &WatchpointDefinition) -> Value {
    let mut obj = Map::new();
    obj.insert("id".to_owned(), Value::String(definition.id.clone()));
    obj.insert("name".to_owned(), Value::String(definition.name.clone()));
    obj.insert("enabled".to_owned(), Value::Bool(definition.enabled));
    obj.insert(
        "mode".to_owned(),
        Value::String(
            match definition.mode {
                WatchConditionMode::All => "all",
                WatchConditionMode::Any => "any",
            }
            .to_owned(),
        ),
    );
    obj.insert(
        "severity".to_owned(),
        Value::String(
            match definition.severity {
                WatchSeverity::Info => "info",
                WatchSeverity::Warning => "warning",
                WatchSeverity::Critical => "critical",
            }
            .to_owned(),
        ),
    );
    obj.insert(
        "cooldown_s".to_owned(),
        Value::Number(serde_json::Number::from(definition.cooldown_s)),
    );
    obj.insert(
        "conditions".to_owned(),
        Value::Array(
            definition
                .conditions
                .iter()
                .map(condition_to_value)
                .collect(),
        ),
    );
    Value::Object(obj)
}

fn condition_to_value(condition: &WatchCondition) -> Value {
    let mut obj = Map::new();
    obj.insert("metric".to_owned(), Value::String(condition.metric.clone()));
    obj.insert(
        "comparator".to_owned(),
        Value::String(
            match condition.comparator {
                WatchComparator::GreaterThan => "gt",
                WatchComparator::GreaterThanOrEqual => "gte",
                WatchComparator::LessThan => "lt",
                WatchComparator::LessThanOrEqual => "lte",
                WatchComparator::Equal => "eq",
                WatchComparator::Contains => "contains",
            }
            .to_owned(),
        ),
    );
    obj.insert(
        "threshold".to_owned(),
        watch_value_to_value(&condition.threshold),
    );
    Value::Object(obj)
}

fn watch_value_to_value(value: &WatchValue) -> Value {
    let mut obj = Map::new();
    match value {
        WatchValue::Number(number) => {
            let number = serde_json::Number::from_f64(*number)
                .unwrap_or_else(|| serde_json::Number::from(0));
            obj.insert("kind".to_owned(), Value::String("number".to_owned()));
            obj.insert("value".to_owned(), Value::Number(number));
        }
        WatchValue::Text(text) => {
            obj.insert("kind".to_owned(), Value::String("text".to_owned()));
            obj.insert("value".to_owned(), Value::String(text.clone()));
        }
        WatchValue::Bool(boolean) => {
            obj.insert("kind".to_owned(), Value::String("bool".to_owned()));
            obj.insert("value".to_owned(), Value::Bool(*boolean));
        }
    }
    Value::Object(obj)
}

fn watchpoint_from_value(value: &Value) -> Result<WatchpointDefinition, String> {
    let Some(obj) = value.as_object() else {
        return Err("watchpoint entry is not an object".to_owned());
    };
    let id = required_str(obj, "id")?.to_owned();
    let name = required_str(obj, "name")?.to_owned();
    let enabled = obj.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let mode = match obj
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("all")
        .to_ascii_lowercase()
        .as_str()
    {
        "all" => WatchConditionMode::All,
        "any" => WatchConditionMode::Any,
        other => return Err(format!("invalid watchpoint mode: {other}")),
    };
    let severity = match obj
        .get("severity")
        .and_then(Value::as_str)
        .unwrap_or("warning")
        .to_ascii_lowercase()
        .as_str()
    {
        "info" => WatchSeverity::Info,
        "warning" => WatchSeverity::Warning,
        "critical" => WatchSeverity::Critical,
        other => return Err(format!("invalid watchpoint severity: {other}")),
    };
    let cooldown_s = obj.get("cooldown_s").and_then(Value::as_u64).unwrap_or(0);
    let conditions = if let Some(entries) = obj.get("conditions").and_then(Value::as_array) {
        entries
            .iter()
            .map(condition_from_value)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    Ok(WatchpointDefinition {
        id,
        name,
        enabled,
        mode,
        severity,
        cooldown_s,
        conditions,
    })
}

fn condition_from_value(value: &Value) -> Result<WatchCondition, String> {
    let Some(obj) = value.as_object() else {
        return Err("watch condition is not an object".to_owned());
    };
    let metric = required_str(obj, "metric")?.to_owned();
    let comparator = match required_str(obj, "comparator")? {
        "gt" => WatchComparator::GreaterThan,
        "gte" => WatchComparator::GreaterThanOrEqual,
        "lt" => WatchComparator::LessThan,
        "lte" => WatchComparator::LessThanOrEqual,
        "eq" => WatchComparator::Equal,
        "contains" => WatchComparator::Contains,
        other => return Err(format!("invalid comparator: {other}")),
    };
    let threshold = watch_value_from_value(
        obj.get("threshold")
            .ok_or_else(|| "missing watch threshold".to_owned())?,
    )?;
    Ok(WatchCondition {
        metric,
        comparator,
        threshold,
    })
}

fn watch_value_from_value(value: &Value) -> Result<WatchValue, String> {
    let Some(obj) = value.as_object() else {
        return Err("watch threshold is not an object".to_owned());
    };
    let kind = required_str(obj, "kind")?;
    let raw = obj
        .get("value")
        .ok_or_else(|| "missing watch threshold value".to_owned())?;
    match kind {
        "number" => raw
            .as_f64()
            .map(WatchValue::Number)
            .ok_or_else(|| "number threshold is not numeric".to_owned()),
        "text" => raw
            .as_str()
            .map(|text| WatchValue::Text(text.to_owned()))
            .ok_or_else(|| "text threshold is not a string".to_owned()),
        "bool" => raw
            .as_bool()
            .map(WatchValue::Bool)
            .ok_or_else(|| "bool threshold is not boolean".to_owned()),
        other => Err(format!("invalid watch threshold kind: {other}")),
    }
}

fn required_str<'a>(obj: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    obj.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or invalid field: {key}"))
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_watchpoints, persist_watchpoints, restore_watchpoints, WatchComparator,
        WatchCondition, WatchConditionMode, WatchSample, WatchSeverity, WatchValue,
        WatchpointDefinition,
    };
    use std::collections::HashMap;

    fn base_definition() -> WatchpointDefinition {
        WatchpointDefinition {
            id: "wp-error-rate".to_owned(),
            name: "Error rate > 3/min".to_owned(),
            enabled: true,
            mode: WatchConditionMode::All,
            severity: WatchSeverity::Warning,
            cooldown_s: 60,
            conditions: vec![WatchCondition {
                metric: "loop.error_rate_per_min".to_owned(),
                comparator: WatchComparator::GreaterThan,
                threshold: WatchValue::Number(3.0),
            }],
        }
    }

    #[test]
    fn all_mode_numeric_threshold_triggers() {
        let mut state = HashMap::new();
        let definition = base_definition();
        let mut metrics = HashMap::new();
        metrics.insert(
            "loop.error_rate_per_min".to_owned(),
            WatchValue::Number(4.5),
        );
        let sample = WatchSample {
            observed_at_epoch_s: 100,
            metrics,
        };

        let triggers = evaluate_watchpoints(&[definition], &mut state, &sample);
        assert_eq!(triggers.len(), 1);
        assert!(triggers[0].message.contains("triggered"));
    }

    #[test]
    fn any_mode_text_contains_triggers() {
        let mut definition = base_definition();
        definition.id = "wp-deadlock".to_owned();
        definition.name = "Deadlock mention".to_owned();
        definition.mode = WatchConditionMode::Any;
        definition.conditions = vec![
            WatchCondition {
                metric: "loop.latest_log".to_owned(),
                comparator: WatchComparator::Contains,
                threshold: WatchValue::Text("deadlock".to_owned()),
            },
            WatchCondition {
                metric: "fleet.tokens_per_hour".to_owned(),
                comparator: WatchComparator::GreaterThan,
                threshold: WatchValue::Number(50.0),
            },
        ];

        let mut state = HashMap::new();
        let mut metrics = HashMap::new();
        metrics.insert(
            "loop.latest_log".to_owned(),
            WatchValue::Text("possible DEADLOCK while waiting".to_owned()),
        );
        let sample = WatchSample {
            observed_at_epoch_s: 200,
            metrics,
        };
        let triggers = evaluate_watchpoints(&[definition], &mut state, &sample);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].watchpoint_id, "wp-deadlock");
    }

    #[test]
    fn cooldown_prevents_retrigger_until_window_passes() {
        let definition = base_definition();
        let mut state = HashMap::new();
        let mut metrics = HashMap::new();
        metrics.insert(
            "loop.error_rate_per_min".to_owned(),
            WatchValue::Number(4.1),
        );

        let first = evaluate_watchpoints(
            std::slice::from_ref(&definition),
            &mut state,
            &WatchSample {
                observed_at_epoch_s: 10,
                metrics: metrics.clone(),
            },
        );
        assert_eq!(first.len(), 1);

        let second = evaluate_watchpoints(
            std::slice::from_ref(&definition),
            &mut state,
            &WatchSample {
                observed_at_epoch_s: 40,
                metrics: metrics.clone(),
            },
        );
        assert!(second.is_empty());

        let third = evaluate_watchpoints(
            &[definition],
            &mut state,
            &WatchSample {
                observed_at_epoch_s: 71,
                metrics,
            },
        );
        assert_eq!(third.len(), 1);
    }

    #[test]
    fn persist_and_restore_round_trip() {
        let definitions = vec![base_definition()];
        let json = persist_watchpoints(&definitions);
        let restored = match restore_watchpoints(&json) {
            Ok(restored) => restored,
            Err(err) => panic!("restore should succeed: {err}"),
        };
        assert_eq!(restored, definitions);
    }
}
