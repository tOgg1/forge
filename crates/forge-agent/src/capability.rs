//! Harness capability registry and spawn-mode guardrails.

use crate::error::AgentServiceError;
use crate::types::{AgentRequestMode, SpawnAgentParams};

/// Execution mode of a concrete harness command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandMode {
    Interactive,
    OneShot,
}

impl CommandMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::OneShot => "one-shot",
        }
    }
}

/// Capabilities for a harness family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarnessCapability {
    pub harness: &'static str,
    pub supports_interactive_agent: bool,
    pub supports_reliable_idle_detection: bool,
    pub supports_approval_signal: bool,
    pub default_command_mode: CommandMode,
    pub interactive_hint: &'static str,
}

impl HarnessCapability {
    pub fn detail_line(self) -> String {
        format!(
            "interactive_agent={}, reliable_idle_detection={}, approval_signal={}, default_mode={}",
            self.supports_interactive_agent,
            self.supports_reliable_idle_detection,
            self.supports_approval_signal,
            self.default_command_mode.as_str()
        )
    }
}

pub fn capability_for_harness(harness: &str) -> HarnessCapability {
    match normalize_harness(harness) {
        "codex" => HarnessCapability {
            harness: "codex",
            supports_interactive_agent: true,
            supports_reliable_idle_detection: true,
            supports_approval_signal: true,
            default_command_mode: CommandMode::Interactive,
            interactive_hint: "use interactive codex command (e.g. 'codex')",
        },
        "claude" => HarnessCapability {
            harness: "claude",
            supports_interactive_agent: true,
            supports_reliable_idle_detection: true,
            supports_approval_signal: true,
            default_command_mode: CommandMode::Interactive,
            interactive_hint: "use interactive claude command (e.g. 'claude')",
        },
        "opencode" => HarnessCapability {
            harness: "opencode",
            supports_interactive_agent: true,
            supports_reliable_idle_detection: false,
            supports_approval_signal: false,
            default_command_mode: CommandMode::Interactive,
            interactive_hint: "use interactive opencode command (e.g. 'opencode')",
        },
        "droid" => HarnessCapability {
            harness: "droid",
            supports_interactive_agent: true,
            supports_reliable_idle_detection: false,
            supports_approval_signal: false,
            default_command_mode: CommandMode::Interactive,
            interactive_hint: "use interactive droid command (e.g. 'droid')",
        },
        _ => HarnessCapability {
            harness: "generic",
            supports_interactive_agent: false,
            supports_reliable_idle_detection: false,
            supports_approval_signal: false,
            default_command_mode: CommandMode::OneShot,
            interactive_hint: "use a supported interactive harness command",
        },
    }
}

pub fn command_mode_for_spawn(
    adapter: &str,
    command: &str,
    args: &[String],
) -> (HarnessCapability, CommandMode) {
    let fallback_harness = first_token(command);
    let harness_name = if adapter.trim().is_empty() {
        fallback_harness
    } else {
        adapter
    };

    let capability = capability_for_harness(harness_name);
    let mode = detect_command_mode(capability.harness, command, args)
        .unwrap_or(capability.default_command_mode);
    (capability, mode)
}

pub fn validate_spawn_guardrails(
    params: &SpawnAgentParams,
) -> Result<HarnessCapability, AgentServiceError> {
    let (capability, command_mode) =
        command_mode_for_spawn(&params.adapter, &params.command, &params.args);
    let requested_mode = if params.requested_mode == AgentRequestMode::OneShot {
        "one-shot"
    } else {
        requested_mode_from_env(params)
    };
    let allow_oneshot_fallback = params.allow_oneshot_fallback || allow_oneshot_fallback(params);

    if requested_mode == "continuous"
        && command_mode == CommandMode::OneShot
        && !allow_oneshot_fallback
    {
        return Err(AgentServiceError::CapabilityMismatch {
            adapter: capability.harness.to_string(),
            requested_mode: requested_mode.to_string(),
            command_mode: command_mode.as_str().to_string(),
            hint: format!(
                "{} instead of one-shot mode, or pass explicit override",
                capability.interactive_hint
            ),
        });
    }

    Ok(capability)
}

fn requested_mode_from_env(params: &SpawnAgentParams) -> &'static str {
    match params
        .env
        .get("FORGE_AGENT_REQUESTED_MODE")
        .map(|value| value.trim().to_ascii_lowercase())
    {
        Some(value) if value == "one-shot" || value == "oneshot" || value == "one_shot" => {
            "one-shot"
        }
        _ => "continuous",
    }
}

fn allow_oneshot_fallback(params: &SpawnAgentParams) -> bool {
    params
        .env
        .get("FORGE_AGENT_ALLOW_ONESHOT")
        .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn normalize_harness(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("claude_code") || trimmed.eq_ignore_ascii_case("claude-code") {
        "claude"
    } else if trimmed.eq_ignore_ascii_case("factory") {
        "droid"
    } else if trimmed.is_empty() {
        "generic"
    } else if trimmed.eq_ignore_ascii_case("codex") {
        "codex"
    } else if trimmed.eq_ignore_ascii_case("claude") {
        "claude"
    } else if trimmed.eq_ignore_ascii_case("opencode") {
        "opencode"
    } else if trimmed.eq_ignore_ascii_case("droid") {
        "droid"
    } else {
        "generic"
    }
}

fn first_token(value: &str) -> &str {
    value.split_whitespace().next().unwrap_or_default()
}

fn detect_command_mode(harness: &str, command: &str, args: &[String]) -> Option<CommandMode> {
    let mut tokens: Vec<String> = command
        .split_whitespace()
        .map(|token| token.to_string())
        .collect();
    tokens.extend(args.iter().cloned());
    if tokens.is_empty() {
        return None;
    }

    let subcommand = tokens.get(1).map(|s| s.as_str()).unwrap_or_default();
    let has_print_flag = tokens
        .iter()
        .any(|token| token == "-p" || token == "--print");

    match normalize_harness(harness) {
        "codex" => {
            if subcommand == "exec" {
                Some(CommandMode::OneShot)
            } else {
                Some(CommandMode::Interactive)
            }
        }
        "opencode" => {
            if subcommand == "run" || subcommand == "exec" {
                Some(CommandMode::OneShot)
            } else {
                Some(CommandMode::Interactive)
            }
        }
        "claude" => {
            if has_print_flag {
                Some(CommandMode::OneShot)
            } else {
                Some(CommandMode::Interactive)
            }
        }
        "droid" => Some(CommandMode::Interactive),
        _ => Some(CommandMode::OneShot),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn spawn_params(command: &str, args: &[&str]) -> SpawnAgentParams {
        SpawnAgentParams {
            agent_id: "a1".to_string(),
            workspace_id: "ws-1".to_string(),
            command: command.to_string(),
            args: args.iter().map(|v| (*v).to_string()).collect(),
            env: std::collections::HashMap::new(),
            working_dir: "/tmp".to_string(),
            session_name: String::new(),
            adapter: "codex".to_string(),
            requested_mode: AgentRequestMode::Continuous,
            allow_oneshot_fallback: false,
        }
    }

    #[test]
    fn codex_exec_detected_as_oneshot() {
        let (cap, mode) = command_mode_for_spawn("codex", "codex exec", &[]);
        assert_eq!(cap.harness, "codex");
        assert_eq!(mode, CommandMode::OneShot);
    }

    #[test]
    fn codex_interactive_detected_as_interactive() {
        let (_, mode) = command_mode_for_spawn("codex", "codex", &[]);
        assert_eq!(mode, CommandMode::Interactive);
    }

    #[test]
    fn continuous_mode_rejected_for_oneshot_command() {
        let params = spawn_params("codex exec", &[]);
        let err = validate_spawn_guardrails(&params).expect_err("expected mismatch");
        match err {
            AgentServiceError::CapabilityMismatch {
                adapter,
                requested_mode,
                command_mode,
                hint,
            } => {
                assert_eq!(adapter, "codex");
                assert_eq!(requested_mode, "continuous");
                assert_eq!(command_mode, "one-shot");
                assert!(hint.contains("interactive codex command"));
            }
            other => panic!("expected CapabilityMismatch, got {other:?}"),
        }
    }

    #[test]
    fn explicit_override_allows_oneshot_for_continuous_request() {
        let mut params = spawn_params("codex exec", &[]);
        params
            .env
            .insert("FORGE_AGENT_ALLOW_ONESHOT".to_string(), "1".to_string());
        assert!(validate_spawn_guardrails(&params).is_ok());
    }

    #[test]
    fn one_shot_requested_mode_allows_oneshot_command() {
        let mut params = spawn_params("codex exec", &[]);
        params.requested_mode = AgentRequestMode::OneShot;
        assert!(validate_spawn_guardrails(&params).is_ok());
    }

    #[test]
    fn generic_harness_defaults_to_oneshot() {
        let (cap, mode) = command_mode_for_spawn("unknown", "foo", &[]);
        assert_eq!(cap.harness, "generic");
        assert_eq!(mode, CommandMode::OneShot);
    }
}
