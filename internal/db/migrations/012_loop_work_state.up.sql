-- Migration: 012_loop_work_state
-- Description: Persist loop work context (task pointer + status)
-- Created: 2026-02-07

CREATE TABLE IF NOT EXISTS loop_work_state (
    id TEXT PRIMARY KEY,
    loop_id TEXT NOT NULL REFERENCES loops(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    status TEXT NOT NULL,
    detail TEXT,
    loop_iteration INTEGER NOT NULL DEFAULT 0,
    is_current INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(loop_id, task_id)
);

CREATE INDEX IF NOT EXISTS idx_loop_work_state_loop_id ON loop_work_state(loop_id);
CREATE INDEX IF NOT EXISTS idx_loop_work_state_loop_current ON loop_work_state(loop_id, is_current);
CREATE INDEX IF NOT EXISTS idx_loop_work_state_loop_updated ON loop_work_state(loop_id, updated_at);

CREATE TRIGGER IF NOT EXISTS update_loop_work_state_timestamp
AFTER UPDATE ON loop_work_state
BEGIN
    UPDATE loop_work_state SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = NEW.id;
END;

