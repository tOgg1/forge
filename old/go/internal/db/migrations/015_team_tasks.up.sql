-- Migration: 015_team_tasks
-- Description: team task inbox + assignment lifecycle
-- Created: 2026-02-13

CREATE TABLE IF NOT EXISTS team_tasks (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN (
        'queued',
        'assigned',
        'running',
        'blocked',
        'done',
        'failed',
        'canceled'
    )),
    priority INTEGER NOT NULL DEFAULT 100,
    assigned_agent_id TEXT,
    submitted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    assigned_at TEXT,
    started_at TEXT,
    finished_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY(team_id) REFERENCES teams(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_team_tasks_team_status_priority
    ON team_tasks(team_id, status, priority, submitted_at);
CREATE INDEX IF NOT EXISTS idx_team_tasks_assigned_agent
    ON team_tasks(assigned_agent_id, status);
CREATE INDEX IF NOT EXISTS idx_team_tasks_updated
    ON team_tasks(updated_at);

CREATE TRIGGER IF NOT EXISTS update_team_tasks_timestamp
AFTER UPDATE ON team_tasks
BEGIN
    UPDATE team_tasks SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = NEW.id;
END;

CREATE TABLE IF NOT EXISTS team_task_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL,
    team_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN (
        'submitted',
        'assigned',
        'reassigned',
        'started',
        'blocked',
        'completed',
        'failed',
        'canceled'
    )),
    from_status TEXT,
    to_status TEXT,
    actor_agent_id TEXT,
    detail TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY(task_id) REFERENCES team_tasks(id) ON DELETE CASCADE,
    FOREIGN KEY(team_id) REFERENCES teams(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_team_task_events_task_created
    ON team_task_events(task_id, created_at);
CREATE INDEX IF NOT EXISTS idx_team_task_events_team_created
    ON team_task_events(team_id, created_at);
