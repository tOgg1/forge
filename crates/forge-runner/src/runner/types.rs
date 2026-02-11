use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const EVENT_TYPE_HEARTBEAT: &str = "heartbeat";
pub const EVENT_TYPE_INPUT_SENT: &str = "input_sent";
pub const EVENT_TYPE_OUTPUT_LINE: &str = "output_line";
pub const EVENT_TYPE_PROMPT_READY: &str = "prompt_ready";
pub const EVENT_TYPE_BUSY: &str = "busy";
pub const EVENT_TYPE_PAUSE: &str = "pause";
pub const EVENT_TYPE_COOLDOWN: &str = "cooldown";
pub const EVENT_TYPE_SWAP_ACCOUNT: &str = "swap_account";
pub const EVENT_TYPE_EXIT: &str = "exit";
pub const EVENT_TYPE_CONTROL_ERROR: &str = "control_error";

pub const DEFAULT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
pub const DEFAULT_TAIL_LINES: usize = 50;
pub const DEFAULT_TAIL_BYTES: usize = 4096;

pub const MAX_PENDING_BYTES: usize = 16384;
pub const MAX_EVENT_LINE_LENGTH: usize = 1024;

pub const DEFAULT_PROMPT_REGEX: &str = r"(?i)(\bready\b|\bidle\b|waiting for input|[>$%])\s*$";
pub const DEFAULT_BUSY_REGEX: &str = r"(?i)(thinking|working|processing|generating)\b";

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("workspace id is required")]
    MissingWorkspaceID,
    #[error("agent id is required")]
    MissingAgentID,
    #[error("command is required")]
    MissingCommand,
    #[error("spawn process: {0}")]
    Spawn(String),
    #[error("io: {0}")]
    Io(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct RunnerEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub workspace_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeartbeatData {
    pub last_activity: String,
    pub idle_for: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tail: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InputSentData {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputLineData {
    pub line: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptReadyData {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusyData {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlData {
    pub action: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub duration: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub until: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlErrorData {
    pub error: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub raw: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExitData {
    pub exit_code: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControlCommand {
    #[serde(rename = "type")]
    pub command_type: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub duration: String,
    #[serde(default)]
    pub until: String,
    #[serde(default)]
    pub account_id: String,
}
