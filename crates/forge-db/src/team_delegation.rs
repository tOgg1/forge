//! Delegation rules engine for team task routing.

use crate::DbError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DelegationTaskPayload {
    pub payload_type: String,
    pub title: String,
    pub repo: String,
    pub tags: Vec<String>,
    pub priority: i64,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DelegationTarget {
    pub agent_id: String,
    pub prompt_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DelegationMatchSpec {
    pub payload_type: String,
    pub tags_any: Vec<String>,
    pub repo_prefix: String,
    pub priority_min: Option<i64>,
    pub path_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DelegationRule {
    pub id: String,
    pub match_spec: DelegationMatchSpec,
    pub target: DelegationTarget,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DelegationRuleSet {
    pub default_agent_id: String,
    pub default_prompt_name: String,
    pub rules: Vec<DelegationRule>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DelegationDecision {
    pub matched: bool,
    pub matched_rule_id: String,
    pub agent_id: String,
    pub prompt_name: String,
    pub explain_lines: Vec<String>,
}

pub fn parse_delegation_rule_set(raw: &str) -> Result<DelegationRuleSet, DbError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(DelegationRuleSet::default());
    }
    let value = serde_json::from_str::<serde_json::Value>(trimmed)
        .map_err(|err| DbError::Validation(format!("invalid delegation rules json: {err}")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| DbError::Validation("delegation rules must be a JSON object".to_owned()))?;

    let default_agent_id = obj
        .get("default_agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let default_prompt_name = obj
        .get("default_prompt_name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_owned();

    let mut rules = Vec::new();
    if let Some(raw_rules) = obj.get("rules") {
        let Some(raw_rules) = raw_rules.as_array() else {
            return Err(DbError::Validation("\"rules\" must be an array".to_owned()));
        };
        for (index, item) in raw_rules.iter().enumerate() {
            let Some(rule_obj) = item.as_object() else {
                return Err(DbError::Validation(format!(
                    "rule at index {index} must be an object"
                )));
            };
            let id = rule_obj
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("rule-{}", index + 1));

            let match_spec = if let Some(raw_match) = rule_obj.get("match") {
                parse_match_spec(raw_match, &id)?
            } else {
                DelegationMatchSpec::default()
            };
            let target = parse_target(rule_obj.get("target"), &id)?;
            rules.push(DelegationRule {
                id,
                match_spec,
                target,
            });
        }
    }

    Ok(DelegationRuleSet {
        default_agent_id,
        default_prompt_name,
        rules,
    })
}

pub fn evaluate_delegation_rules(
    rule_set: &DelegationRuleSet,
    payload: &DelegationTaskPayload,
) -> DelegationDecision {
    let payload = normalize_payload(payload);
    let mut explain_lines = Vec::new();

    for rule in &rule_set.rules {
        let (matched, reasons) = rule_matches(rule, &payload);
        if matched {
            explain_lines.push(format!("rule {}: match", rule.id));
            return DelegationDecision {
                matched: true,
                matched_rule_id: rule.id.clone(),
                agent_id: rule.target.agent_id.clone(),
                prompt_name: rule.target.prompt_name.clone(),
                explain_lines,
            };
        }
        explain_lines.push(format!("rule {}: skip ({})", rule.id, reasons.join("; ")));
    }

    if !rule_set.default_agent_id.is_empty() || !rule_set.default_prompt_name.is_empty() {
        explain_lines.push("fallback: team default target".to_owned());
        return DelegationDecision {
            matched: false,
            matched_rule_id: String::new(),
            agent_id: rule_set.default_agent_id.clone(),
            prompt_name: rule_set.default_prompt_name.clone(),
            explain_lines,
        };
    }

    explain_lines.push("fallback: no target configured".to_owned());
    DelegationDecision {
        matched: false,
        matched_rule_id: String::new(),
        agent_id: String::new(),
        prompt_name: String::new(),
        explain_lines,
    }
}

pub fn resolve_delegation_for_team(
    team: &crate::team_repository::Team,
    payload: &DelegationTaskPayload,
) -> Result<DelegationDecision, DbError> {
    let rule_set = parse_delegation_rule_set(&team.delegation_rules_json)?;
    Ok(evaluate_delegation_rules(&rule_set, payload))
}

#[must_use]
pub fn render_delegation_explain_text(decision: &DelegationDecision) -> String {
    if decision.explain_lines.is_empty() {
        return "no delegation explain lines".to_owned();
    }
    let mut lines = Vec::new();
    if decision.matched {
        lines.push(format!(
            "decision: matched {} -> agent={} prompt={}",
            decision.matched_rule_id, decision.agent_id, decision.prompt_name
        ));
    } else if !decision.agent_id.is_empty() || !decision.prompt_name.is_empty() {
        lines.push(format!(
            "decision: fallback -> agent={} prompt={}",
            decision.agent_id, decision.prompt_name
        ));
    } else {
        lines.push("decision: no target".to_owned());
    }
    lines.extend(decision.explain_lines.iter().cloned());
    lines.join("\n")
}

fn parse_match_spec(
    raw_match: &serde_json::Value,
    rule_id: &str,
) -> Result<DelegationMatchSpec, DbError> {
    let Some(match_obj) = raw_match.as_object() else {
        return Err(DbError::Validation(format!(
            "rule {rule_id}: \"match\" must be an object"
        )));
    };
    let payload_type = match_obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let repo_prefix = match_obj
        .get("repo_prefix")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let priority_min = match_obj
        .get("priority_min")
        .and_then(|v| v.as_i64())
        .filter(|v| *v >= 0);
    let tags_any = parse_string_array(match_obj.get("tags_any"), rule_id, "tags_any")?
        .into_iter()
        .map(|tag| tag.to_ascii_lowercase())
        .collect();
    let path_prefixes =
        parse_string_array(match_obj.get("path_prefixes"), rule_id, "path_prefixes")?
            .into_iter()
            .map(|path| path.to_ascii_lowercase())
            .collect();

    Ok(DelegationMatchSpec {
        payload_type,
        tags_any,
        repo_prefix,
        priority_min,
        path_prefixes,
    })
}

fn parse_target(
    raw_target: Option<&serde_json::Value>,
    rule_id: &str,
) -> Result<DelegationTarget, DbError> {
    let Some(raw_target) = raw_target else {
        return Err(DbError::Validation(format!(
            "rule {rule_id}: missing target object"
        )));
    };
    let Some(target_obj) = raw_target.as_object() else {
        return Err(DbError::Validation(format!(
            "rule {rule_id}: target must be an object"
        )));
    };

    let agent_id = target_obj
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let prompt_name = target_obj
        .get("prompt_name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_owned();

    if agent_id.is_empty() && prompt_name.is_empty() {
        return Err(DbError::Validation(format!(
            "rule {rule_id}: target requires agent_id or prompt_name"
        )));
    }

    Ok(DelegationTarget {
        agent_id,
        prompt_name,
    })
}

fn parse_string_array(
    value: Option<&serde_json::Value>,
    rule_id: &str,
    field: &str,
) -> Result<Vec<String>, DbError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(DbError::Validation(format!(
            "rule {rule_id}: {field} must be an array"
        )));
    };
    let mut out = Vec::new();
    for item in items {
        let Some(str_value) = item.as_str() else {
            return Err(DbError::Validation(format!(
                "rule {rule_id}: {field} values must be strings"
            )));
        };
        let trimmed = str_value.trim();
        if !trimmed.is_empty() {
            out.push(trimmed.to_owned());
        }
    }
    Ok(out)
}

fn normalize_payload(payload: &DelegationTaskPayload) -> DelegationTaskPayload {
    DelegationTaskPayload {
        payload_type: payload.payload_type.trim().to_ascii_lowercase(),
        title: payload.title.trim().to_owned(),
        repo: payload.repo.trim().to_ascii_lowercase(),
        tags: payload
            .tags
            .iter()
            .map(|tag| tag.trim().to_ascii_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect(),
        priority: payload.priority.max(0),
        paths: payload
            .paths
            .iter()
            .map(|path| path.trim().to_ascii_lowercase())
            .filter(|path| !path.is_empty())
            .collect(),
    }
}

fn rule_matches(rule: &DelegationRule, payload: &DelegationTaskPayload) -> (bool, Vec<String>) {
    let mut reasons = Vec::new();
    if !rule.match_spec.payload_type.is_empty()
        && rule.match_spec.payload_type != payload.payload_type
    {
        reasons.push(format!(
            "type {} != {}",
            payload.payload_type, rule.match_spec.payload_type
        ));
    }

    if !rule.match_spec.tags_any.is_empty() {
        let has_tag = payload
            .tags
            .iter()
            .any(|tag| rule.match_spec.tags_any.contains(tag));
        if !has_tag {
            reasons.push(format!("tags missing any {:?}", rule.match_spec.tags_any));
        }
    }

    if !rule.match_spec.repo_prefix.is_empty()
        && !payload.repo.starts_with(&rule.match_spec.repo_prefix)
    {
        reasons.push(format!(
            "repo {} not prefixed {}",
            payload.repo, rule.match_spec.repo_prefix
        ));
    }

    if let Some(priority_min) = rule.match_spec.priority_min {
        if payload.priority < priority_min {
            reasons.push(format!("priority {} < {}", payload.priority, priority_min));
        }
    }

    if !rule.match_spec.path_prefixes.is_empty() {
        let has_path = payload.paths.iter().any(|path| {
            rule.match_spec
                .path_prefixes
                .iter()
                .any(|prefix| path.starts_with(prefix))
        });
        if !has_path {
            reasons.push(format!(
                "paths missing prefixes {:?}",
                rule.match_spec.path_prefixes
            ));
        }
    }

    (reasons.is_empty(), reasons)
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_delegation_rules, parse_delegation_rule_set, render_delegation_explain_text,
        resolve_delegation_for_team, DelegationTaskPayload,
    };
    use crate::team_repository::Team;

    fn payload() -> DelegationTaskPayload {
        DelegationTaskPayload {
            payload_type: "incident".to_owned(),
            title: "prod db lock".to_owned(),
            repo: "oss/forge".to_owned(),
            tags: vec!["urgent".to_owned(), "db".to_owned()],
            priority: 90,
            paths: vec!["crates/forge-db/src/lib.rs".to_owned()],
        }
    }

    #[test]
    fn matches_rule_on_type_tag_and_priority() {
        let rules = parse_delegation_rule_set(
            r#"{
                "default_agent_id":"agent-default",
                "rules":[
                    {
                        "id":"critical-incident",
                        "match":{"type":"incident","tags_any":["urgent"],"priority_min":80},
                        "target":{"agent_id":"agent-lead","prompt_name":"incident-lead"}
                    }
                ]
            }"#,
        )
        .unwrap_or_else(|err| panic!("parse rules: {err}"));
        let decision = evaluate_delegation_rules(&rules, &payload());
        assert!(decision.matched);
        assert_eq!(decision.matched_rule_id, "critical-incident");
        assert_eq!(decision.agent_id, "agent-lead");
        assert_eq!(decision.prompt_name, "incident-lead");
    }

    #[test]
    fn first_matching_rule_wins_deterministically() {
        let rules = parse_delegation_rule_set(
            r#"{
                "rules":[
                    {
                        "id":"first",
                        "match":{"type":"incident"},
                        "target":{"agent_id":"agent-a","prompt_name":"prompt-a"}
                    },
                    {
                        "id":"second",
                        "match":{"type":"incident"},
                        "target":{"agent_id":"agent-b","prompt_name":"prompt-b"}
                    }
                ]
            }"#,
        )
        .unwrap_or_else(|err| panic!("parse rules: {err}"));
        let decision = evaluate_delegation_rules(&rules, &payload());
        assert!(decision.matched);
        assert_eq!(decision.matched_rule_id, "first");
        assert_eq!(decision.agent_id, "agent-a");
    }

    #[test]
    fn falls_back_to_default_target_when_no_rule_matches() {
        let rules = parse_delegation_rule_set(
            r#"{
                "default_agent_id":"agent-default",
                "default_prompt_name":"default-plan",
                "rules":[
                    {
                        "id":"security-only",
                        "match":{"tags_any":["security"]},
                        "target":{"agent_id":"agent-sec"}
                    }
                ]
            }"#,
        )
        .unwrap_or_else(|err| panic!("parse rules: {err}"));
        let decision = evaluate_delegation_rules(&rules, &payload());
        assert!(!decision.matched);
        assert_eq!(decision.agent_id, "agent-default");
        assert_eq!(decision.prompt_name, "default-plan");
    }

    #[test]
    fn repo_and_path_prefix_matching_is_supported() {
        let rules = parse_delegation_rule_set(
            r#"{
                "rules":[
                    {
                        "id":"db-lane",
                        "match":{"repo_prefix":"oss/","path_prefixes":["crates/forge-db/"]},
                        "target":{"agent_id":"agent-db"}
                    }
                ]
            }"#,
        )
        .unwrap_or_else(|err| panic!("parse rules: {err}"));
        let decision = evaluate_delegation_rules(&rules, &payload());
        assert!(decision.matched);
        assert_eq!(decision.matched_rule_id, "db-lane");
        assert_eq!(decision.agent_id, "agent-db");
    }

    #[test]
    fn invalid_rule_json_returns_validation_error() {
        let invalid = parse_delegation_rule_set(
            r#"{
                "rules":[
                    {"id":"bad","match":{"type":"incident"}}
                ]
            }"#,
        );
        assert!(matches!(invalid, Err(crate::DbError::Validation(_))));
    }

    #[test]
    fn explain_text_is_deterministic() {
        let rules = parse_delegation_rule_set(
            r#"{
                "default_agent_id":"agent-default",
                "rules":[
                    {
                        "id":"high-prio",
                        "match":{"priority_min":95},
                        "target":{"agent_id":"agent-p0"}
                    },
                    {
                        "id":"incident",
                        "match":{"type":"incident"},
                        "target":{"agent_id":"agent-incident","prompt_name":"incident"}
                    }
                ]
            }"#,
        )
        .unwrap_or_else(|err| panic!("parse rules: {err}"));
        let decision = evaluate_delegation_rules(&rules, &payload());
        let text = render_delegation_explain_text(&decision);
        assert_eq!(
            text,
            "decision: matched incident -> agent=agent-incident prompt=incident\nrule high-prio: skip (priority 90 < 95)\nrule incident: match"
        );
    }

    #[test]
    fn resolves_from_team_config_json() {
        let team = Team {
            id: "team-1".to_owned(),
            name: "ops".to_owned(),
            delegation_rules_json: r#"{
                "rules":[
                    {
                        "id":"incident",
                        "match":{"type":"incident"},
                        "target":{"agent_id":"agent-incident"}
                    }
                ]
            }"#
            .to_owned(),
            ..Team::default()
        };
        let decision = resolve_delegation_for_team(&team, &payload())
            .unwrap_or_else(|err| panic!("resolve delegation for team: {err}"));
        assert!(decision.matched);
        assert_eq!(decision.agent_id, "agent-incident");
    }
}
