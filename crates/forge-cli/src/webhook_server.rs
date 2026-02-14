use std::collections::BTreeMap;
#[cfg(test)]
use std::env;
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::job::{CronTriggerRecord, JobStore};
use crate::webhook_auth::{authorize_webhook_request, WebhookAuthConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookServerConfig {
    pub enabled: bool,
    pub bearer_token: String,
}

impl Default for WebhookServerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bearer_token: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookHttpRequest {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookHttpResponse {
    pub status_code: u16,
    pub body: String,
}

#[must_use]
pub fn crate_label() -> &'static str {
    "forge-cli/webhook-server"
}

#[must_use]
pub fn validate_request_gate(
    config: &WebhookServerConfig,
    request: &WebhookHttpRequest,
) -> Option<WebhookHttpResponse> {
    if !config.enabled {
        return Some(json_response(503, json!({ "error": "webhook disabled" })));
    }

    if !request.method.eq_ignore_ascii_case("POST") {
        return Some(json_response(405, json!({ "error": "method not allowed" })));
    }

    let decision = authorize_webhook_request(
        &WebhookAuthConfig {
            bearer_token: config.bearer_token.clone(),
        },
        &request.headers,
    );
    if !decision.authorized {
        return Some(json_response(401, json!({ "error": "unauthorized" })));
    }

    None
}

#[must_use]
pub fn handle_webhook_request(
    config: &WebhookServerConfig,
    store: &JobStore,
    request: &WebhookHttpRequest,
) -> WebhookHttpResponse {
    if let Some(response) = validate_request_gate(config, request) {
        return response;
    }

    let request_path = normalize_request_path(&request.path);
    let matched_trigger = match resolve_webhook_trigger(store, &request_path) {
        Ok(WebhookTriggerResolution::Matched(trigger)) => trigger,
        Ok(WebhookTriggerResolution::NotFound) => {
            return json_response(
                404,
                json!({
                    "error": format!("no webhook trigger for path {}", request_path),
                }),
            )
        }
        Ok(WebhookTriggerResolution::Ambiguous) => {
            return json_response(
                409,
                json!({
                    "error": format!("multiple webhook triggers configured for path {}", request_path),
                }),
            )
        }
        Err(err) => return json_response(500, json!({ "error": err })),
    };

    let trigger_source = format!("webhook:{}", matched_trigger.cron);
    match store.record_run(
        &matched_trigger.job_name,
        &trigger_source,
        webhook_inputs(request, &request_path),
    ) {
        Ok(run) => json_response(
            200,
            json!({
                "run_id": run.run_id,
                "job_name": run.job_name,
                "status": run.status,
            }),
        ),
        Err(err) => json_response(500, json!({ "error": err })),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WebhookTriggerResolution {
    Matched(CronTriggerRecord),
    NotFound,
    Ambiguous,
}

fn resolve_webhook_trigger(
    store: &JobStore,
    request_path: &str,
) -> Result<WebhookTriggerResolution, String> {
    let matching = store
        .list_triggers()?
        .into_iter()
        .filter(|trigger| {
            trigger.enabled
                && trigger.trigger_type == "webhook"
                && normalize_request_path(&trigger.cron) == request_path
        })
        .collect::<Vec<_>>();

    match matching.len() {
        0 => Ok(WebhookTriggerResolution::NotFound),
        1 => match matching.into_iter().next() {
            Some(trigger) => Ok(WebhookTriggerResolution::Matched(trigger)),
            None => Err("internal error: expected one trigger".to_string()),
        },
        _ => Ok(WebhookTriggerResolution::Ambiguous),
    }
}

fn normalize_request_path(path: &str) -> String {
    let trimmed = path.trim();
    let normalized = trimmed
        .split_once('?')
        .map_or(trimmed, |(base, _)| base)
        .trim()
        .to_string();
    if normalized.is_empty() {
        "/".to_string()
    } else {
        normalized
    }
}

fn webhook_inputs(request: &WebhookHttpRequest, request_path: &str) -> BTreeMap<String, String> {
    let mut inputs = BTreeMap::new();
    inputs.insert("webhook_path".to_string(), request_path.to_string());

    let payload = String::from_utf8_lossy(&request.body).trim().to_string();
    if !payload.is_empty() {
        inputs.insert("payload".to_string(), payload);
    }

    if let Some(event_id) = header_value(&request.headers, "x-event-id") {
        inputs.insert("event_id".to_string(), event_id.to_string());
    }
    inputs
}

fn header_value<'a>(headers: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    for (name, value) in headers {
        if name.eq_ignore_ascii_case(key) {
            return Some(value.trim());
        }
    }
    None
}

fn json_response(status_code: u16, body: serde_json::Value) -> WebhookHttpResponse {
    WebhookHttpResponse {
        status_code,
        body: body.to_string(),
    }
}

#[cfg(test)]
fn temp_store(tag: &str) -> JobStore {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let path = env::temp_dir().join(format!("forge-webhook-test-{tag}-{nanos}"));
    JobStore::new(path)
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::{
        crate_label, handle_webhook_request, temp_store, validate_request_gate, WebhookHttpRequest,
        WebhookServerConfig,
    };
    use crate::job::JobStore;
    use std::collections::BTreeMap;

    fn request(method: &str, headers: &[(&str, &str)]) -> WebhookHttpRequest {
        let mut map = BTreeMap::new();
        for (key, value) in headers {
            map.insert((*key).to_string(), (*value).to_string());
        }
        WebhookHttpRequest {
            method: method.to_string(),
            path: "/hooks/nightly".to_string(),
            headers: map,
            body: br#"{"job":"nightly"}"#.to_vec(),
        }
    }

    fn cleanup(store: &JobStore) {
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-cli/webhook-server");
    }

    #[test]
    fn disabled_webhook_returns_503() {
        let config = WebhookServerConfig {
            enabled: false,
            bearer_token: String::new(),
        };
        let response = match validate_request_gate(&config, &request("POST", &[])) {
            Some(response) => response,
            None => panic!("response"),
        };
        assert_eq!(response.status_code, 503);
    }

    #[test]
    fn missing_bearer_header_returns_401() {
        let config = WebhookServerConfig {
            enabled: true,
            bearer_token: "secret".to_string(),
        };
        let response = match validate_request_gate(&config, &request("POST", &[])) {
            Some(response) => response,
            None => panic!("response"),
        };
        assert_eq!(response.status_code, 401);
    }

    #[test]
    fn wrong_bearer_token_returns_401() {
        let config = WebhookServerConfig {
            enabled: true,
            bearer_token: "secret".to_string(),
        };
        let response = validate_request_gate(
            &config,
            &request("POST", &[("Authorization", "Bearer wrong")]),
        );
        let response = match response {
            Some(response) => response,
            None => panic!("response"),
        };
        assert_eq!(response.status_code, 401);
    }

    #[test]
    fn valid_bearer_token_passes_gate() {
        let config = WebhookServerConfig {
            enabled: true,
            bearer_token: "secret".to_string(),
        };
        let response = validate_request_gate(
            &config,
            &request("POST", &[("Authorization", "Bearer secret")]),
        );
        assert!(response.is_none());
    }

    #[test]
    fn no_token_config_allows_post_without_auth() {
        let config = WebhookServerConfig::default();
        let response = validate_request_gate(&config, &request("POST", &[]));
        assert!(response.is_none());
    }

    #[test]
    fn non_post_method_returns_405() {
        let config = WebhookServerConfig::default();
        let response = match validate_request_gate(&config, &request("GET", &[])) {
            Some(response) => response,
            None => panic!("response"),
        };
        assert_eq!(response.status_code, 405);
    }

    #[test]
    fn routed_webhook_records_job_run_and_returns_run_id() {
        let store = temp_store("route-success");
        let now = "2026-02-13T00:00:00Z";
        if let Err(err) = store.create_job("nightly", "wf-nightly", now) {
            panic!("create job: {err}");
        }
        let base = match DateTime::parse_from_rfc3339(now) {
            Ok(base) => base.with_timezone(&Utc),
            Err(err) => panic!("parse timestamp: {err}"),
        };
        if let Err(err) = store.create_webhook_trigger("nightly", "/hooks/nightly", base) {
            panic!("create trigger: {err}");
        }

        let mut req = request("POST", &[("Authorization", "Bearer secret")]);
        req.path = "/hooks/nightly".to_string();
        req.headers
            .insert("Authorization".to_string(), "Bearer secret".to_string());

        let config = WebhookServerConfig {
            enabled: true,
            bearer_token: "secret".to_string(),
        };
        let response = handle_webhook_request(&config, &store, &req);
        assert_eq!(response.status_code, 200, "body={}", response.body);
        assert!(response.body.contains("\"run_id\""));
        assert!(response.body.contains("\"job_name\":\"nightly\""));

        let runs = match store.list_runs("nightly", 10) {
            Ok(runs) => runs,
            Err(err) => panic!("list runs: {err}"),
        };
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].trigger, "webhook:/hooks/nightly");
        cleanup(&store);
    }

    #[test]
    fn unknown_webhook_path_returns_404() {
        let store = temp_store("route-404");
        let now = "2026-02-13T00:00:00Z";
        if let Err(err) = store.create_job("nightly", "wf-nightly", now) {
            panic!("create job: {err}");
        }
        let base = match DateTime::parse_from_rfc3339(now) {
            Ok(base) => base.with_timezone(&Utc),
            Err(err) => panic!("parse timestamp: {err}"),
        };
        if let Err(err) = store.create_webhook_trigger("nightly", "/hooks/nightly", base) {
            panic!("create trigger: {err}");
        }

        let mut req = request("POST", &[]);
        req.path = "/hooks/unknown".to_string();
        let response = handle_webhook_request(&WebhookServerConfig::default(), &store, &req);
        assert_eq!(response.status_code, 404, "body={}", response.body);
        assert!(response.body.contains("no webhook trigger"));
        cleanup(&store);
    }

    #[test]
    fn duplicate_webhook_path_returns_409() {
        let store = temp_store("route-409");
        let now = "2026-02-13T00:00:00Z";
        if let Err(err) = store.create_job("nightly", "wf-1", now) {
            panic!("create job 1: {err}");
        }
        if let Err(err) = store.create_job("release", "wf-2", now) {
            panic!("create job 2: {err}");
        }
        let base = match DateTime::parse_from_rfc3339(now) {
            Ok(base) => base.with_timezone(&Utc),
            Err(err) => panic!("parse timestamp: {err}"),
        };
        if let Err(err) = store.create_webhook_trigger("nightly", "/hooks/shared", base) {
            panic!("create trigger 1: {err}");
        }
        if let Err(err) = store.create_webhook_trigger("release", "/hooks/shared", base) {
            panic!("create trigger 2: {err}");
        }

        let mut req = request("POST", &[]);
        req.path = "/hooks/shared".to_string();
        let response = handle_webhook_request(&WebhookServerConfig::default(), &store, &req);
        assert_eq!(response.status_code, 409, "body={}", response.body);
        assert!(response.body.contains("multiple webhook triggers"));
        cleanup(&store);
    }
}
