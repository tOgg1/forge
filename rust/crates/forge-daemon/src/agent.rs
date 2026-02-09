//! Agent registry: in-memory store for daemon-managed agents.
//!
//! Provides thread-safe agent lifecycle management with workspace/state filtering,
//! mirroring Go daemon `agents map[string]*agentInfo` semantics.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};

use crate::transcript::{TranscriptEntry, TranscriptEntryType, TranscriptStore};

/// Agent states matching proto `AgentState` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentState {
    Unspecified,
    Starting,
    Running,
    Idle,
    WaitingApproval,
    Paused,
    Stopping,
    Stopped,
    Failed,
}

impl AgentState {
    pub fn to_proto_i32(self) -> i32 {
        match self {
            Self::Unspecified => 0,
            Self::Starting => 1,
            Self::Running => 2,
            Self::Idle => 3,
            Self::WaitingApproval => 4,
            Self::Paused => 5,
            Self::Stopping => 6,
            Self::Stopped => 7,
            Self::Failed => 8,
        }
    }

    pub fn from_proto_i32(v: i32) -> Self {
        match v {
            1 => Self::Starting,
            2 => Self::Running,
            3 => Self::Idle,
            4 => Self::WaitingApproval,
            5 => Self::Paused,
            6 => Self::Stopping,
            7 => Self::Stopped,
            8 => Self::Failed,
            _ => Self::Unspecified,
        }
    }
}

/// Snapshot of an agent, returned to callers (no internal handles).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub id: String,
    pub workspace_id: String,
    pub state: AgentState,
    pub pane_id: String,
    pub pid: i32,
    pub command: String,
    pub adapter: String,
    pub spawned_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub content_hash: String,
}

/// Mutable agent info stored in the registry.
pub struct AgentInfo {
    pub id: String,
    pub workspace_id: String,
    pub state: AgentState,
    pub pane_id: String,
    pub pid: i32,
    pub command: String,
    pub adapter: String,
    pub spawned_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub content_hash: String,
    pub transcript: TranscriptStore,
}

impl AgentInfo {
    fn to_snapshot(&self) -> Agent {
        Agent {
            id: self.id.clone(),
            workspace_id: self.workspace_id.clone(),
            state: self.state,
            pane_id: self.pane_id.clone(),
            pid: self.pid,
            command: self.command.clone(),
            adapter: self.adapter.clone(),
            spawned_at: self.spawned_at,
            last_activity_at: self.last_activity_at,
            content_hash: self.content_hash.clone(),
        }
    }
}

/// Thread-safe agent registry.
#[derive(Clone)]
pub struct AgentManager {
    agents: Arc<RwLock<HashMap<String, AgentInfo>>>,
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentManager {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new agent. Returns the snapshot.
    /// Used by SpawnAgent RPC handler and tests.
    pub fn register(&self, info: AgentInfo) -> Agent {
        let snapshot = info.to_snapshot();
        let id = info.id.clone();
        let mut agents = write_agents(&self.agents);
        agents.insert(id, info);
        snapshot
    }

    /// Remove an agent by id. Returns the snapshot if it existed.
    pub fn remove(&self, agent_id: &str) -> Option<Agent> {
        let mut agents = write_agents(&self.agents);
        agents.remove(agent_id).map(|info| info.to_snapshot())
    }

    /// Get a single agent snapshot.
    pub fn get(&self, agent_id: &str) -> Option<Agent> {
        let agents = read_agents(&self.agents);
        agents.get(agent_id).map(|info| info.to_snapshot())
    }

    /// List agents with optional workspace and state filters.
    pub fn list(&self, workspace_id: Option<&str>, states: &[AgentState]) -> Vec<Agent> {
        let agents = read_agents(&self.agents);
        let mut result = Vec::new();
        for info in agents.values() {
            if let Some(ws) = workspace_id {
                if !ws.is_empty() && info.workspace_id != ws {
                    continue;
                }
            }
            if !states.is_empty() && !states.contains(&info.state) {
                continue;
            }
            result.push(info.to_snapshot());
        }
        result
    }

    /// Update last_activity_at for an agent.
    pub fn touch(&self, agent_id: &str) {
        let mut agents = write_agents(&self.agents);
        if let Some(info) = agents.get_mut(agent_id) {
            info.last_activity_at = Utc::now();
        }
    }

    /// Update pane snapshot metadata observed by capture/stream handlers.
    pub fn update_snapshot(&self, agent_id: &str, content_hash: String, state: Option<AgentState>) {
        let mut agents = write_agents(&self.agents);
        if let Some(info) = agents.get_mut(agent_id) {
            info.content_hash = content_hash;
            info.last_activity_at = Utc::now();
            if let Some(state) = state {
                info.state = state;
            }
        }
    }

    /// Record a transcript entry for an agent.
    pub fn add_transcript_entry(
        &self,
        agent_id: &str,
        entry_type: TranscriptEntryType,
        content: &str,
    ) {
        self.add_transcript_entry_full(
            agent_id,
            TranscriptEntry {
                entry_type,
                content: content.to_string(),
                timestamp: Utc::now(),
                metadata: HashMap::new(),
            },
        );
    }

    /// Record a transcript entry with explicit fields (used by transcript RPC and tests).
    pub fn add_transcript_entry_full(&self, agent_id: &str, entry: TranscriptEntry) {
        let mut agents = write_agents(&self.agents);
        if let Some(info) = agents.get_mut(agent_id) {
            info.transcript.add(entry);
        }
    }

    /// Return the number of managed agents.
    pub fn count(&self) -> usize {
        let agents = read_agents(&self.agents);
        agents.len()
    }

    /// Check if an agent exists.
    pub fn contains(&self, agent_id: &str) -> bool {
        let agents = read_agents(&self.agents);
        agents.contains_key(agent_id)
    }
}

fn read_agents(
    lock: &Arc<RwLock<HashMap<String, AgentInfo>>>,
) -> std::sync::RwLockReadGuard<'_, HashMap<String, AgentInfo>> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn write_agents(
    lock: &Arc<RwLock<HashMap<String, AgentInfo>>>,
) -> std::sync::RwLockWriteGuard<'_, HashMap<String, AgentInfo>> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_info(id: &str, workspace: &str, state: AgentState) -> AgentInfo {
        let now = Utc::now();
        AgentInfo {
            id: id.to_string(),
            workspace_id: workspace.to_string(),
            state,
            pane_id: format!("{id}:0.0"),
            pid: 1000,
            command: "claude".to_string(),
            adapter: "claude_code".to_string(),
            spawned_at: now,
            last_activity_at: now,
            content_hash: String::new(),
            transcript: TranscriptStore::new(),
        }
    }

    #[test]
    fn register_and_get() {
        let mgr = AgentManager::new();
        mgr.register(make_info("a1", "ws1", AgentState::Running));
        let agent = mgr.get("a1");
        assert!(agent.is_some());
        let agent = agent.unwrap_or_else(|| panic!("expected agent"));
        assert_eq!(agent.id, "a1");
        assert_eq!(agent.workspace_id, "ws1");
        assert_eq!(agent.state, AgentState::Running);
    }

    #[test]
    fn get_missing_returns_none() {
        let mgr = AgentManager::new();
        assert!(mgr.get("missing").is_none());
    }

    #[test]
    fn remove_returns_snapshot() {
        let mgr = AgentManager::new();
        mgr.register(make_info("a1", "ws1", AgentState::Running));
        let removed = mgr.remove("a1");
        assert!(removed.is_some());
        assert!(mgr.get("a1").is_none());
    }

    #[test]
    fn list_no_filter() {
        let mgr = AgentManager::new();
        mgr.register(make_info("a1", "ws1", AgentState::Running));
        mgr.register(make_info("a2", "ws2", AgentState::Idle));
        let all = mgr.list(None, &[]);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn list_workspace_filter() {
        let mgr = AgentManager::new();
        mgr.register(make_info("a1", "ws1", AgentState::Running));
        mgr.register(make_info("a2", "ws2", AgentState::Running));
        let ws1 = mgr.list(Some("ws1"), &[]);
        assert_eq!(ws1.len(), 1);
        assert_eq!(ws1[0].id, "a1");
    }

    #[test]
    fn list_state_filter() {
        let mgr = AgentManager::new();
        mgr.register(make_info("a1", "ws1", AgentState::Running));
        mgr.register(make_info("a2", "ws1", AgentState::Idle));
        mgr.register(make_info("a3", "ws1", AgentState::Stopped));
        let running = mgr.list(None, &[AgentState::Running, AgentState::Idle]);
        assert_eq!(running.len(), 2);
    }

    #[test]
    fn list_combined_filters() {
        let mgr = AgentManager::new();
        mgr.register(make_info("a1", "ws1", AgentState::Running));
        mgr.register(make_info("a2", "ws1", AgentState::Idle));
        mgr.register(make_info("a3", "ws2", AgentState::Running));
        let result = mgr.list(Some("ws1"), &[AgentState::Running]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "a1");
    }

    #[test]
    fn count_and_contains() {
        let mgr = AgentManager::new();
        assert_eq!(mgr.count(), 0);
        assert!(!mgr.contains("a1"));
        mgr.register(make_info("a1", "ws1", AgentState::Running));
        assert_eq!(mgr.count(), 1);
        assert!(mgr.contains("a1"));
    }
}
