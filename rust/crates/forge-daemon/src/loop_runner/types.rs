use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopRunnerState {
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRunner {
    pub loop_id: String,
    pub instance_id: String,
    pub config_path: String,
    pub command_path: String,
    pub pid: i32,
    pub state: LoopRunnerState,
    pub last_error: String,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct StartLoopRunnerRequest {
    pub loop_id: String,
    pub config_path: String,
    pub command_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopLoopRunnerResult {
    pub success: bool,
    pub runner: LoopRunner,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LoopRunnerError {
    #[error("loop_id is required")]
    InvalidArgument,
    #[error("loop runner {0:?} already running")]
    AlreadyExists(String),
    #[error("loop runner {0:?} not found")]
    NotFound(String),
    #[error("failed to start loop runner: {0}")]
    StartFailed(String),
    #[error("failed to stop loop runner {0:?}: {1}")]
    StopFailed(String, String),
    #[error("loop runner {0:?} has no process handle")]
    NoProcessHandle(String),
}
