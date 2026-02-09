//! Validation and normalization for fmail names (agents, topics, tags).

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
}
