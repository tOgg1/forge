use std::io::Write;

use serde::Serialize;

/// JSON/JSONL error response shape matching Go's `ErrorEnvelope`.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorPayload,
}

/// Structured error details within the envelope.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Parsed global flags from the root command line.
#[derive(Debug, Clone, Default)]
pub struct GlobalFlags {
    pub json: bool,
    pub jsonl: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub watch: bool,
    pub no_color: bool,
    pub no_progress: bool,
    pub non_interactive: bool,
    pub yes: bool,
    pub robot_help: bool,
    pub config: String,
    pub since: String,
    pub log_level: String,
    pub log_format: String,
    pub chdir: String,
    pub version: bool,
}

/// Parse global flags from the front of the argument list.
/// Returns the parsed flags and the index of the first non-global token.
pub fn parse_global_flags(args: &[String]) -> (GlobalFlags, usize) {
    let mut flags = GlobalFlags::default();
    let mut index = 0usize;
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => flags.json = true,
            "--jsonl" => flags.jsonl = true,
            "--verbose" | "-v" => flags.verbose = true,
            "--quiet" => flags.quiet = true,
            "--watch" => flags.watch = true,
            "--no-color" => flags.no_color = true,
            "--no-progress" => flags.no_progress = true,
            "--non-interactive" => flags.non_interactive = true,
            "--yes" | "-y" => flags.yes = true,
            "--robot-help" => flags.robot_help = true,
            "--version" => flags.version = true,
            "--config" => {
                index += 1;
                if let Some(val) = args.get(index) {
                    flags.config = val.clone();
                }
            }
            "--since" => {
                index += 1;
                if let Some(val) = args.get(index) {
                    flags.since = val.clone();
                }
            }
            "--log-level" => {
                index += 1;
                if let Some(val) = args.get(index) {
                    flags.log_level = val.clone();
                }
            }
            "--log-format" => {
                index += 1;
                if let Some(val) = args.get(index) {
                    flags.log_format = val.clone();
                }
            }
            "--chdir" | "-C" => {
                index += 1;
                if let Some(val) = args.get(index) {
                    flags.chdir = val.clone();
                }
            }
            _ => break,
        }
        index += 1;
    }
    (flags, index)
}

/// Classification result for an error message.
struct Classification {
    code: &'static str,
    hint: Option<String>,
    details: Option<serde_json::Value>,
    exit_code: i32,
}

/// Classify an error message into an error code, hint, details, and exit code.
/// Matches Go's `classifyError` logic.
fn classify_error(message: &str) -> Classification {
    let lower = message.to_lowercase();

    if lower.contains("ambiguous") {
        return Classification {
            code: "ERR_AMBIGUOUS",
            hint: Some("Use a longer prefix or full ID.".to_string()),
            details: None,
            exit_code: 1,
        };
    }
    if lower.contains("not found") {
        let (resource, id) = infer_resource_and_id(&lower, message);
        let hint = list_hint_for_resource(&resource);
        let details = if !resource.is_empty() {
            let mut map = serde_json::Map::new();
            map.insert("resource".to_string(), serde_json::Value::String(resource));
            if !id.is_empty() {
                map.insert("id".to_string(), serde_json::Value::String(id));
            }
            Some(serde_json::Value::Object(map))
        } else {
            None
        };
        return Classification {
            code: "ERR_NOT_FOUND",
            hint: if hint.is_empty() { None } else { Some(hint) },
            details,
            exit_code: 1,
        };
    }
    if lower.contains("already exists") {
        return Classification {
            code: "ERR_EXISTS",
            hint: None,
            details: None,
            exit_code: 1,
        };
    }
    if lower.contains("unknown flag") {
        return Classification {
            code: "ERR_INVALID_FLAG",
            hint: None,
            details: None,
            exit_code: 1,
        };
    }
    if lower.contains("invalid")
        || lower.contains("required")
        || lower.contains("usage")
        || lower.contains("must")
    {
        return Classification {
            code: "ERR_INVALID",
            hint: None,
            details: None,
            exit_code: 1,
        };
    }
    if lower.contains("permission denied")
        || lower.contains("timeout")
        || lower.contains("connection")
    {
        return Classification {
            code: "ERR_OPERATION_FAILED",
            hint: None,
            details: None,
            exit_code: 2,
        };
    }
    if lower.contains("failed to") || lower.contains("unable to") {
        return Classification {
            code: "ERR_OPERATION_FAILED",
            hint: None,
            details: None,
            exit_code: 2,
        };
    }

    Classification {
        code: "ERR_UNKNOWN",
        hint: None,
        details: None,
        exit_code: 1,
    }
}

fn infer_resource_and_id(lower: &str, original: &str) -> (String, String) {
    let resource = if lower.contains("workspace") {
        "workspace"
    } else if lower.contains("node") {
        "node"
    } else if lower.contains("agent") {
        "agent"
    } else {
        ""
    };
    (resource.to_string(), extract_quoted_value(original))
}

fn extract_quoted_value(message: &str) -> String {
    if let Some(start) = message.find('\'') {
        if let Some(end) = message[start + 1..].find('\'') {
            return message[start + 1..start + 1 + end].to_string();
        }
    }
    String::new()
}

fn list_hint_for_resource(resource: &str) -> String {
    match resource {
        "node" => "Run `forge node list` to see valid IDs.".to_string(),
        "workspace" => "Run `forge ws list` to see valid IDs.".to_string(),
        "agent" => "Run `forge agent list` to see valid IDs.".to_string(),
        _ => String::new(),
    }
}

/// Build an `ErrorEnvelope` from an error message.
pub fn build_error_envelope(message: &str) -> ErrorEnvelope {
    let c = classify_error(message);
    ErrorEnvelope {
        error: ErrorPayload {
            code: c.code.to_string(),
            message: message.to_string(),
            hint: c.hint,
            details: c.details,
        },
    }
}

/// Determine the exit code for an error message.
pub fn exit_code_from_error(message: &str) -> i32 {
    classify_error(message).exit_code
}

/// Handle a CLI error: if JSON/JSONL mode, write the error envelope to stdout;
/// otherwise write the plain message to stderr. Returns the exit code.
pub fn handle_cli_error(
    message: &str,
    flags: &GlobalFlags,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let exit_code = exit_code_from_error(message);
    if flags.json || flags.jsonl {
        let envelope = build_error_envelope(message);
        if flags.jsonl {
            if let Ok(data) = serde_json::to_string(&envelope) {
                let _ = writeln!(stdout, "{data}");
            }
        } else if let Ok(data) = serde_json::to_string_pretty(&envelope) {
            let _ = writeln!(stdout, "{data}");
        }
    } else {
        let _ = writeln!(stderr, "{message}");
    }
    exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_utf8(bytes: Vec<u8>) -> String {
        match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(err) => panic!("expected utf-8 output: {err}"),
        }
    }

    #[test]
    fn parse_json_flag() {
        let args: Vec<String> = vec!["--json".into(), "kill".into()];
        let (flags, idx) = parse_global_flags(&args);
        assert!(flags.json);
        assert_eq!(idx, 1);
    }

    #[test]
    fn parse_multiple_flags() {
        let args: Vec<String> = vec![
            "--json".into(),
            "--verbose".into(),
            "--quiet".into(),
            "ps".into(),
        ];
        let (flags, idx) = parse_global_flags(&args);
        assert!(flags.json);
        assert!(flags.verbose);
        assert!(flags.quiet);
        assert_eq!(idx, 3);
    }

    #[test]
    fn parse_version_flag() {
        let args: Vec<String> = vec!["--version".into()];
        let (flags, idx) = parse_global_flags(&args);
        assert!(flags.version);
        assert_eq!(idx, 1);
    }

    #[test]
    fn parse_config_with_value() {
        let args: Vec<String> = vec!["--config".into(), "/tmp/config.yaml".into(), "ps".into()];
        let (flags, idx) = parse_global_flags(&args);
        assert_eq!(flags.config, "/tmp/config.yaml");
        assert_eq!(idx, 2);
    }

    #[test]
    fn parse_chdir_short() {
        let args: Vec<String> = vec!["-C".into(), "/tmp".into(), "run".into()];
        let (flags, idx) = parse_global_flags(&args);
        assert_eq!(flags.chdir, "/tmp");
        assert_eq!(idx, 2);
    }

    #[test]
    fn parse_no_flags() {
        let args: Vec<String> = vec!["kill".into(), "--all".into()];
        let (flags, idx) = parse_global_flags(&args);
        assert!(!flags.json);
        assert_eq!(idx, 0);
    }

    #[test]
    fn classify_ambiguous() {
        let envelope = build_error_envelope("ambiguous loop prefix 'ab'");
        assert_eq!(envelope.error.code, "ERR_AMBIGUOUS");
        assert_eq!(
            envelope.error.hint.as_deref(),
            Some("Use a longer prefix or full ID.")
        );
    }

    #[test]
    fn classify_not_found_with_resource() {
        let envelope = build_error_envelope("node 'abc123' not found");
        assert_eq!(envelope.error.code, "ERR_NOT_FOUND");
        assert_eq!(
            envelope.error.hint.as_deref(),
            Some("Run `forge node list` to see valid IDs.")
        );
        let details = match envelope.error.details {
            Some(details) => details,
            None => panic!("expected details for not-found classification"),
        };
        assert_eq!(details["resource"], "node");
        assert_eq!(details["id"], "abc123");
    }

    #[test]
    fn classify_already_exists() {
        let envelope = build_error_envelope("pool 'main' already exists");
        assert_eq!(envelope.error.code, "ERR_EXISTS");
    }

    #[test]
    fn classify_unknown_flag() {
        let envelope = build_error_envelope("unknown flag: --foobar");
        assert_eq!(envelope.error.code, "ERR_INVALID_FLAG");
    }

    #[test]
    fn classify_invalid() {
        let envelope = build_error_envelope("invalid value for --count");
        assert_eq!(envelope.error.code, "ERR_INVALID");
    }

    #[test]
    fn classify_operation_failed_permission() {
        let code = exit_code_from_error("permission denied on /var/db");
        assert_eq!(code, 2);
    }

    #[test]
    fn classify_operation_failed_timeout() {
        let code = exit_code_from_error("connection timeout reached");
        assert_eq!(code, 2);
    }

    #[test]
    fn classify_failed_to() {
        let envelope = build_error_envelope("failed to open database");
        assert_eq!(envelope.error.code, "ERR_OPERATION_FAILED");
        assert_eq!(exit_code_from_error("failed to open database"), 2);
    }

    #[test]
    fn classify_unknown() {
        let envelope = build_error_envelope("something went wrong");
        assert_eq!(envelope.error.code, "ERR_UNKNOWN");
        assert_eq!(exit_code_from_error("something went wrong"), 1);
    }

    #[test]
    fn handle_cli_error_json_mode() {
        let flags = GlobalFlags {
            json: true,
            ..Default::default()
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = handle_cli_error("node 'x' not found", &flags, &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        let out = decode_utf8(stdout);
        assert!(out.contains("ERR_NOT_FOUND"));
        assert!(out.contains("node 'x' not found"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn handle_cli_error_text_mode() {
        let flags = GlobalFlags::default();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = handle_cli_error("something broke", &flags, &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert_eq!(decode_utf8(stderr), "something broke\n");
    }

    #[test]
    fn handle_cli_error_jsonl_mode() {
        let flags = GlobalFlags {
            jsonl: true,
            ..Default::default()
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = handle_cli_error("timeout waiting", &flags, &mut stdout, &mut stderr);
        assert_eq!(code, 2);
        let out = decode_utf8(stdout);
        // JSONL is compact single line
        assert!(!out.contains('\n') || out.trim_end().matches('\n').count() == 0);
        assert!(out.contains("ERR_OPERATION_FAILED"));
        assert!(stderr.is_empty());
    }
}
