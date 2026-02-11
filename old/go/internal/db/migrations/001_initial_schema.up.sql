-- Migration: 001_initial_schema
-- Description: Initial schema with all core tables
-- Created: 2025-12-22

-- Enable foreign keys
PRAGMA foreign_keys = ON;

-- ============================================================================
-- NODES TABLE
-- ============================================================================
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    ssh_target TEXT,
    ssh_backend TEXT NOT NULL DEFAULT 'auto' CHECK (ssh_backend IN ('native', 'system', 'auto')),
    ssh_key_path TEXT,
    status TEXT NOT NULL DEFAULT 'unknown' CHECK (status IN ('online', 'offline', 'unknown')),
    is_local INTEGER NOT NULL DEFAULT 0,
    last_seen_at TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_nodes_name ON nodes(name);
CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes(status);

-- ============================================================================
-- WORKSPACES TABLE
-- ============================================================================
CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    repo_path TEXT NOT NULL,
    tmux_session TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive', 'error')),
    git_info_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(node_id, repo_path),
    UNIQUE(node_id, tmux_session)
);

CREATE INDEX IF NOT EXISTS idx_workspaces_node_id ON workspaces(node_id);
CREATE INDEX IF NOT EXISTS idx_workspaces_status ON workspaces(status);
CREATE INDEX IF NOT EXISTS idx_workspaces_name ON workspaces(name);

-- ============================================================================
-- ACCOUNTS TABLE
-- ============================================================================
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL CHECK (provider IN ('anthropic', 'openai', 'google', 'custom')),
    profile_name TEXT NOT NULL,
    credential_ref TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    cooldown_until TEXT,
    usage_stats_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(provider, profile_name)
);

CREATE INDEX IF NOT EXISTS idx_accounts_provider ON accounts(provider);
CREATE INDEX IF NOT EXISTS idx_accounts_is_active ON accounts(is_active);
CREATE INDEX IF NOT EXISTS idx_accounts_cooldown ON accounts(cooldown_until);

-- ============================================================================
-- AGENTS TABLE
-- ============================================================================
CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    type TEXT NOT NULL CHECK (type IN ('opencode', 'claude-code', 'codex', 'gemini', 'generic')),
    tmux_pane TEXT NOT NULL,
    account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
    state TEXT NOT NULL DEFAULT 'starting' CHECK (state IN ('working', 'idle', 'awaiting_approval', 'rate_limited', 'error', 'paused', 'starting', 'stopped')),
    state_confidence TEXT NOT NULL DEFAULT 'low' CHECK (state_confidence IN ('high', 'medium', 'low')),
    state_reason TEXT,
    state_detected_at TEXT,
    paused_until TEXT,
    last_activity_at TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(workspace_id, tmux_pane)
);

CREATE INDEX IF NOT EXISTS idx_agents_workspace_id ON agents(workspace_id);
CREATE INDEX IF NOT EXISTS idx_agents_state ON agents(state);
CREATE INDEX IF NOT EXISTS idx_agents_account_id ON agents(account_id);
CREATE INDEX IF NOT EXISTS idx_agents_type ON agents(type);

-- ============================================================================
-- QUEUE_ITEMS TABLE
-- ============================================================================
CREATE TABLE IF NOT EXISTS queue_items (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    type TEXT NOT NULL CHECK (type IN ('message', 'pause', 'conditional')),
    position INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'dispatched', 'completed', 'failed', 'skipped')),
    payload_json TEXT NOT NULL,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    dispatched_at TEXT,
    completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_queue_items_agent_id ON queue_items(agent_id);
CREATE INDEX IF NOT EXISTS idx_queue_items_status ON queue_items(status);
CREATE INDEX IF NOT EXISTS idx_queue_items_position ON queue_items(agent_id, position);

-- ============================================================================
-- EVENTS TABLE
-- ============================================================================
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    type TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('node', 'workspace', 'agent', 'queue', 'account', 'system')),
    entity_id TEXT NOT NULL,
    payload_json TEXT,
    metadata_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(type);
CREATE INDEX IF NOT EXISTS idx_events_entity ON events(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_events_entity_timestamp ON events(entity_type, entity_id, timestamp);

-- ============================================================================
-- ALERTS TABLE
-- ============================================================================
CREATE TABLE IF NOT EXISTS alerts (
    id TEXT PRIMARY KEY,
    workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
    agent_id TEXT REFERENCES agents(id) ON DELETE CASCADE,
    type TEXT NOT NULL CHECK (type IN ('approval_needed', 'cooldown', 'error', 'rate_limit')),
    severity TEXT NOT NULL DEFAULT 'warning' CHECK (severity IN ('info', 'warning', 'error', 'critical')),
    message TEXT NOT NULL,
    is_resolved INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_alerts_workspace_id ON alerts(workspace_id);
CREATE INDEX IF NOT EXISTS idx_alerts_agent_id ON alerts(agent_id);
CREATE INDEX IF NOT EXISTS idx_alerts_is_resolved ON alerts(is_resolved);
CREATE INDEX IF NOT EXISTS idx_alerts_severity ON alerts(severity);

-- ============================================================================
-- TRANSCRIPTS TABLE
-- ============================================================================
CREATE TABLE IF NOT EXISTS transcripts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    captured_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_transcripts_agent_id ON transcripts(agent_id);
CREATE INDEX IF NOT EXISTS idx_transcripts_captured_at ON transcripts(agent_id, captured_at);
CREATE INDEX IF NOT EXISTS idx_transcripts_hash ON transcripts(agent_id, content_hash);

-- ============================================================================
-- APPROVALS TABLE
-- ============================================================================
CREATE TABLE IF NOT EXISTS approvals (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    request_type TEXT NOT NULL,
    request_details_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'denied', 'expired')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT,
    resolved_by TEXT
);

CREATE INDEX IF NOT EXISTS idx_approvals_agent_id ON approvals(agent_id);
CREATE INDEX IF NOT EXISTS idx_approvals_status ON approvals(status);

-- ============================================================================
-- TRIGGERS
-- ============================================================================
CREATE TRIGGER IF NOT EXISTS update_nodes_timestamp 
AFTER UPDATE ON nodes
BEGIN
    UPDATE nodes SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_workspaces_timestamp 
AFTER UPDATE ON workspaces
BEGIN
    UPDATE workspaces SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_agents_timestamp 
AFTER UPDATE ON agents
BEGIN
    UPDATE agents SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_accounts_timestamp 
AFTER UPDATE ON accounts
BEGIN
    UPDATE accounts SET updated_at = datetime('now') WHERE id = NEW.id;
END;
