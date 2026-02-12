-- Migration: 013_persistent_agents
-- Description: Schema for persistent agent lifecycle + events
-- Created: 2026-02-12

-- ============================================================================
-- PERSISTENT_AGENTS TABLE
-- ============================================================================
-- Stores first-class persistent agent records used by the M10 lifecycle
-- system. Separate from the existing `agents` table (001) which is tied
-- to forged daemon/tmux pane primitives.
CREATE TABLE IF NOT EXISTS persistent_agents (
    id TEXT PRIMARY KEY,
    parent_agent_id TEXT,
    workspace_id TEXT NOT NULL,
    repo TEXT,
    node TEXT,
    harness TEXT NOT NULL,
    mode TEXT NOT NULL CHECK (mode IN ('continuous', 'one-shot')),
    state TEXT NOT NULL DEFAULT 'starting' CHECK (state IN (
        'unspecified', 'starting', 'running', 'idle',
        'waiting_approval', 'paused', 'stopping', 'stopped', 'failed'
    )),
    ttl_seconds INTEGER,
    labels_json TEXT,
    tags_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_activity_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_persistent_agents_workspace ON persistent_agents(workspace_id);
CREATE INDEX IF NOT EXISTS idx_persistent_agents_state ON persistent_agents(state);
CREATE INDEX IF NOT EXISTS idx_persistent_agents_parent ON persistent_agents(parent_agent_id);
CREATE INDEX IF NOT EXISTS idx_persistent_agents_updated ON persistent_agents(updated_at);
CREATE INDEX IF NOT EXISTS idx_persistent_agents_harness ON persistent_agents(harness);

CREATE TRIGGER IF NOT EXISTS update_persistent_agents_timestamp
AFTER UPDATE ON persistent_agents
BEGIN
    UPDATE persistent_agents SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = NEW.id;
END;

-- ============================================================================
-- PERSISTENT_AGENT_EVENTS TABLE
-- ============================================================================
-- Append-only audit log for persistent agent operations.
CREATE TABLE IF NOT EXISTS persistent_agent_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT,
    kind TEXT NOT NULL,
    outcome TEXT NOT NULL,
    detail TEXT,
    timestamp TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_persistent_agent_events_agent ON persistent_agent_events(agent_id);
CREATE INDEX IF NOT EXISTS idx_persistent_agent_events_kind ON persistent_agent_events(kind);
CREATE INDEX IF NOT EXISTS idx_persistent_agent_events_timestamp ON persistent_agent_events(timestamp);
CREATE INDEX IF NOT EXISTS idx_persistent_agent_events_agent_ts ON persistent_agent_events(agent_id, timestamp);
