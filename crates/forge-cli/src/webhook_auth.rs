use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const WEBHOOK_BEARER_TOKEN_ENV: &str = "FORGE_WEBHOOK_BEARER_TOKEN";
pub const WEBHOOK_BEARER_TOKEN_KEY: &str = "webhook_bearer_token";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WebhookAuthConfig {
    pub bearer_token: String,
}

impl WebhookAuthConfig {
    #[must_use]
    pub fn enabled(&self) -> bool {
        !self.bearer_token.trim().is_empty()
    }

    #[must_use]
    pub fn from_env() -> Self {
        Self {
            bearer_token: std::env::var(WEBHOOK_BEARER_TOKEN_ENV)
                .unwrap_or_default()
                .trim()
                .to_string(),
        }
    }

    #[must_use]
    pub fn from_map(values: &BTreeMap<String, String>) -> Self {
        if let Some(token) = values.get(WEBHOOK_BEARER_TOKEN_KEY) {
            return Self {
                bearer_token: token.trim().to_string(),
            };
        }
        Self::from_env()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookAuthDecision {
    pub authorized: bool,
    pub status_code: u16,
    pub reason: String,
}

impl WebhookAuthDecision {
    #[must_use]
    pub fn authorized() -> Self {
        Self {
            authorized: true,
            status_code: 200,
            reason: "authorized".to_string(),
        }
    }

    #[must_use]
    pub fn unauthorized(reason: impl Into<String>) -> Self {
        Self {
            authorized: false,
            status_code: 401,
            reason: reason.into(),
        }
    }
}

#[must_use]
pub fn authorize_webhook_request(
    config: &WebhookAuthConfig,
    headers: &BTreeMap<String, String>,
) -> WebhookAuthDecision {
    if !config.enabled() {
        return WebhookAuthDecision::authorized();
    }

    let Some(authorization) = find_header_value(headers, "authorization") else {
        return WebhookAuthDecision::unauthorized("missing Authorization header");
    };

    authorize_authorization_header(config, authorization)
}

#[must_use]
pub fn authorize_authorization_header(
    config: &WebhookAuthConfig,
    authorization_header: &str,
) -> WebhookAuthDecision {
    if !config.enabled() {
        return WebhookAuthDecision::authorized();
    }

    let Some(token) = parse_bearer_token(authorization_header) else {
        return WebhookAuthDecision::unauthorized("Authorization must use Bearer token format");
    };
    if token == config.bearer_token {
        WebhookAuthDecision::authorized()
    } else {
        WebhookAuthDecision::unauthorized("invalid bearer token")
    }
}

fn find_header_value<'a>(headers: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    for (header_name, value) in headers {
        if header_name.eq_ignore_ascii_case(key) {
            return Some(value.trim());
        }
    }
    None
}

fn parse_bearer_token(header_value: &str) -> Option<&str> {
    let trimmed = header_value.trim();
    let remainder = if let Some(value) = trimmed.strip_prefix("Bearer ") {
        value
    } else if let Some(value) = trimmed.strip_prefix("bearer ") {
        value
    } else {
        return None;
    };

    let token = remainder.trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        authorize_webhook_request, WebhookAuthConfig, WEBHOOK_BEARER_TOKEN_ENV,
        WEBHOOK_BEARER_TOKEN_KEY,
    };
    use std::collections::BTreeMap;

    fn headers(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for (key, value) in entries {
            out.insert((*key).to_string(), (*value).to_string());
        }
        out
    }

    fn with_env_var<T>(key: &str, value: Option<&str>, callback: impl FnOnce() -> T) -> T {
        let prior = std::env::var(key).ok();
        match value {
            Some(next) => std::env::set_var(key, next),
            None => std::env::remove_var(key),
        }
        let result = callback();
        match prior {
            Some(existing) => std::env::set_var(key, existing),
            None => std::env::remove_var(key),
        }
        result
    }

    #[test]
    fn auth_disabled_allows_request_without_header() {
        let config = WebhookAuthConfig::default();
        let decision = authorize_webhook_request(&config, &BTreeMap::new());
        assert!(decision.authorized);
        assert_eq!(decision.status_code, 200);
    }

    #[test]
    fn missing_authorization_header_rejected_with_401() {
        let config = WebhookAuthConfig {
            bearer_token: "secret".to_string(),
        };
        let decision = authorize_webhook_request(&config, &BTreeMap::new());
        assert!(!decision.authorized);
        assert_eq!(decision.status_code, 401);
        assert_eq!(decision.reason, "missing Authorization header");
    }

    #[test]
    fn invalid_scheme_rejected_with_401() {
        let config = WebhookAuthConfig {
            bearer_token: "secret".to_string(),
        };
        let request_headers = headers(&[("Authorization", "Token secret")]);
        let decision = authorize_webhook_request(&config, &request_headers);
        assert!(!decision.authorized);
        assert_eq!(decision.status_code, 401);
        assert_eq!(
            decision.reason,
            "Authorization must use Bearer token format"
        );
    }

    #[test]
    fn wrong_bearer_token_rejected_with_401() {
        let config = WebhookAuthConfig {
            bearer_token: "secret".to_string(),
        };
        let request_headers = headers(&[("Authorization", "Bearer other")]);
        let decision = authorize_webhook_request(&config, &request_headers);
        assert!(!decision.authorized);
        assert_eq!(decision.status_code, 401);
        assert_eq!(decision.reason, "invalid bearer token");
    }

    #[test]
    fn correct_bearer_token_authorized() {
        let config = WebhookAuthConfig {
            bearer_token: "secret".to_string(),
        };
        let request_headers = headers(&[("authorization", "Bearer secret")]);
        let decision = authorize_webhook_request(&config, &request_headers);
        assert!(decision.authorized);
        assert_eq!(decision.status_code, 200);
        assert_eq!(decision.reason, "authorized");
    }

    #[test]
    fn from_map_uses_token_key() {
        let mut values = BTreeMap::new();
        values.insert(
            WEBHOOK_BEARER_TOKEN_KEY.to_string(),
            " token-123 ".to_string(),
        );
        let config = WebhookAuthConfig::from_map(&values);
        assert_eq!(config.bearer_token, "token-123");
    }

    #[test]
    fn from_map_falls_back_to_env_when_key_missing() {
        with_env_var(WEBHOOK_BEARER_TOKEN_ENV, Some("from-env"), || {
            let config = WebhookAuthConfig::from_map(&BTreeMap::new());
            assert_eq!(config.bearer_token, "from-env");
        });
    }
}
