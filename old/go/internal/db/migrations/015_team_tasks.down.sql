-- Migration: 015_team_tasks (rollback)
-- Description: remove team task inbox + assignment lifecycle schema

DROP INDEX IF EXISTS idx_team_task_events_team_created;
DROP INDEX IF EXISTS idx_team_task_events_task_created;

DROP TABLE IF EXISTS team_task_events;

DROP TRIGGER IF EXISTS update_team_tasks_timestamp;

DROP INDEX IF EXISTS idx_team_tasks_updated;
DROP INDEX IF EXISTS idx_team_tasks_assigned_agent;
DROP INDEX IF EXISTS idx_team_tasks_team_status_priority;

DROP TABLE IF EXISTS team_tasks;
