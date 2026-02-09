//! Message type and ID generation ported from Go `internal/fmail/message.go`.

use std::sync::atomic::{AtomicU32, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::validate::{normalize_agent_name, normalize_target, validate_priority, validate_tags};

pub const PRIORITY_LOW: &str = "low";
pub const PRIORITY_NORMAL: &str = "normal";
pub const PRIORITY_HIGH: &str = "high";

pub const MAX_MESSAGE_SIZE: usize = 1 << 20; // 1MB

static ID_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A mail message, matching Go's `Message` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub from: String,
    pub to: String,
    pub time: DateTime<Utc>,
    pub body: serde_json::Value,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reply_to: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub priority: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Generate a sortable message ID using UTC time and a per-process sequence counter.
///
/// Go parity: `GenerateMessageID` in `message.go`.
pub fn generate_message_id(now: DateTime<Utc>) -> String {
    let seq = ID_COUNTER.fetch_add(1, Ordering::Relaxed) % 10000;
    format!("{}-{:04}", now.format("%Y%m%d-%H%M%S"), seq)
}

impl Message {
    /// Validate required fields and basic constraints.
    ///
    /// Go parity: `Message.Validate()`.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("missing id".to_string());
        }
        normalize_agent_name(&self.from).map_err(|_| format!("invalid from: {}", self.from))?;
        normalize_target(&self.to).map_err(|_| format!("invalid to: {}", self.to))?;
        if self.body.is_null() && self.body == serde_json::Value::Null {
            // null is a valid body in Go (json.RawMessage("null"))
        }
        if !self.priority.is_empty() {
            validate_priority(&self.priority)?;
        }
        if !self.tags.is_empty() {
            validate_tags(&self.tags)?;
        }
        Ok(())
    }
}

/// Parse a raw string as a message body. If it's valid JSON, return the parsed value;
/// otherwise return it as a JSON string.
///
/// Go parity: `parseMessageBody`.
pub fn parse_message_body(raw: &str) -> Result<serde_json::Value, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty message body".to_string());
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(value);
    }
    Ok(serde_json::Value::String(raw.to_string()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn generate_id_format() {
        let now = "2026-02-09T12:30:45Z"
            .parse::<DateTime<Utc>>()
            .ok()
            .unwrap_or_else(Utc::now);
        let id = generate_message_id(now);
        assert!(id.starts_with("20260209-123045-"), "got: {id}");
        assert!(id.len() > 15, "got: {id}");
    }

    #[test]
    fn generate_id_increments() {
        let now = Utc::now();
        let id1 = generate_message_id(now);
        let id2 = generate_message_id(now);
        assert_ne!(id1, id2);
    }

    #[test]
    fn parse_body_json_object() {
        let body = parse_message_body(r#"{"key": "value"}"#);
        assert!(body.is_ok());
        let val = body.unwrap_or_default();
        assert!(val.is_object());
    }

    #[test]
    fn parse_body_json_array() {
        let body = parse_message_body("[1, 2, 3]");
        assert!(body.is_ok());
        let val = body.unwrap_or_default();
        assert!(val.is_array());
    }

    #[test]
    fn parse_body_json_string() {
        let body = parse_message_body(r#""hello""#);
        assert!(body.is_ok());
    }

    #[test]
    fn parse_body_json_null() {
        let body = parse_message_body("null");
        assert!(body.is_ok());
        let val = body.unwrap_or_default();
        assert!(val.is_null());
    }

    #[test]
    fn parse_body_plain_string() {
        let body = parse_message_body("hello world");
        assert!(body.is_ok());
        let val = body.unwrap_or_default();
        assert!(val.is_string());
        assert_eq!(val.as_str().unwrap_or_default(), "hello world");
    }

    #[test]
    fn parse_body_empty_is_error() {
        assert!(parse_message_body("").is_err());
        assert!(parse_message_body("   ").is_err());
    }

    #[test]
    fn validate_valid_message() {
        let msg = Message {
            id: "20260209-120000-0001".to_string(),
            from: "alice".to_string(),
            to: "task".to_string(),
            time: Utc::now(),
            body: serde_json::Value::String("hello".to_string()),
            reply_to: String::new(),
            priority: String::new(),
            host: String::new(),
            tags: vec![],
        };
        assert!(msg.validate().is_ok());
    }

    #[test]
    fn validate_missing_id() {
        let msg = Message {
            id: String::new(),
            from: "alice".to_string(),
            to: "task".to_string(),
            time: Utc::now(),
            body: serde_json::Value::String("hello".to_string()),
            reply_to: String::new(),
            priority: String::new(),
            host: String::new(),
            tags: vec![],
        };
        let err = msg.validate().unwrap_err();
        assert!(err.contains("missing id"), "got: {err}");
    }

    #[test]
    fn validate_invalid_from() {
        let msg = Message {
            id: "test-id".to_string(),
            from: "INVALID NAME!".to_string(),
            to: "task".to_string(),
            time: Utc::now(),
            body: serde_json::Value::String("hello".to_string()),
            reply_to: String::new(),
            priority: String::new(),
            host: String::new(),
            tags: vec![],
        };
        let err = msg.validate().unwrap_err();
        assert!(err.contains("invalid from"), "got: {err}");
    }

    #[test]
    fn validate_invalid_priority() {
        let msg = Message {
            id: "test-id".to_string(),
            from: "alice".to_string(),
            to: "task".to_string(),
            time: Utc::now(),
            body: serde_json::Value::String("hello".to_string()),
            reply_to: String::new(),
            priority: "urgent".to_string(),
            host: String::new(),
            tags: vec![],
        };
        let err = msg.validate().unwrap_err();
        assert!(err.contains("invalid priority"), "got: {err}");
    }

    #[test]
    fn validate_with_dm_target() {
        let msg = Message {
            id: "test-id".to_string(),
            from: "alice".to_string(),
            to: "@bob".to_string(),
            time: Utc::now(),
            body: serde_json::Value::String("hello".to_string()),
            reply_to: String::new(),
            priority: String::new(),
            host: String::new(),
            tags: vec![],
        };
        assert!(msg.validate().is_ok());
    }

    #[test]
    fn message_serialization_round_trip() {
        let msg = Message {
            id: "20260209-120000-0001".to_string(),
            from: "alice".to_string(),
            to: "task".to_string(),
            time: "2026-02-09T12:00:00Z"
                .parse()
                .unwrap_or_else(|_| Utc::now()),
            body: serde_json::json!({"key": "value"}),
            reply_to: String::new(),
            priority: "high".to_string(),
            host: String::new(),
            tags: vec!["bug".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap_or_default();
        let parsed: Message = serde_json::from_str(&json).unwrap_or_else(|_| msg.clone());
        assert_eq!(parsed.id, msg.id);
        assert_eq!(parsed.from, msg.from);
        assert_eq!(parsed.priority, msg.priority);
        assert_eq!(parsed.tags, msg.tags);
        // reply_to should be absent (skip_serializing_if = "String::is_empty")
        assert!(!json.contains("reply_to"));
        // host should be absent
        assert!(!json.contains("host"));
    }
}
