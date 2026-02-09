//! Validation and normalization for fmail names (agents, topics, tags).

pub const MAX_TAGS_PER_MESSAGE: usize = 10;
pub const MAX_TAG_LENGTH: usize = 50;

/// Check if a string matches `^[a-z0-9-]+$`.
fn is_valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Normalize and validate an agent name.
///
/// Go parity: lowercases, trims whitespace, checks `^[a-z0-9-]+$`.
pub fn normalize_agent_name(name: &str) -> Result<String, String> {
    let normalized = name.trim().to_lowercase();
    if !is_valid_name(&normalized) {
        return Err("invalid agent name".to_string());
    }
    Ok(normalized)
}

/// Normalize and validate a topic name.
///
/// Go parity: `NormalizeTopic` in `validate.go`.
pub fn normalize_topic(topic: &str) -> Result<String, String> {
    let normalized = topic.trim().to_lowercase();
    if !is_valid_name(&normalized) {
        return Err("invalid topic name".to_string());
    }
    Ok(normalized)
}

/// Normalize a target (topic or @agent) and return `(normalized, is_dm)`.
///
/// Go parity: `NormalizeTarget` in `validate.go`.
pub fn normalize_target(target: &str) -> Result<(String, bool), String> {
    let raw = target.trim();
    if raw.is_empty() {
        return Err("invalid target".to_string());
    }
    if let Some(agent_part) = raw.strip_prefix('@') {
        let agent = normalize_agent_name(agent_part)?;
        Ok((format!("@{agent}"), true))
    } else {
        let topic = normalize_topic(raw)?;
        Ok((topic, false))
    }
}

/// Validate a priority value.
///
/// Go parity: `ValidatePriority` in `message.go`.
pub fn validate_priority(value: &str) -> Result<(), String> {
    match value {
        "low" | "normal" | "high" => Ok(()),
        _ => Err(format!("invalid priority: {value}")),
    }
}

/// Validate a single tag.
///
/// Go parity: `ValidateTag` in `validate.go`.
pub fn validate_tag(tag: &str) -> Result<(), String> {
    if tag.is_empty() {
        return Err("invalid tag".to_string());
    }
    if tag.len() > MAX_TAG_LENGTH {
        return Err(format!("invalid tag: exceeds {MAX_TAG_LENGTH} chars"));
    }
    if !is_valid_name(tag) {
        return Err(format!("invalid tag: {tag}"));
    }
    Ok(())
}

/// Validate all tags for a message.
///
/// Go parity: `ValidateTags` in `validate.go`.
pub fn validate_tags(tags: &[String]) -> Result<(), String> {
    if tags.len() > MAX_TAGS_PER_MESSAGE {
        return Err(format!("invalid tag: max {MAX_TAGS_PER_MESSAGE} tags"));
    }
    for tag in tags {
        validate_tag(tag)?;
    }
    Ok(())
}

/// Normalize tags: lowercase, trim, validate, deduplicate.
///
/// Go parity: `NormalizeTags` in `validate.go`.
pub fn normalize_tags(tags: &[String]) -> Result<Vec<String>, String> {
    if tags.is_empty() {
        return Ok(vec![]);
    }
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(tags.len());
    for tag in tags {
        let normalized = tag.trim().to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        validate_tag(&normalized)?;
        if seen.insert(normalized.clone()) {
            result.push(normalized);
        }
    }
    if result.len() > MAX_TAGS_PER_MESSAGE {
        return Err(format!("invalid tag: max {MAX_TAGS_PER_MESSAGE} tags"));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_valid_names() {
        assert_eq!(normalize_agent_name("hello").as_deref(), Ok("hello"));
        assert_eq!(normalize_agent_name("Hello").as_deref(), Ok("hello"));
        assert_eq!(
            normalize_agent_name("  my-agent  ").as_deref(),
            Ok("my-agent")
        );
        assert_eq!(
            normalize_agent_name("agent-123").as_deref(),
            Ok("agent-123")
        );
    }

    #[test]
    fn normalize_rejects_invalid() {
        assert!(normalize_agent_name("").is_err());
        assert!(normalize_agent_name("   ").is_err());
        assert!(normalize_agent_name("has spaces").is_err());
        assert!(normalize_agent_name("HAS_UNDERSCORE").is_err());
        assert!(normalize_agent_name("special!char").is_err());
    }

    #[test]
    fn normalize_topic_valid() {
        assert_eq!(normalize_topic("task").as_deref(), Ok("task"));
        assert_eq!(normalize_topic("My-Topic").as_deref(), Ok("my-topic"));
    }

    #[test]
    fn normalize_topic_invalid() {
        assert!(normalize_topic("").is_err());
        assert!(normalize_topic("has spaces").is_err());
    }

    #[test]
    fn normalize_target_topic() {
        let (target, is_dm) = normalize_target("task").unwrap_or_default();
        assert_eq!(target, "task");
        assert!(!is_dm);
    }

    #[test]
    fn normalize_target_dm() {
        let (target, is_dm) = normalize_target("@Alice").unwrap_or_default();
        assert_eq!(target, "@alice");
        assert!(is_dm);
    }

    #[test]
    fn normalize_target_empty() {
        assert!(normalize_target("").is_err());
        assert!(normalize_target("   ").is_err());
    }

    #[test]
    fn normalize_target_invalid_dm() {
        assert!(normalize_target("@").is_err());
        assert!(normalize_target("@bad name!").is_err());
    }

    #[test]
    fn validate_priority_valid() {
        assert!(validate_priority("low").is_ok());
        assert!(validate_priority("normal").is_ok());
        assert!(validate_priority("high").is_ok());
    }

    #[test]
    fn validate_priority_invalid() {
        assert!(validate_priority("urgent").is_err());
        assert!(validate_priority("").is_err());
    }

    #[test]
    fn validate_tag_valid() {
        assert!(validate_tag("bug").is_ok());
        assert!(validate_tag("feature-request").is_ok());
    }

    #[test]
    fn validate_tag_empty() {
        assert!(validate_tag("").is_err());
    }

    #[test]
    fn validate_tag_too_long() {
        let long = "a".repeat(MAX_TAG_LENGTH + 1);
        assert!(validate_tag(&long).is_err());
    }

    #[test]
    fn validate_tag_invalid_chars() {
        assert!(validate_tag("has spaces").is_err());
        assert!(validate_tag("UPPER").is_err());
    }

    #[test]
    fn validate_tags_too_many() {
        let tags: Vec<String> = (0..MAX_TAGS_PER_MESSAGE + 1)
            .map(|i| format!("tag-{i}"))
            .collect();
        assert!(validate_tags(&tags).is_err());
    }

    #[test]
    fn normalize_tags_deduplicates() {
        let tags = vec!["Bug".to_string(), "bug".to_string(), "feature".to_string()];
        let result = normalize_tags(&tags).unwrap_or_default();
        assert_eq!(result, vec!["bug", "feature"]);
    }

    #[test]
    fn normalize_tags_skips_empty() {
        let tags = vec!["bug".to_string(), "  ".to_string(), "feature".to_string()];
        let result = normalize_tags(&tags).unwrap_or_default();
        assert_eq!(result, vec!["bug", "feature"]);
    }

    #[test]
    fn normalize_tags_empty_input() {
        let result = normalize_tags(&[]).unwrap_or_default();
        assert!(result.is_empty());
    }
}
