-- Migration: 014_team_model (rollback)
-- Description: remove team model + members + delegation config

DROP INDEX IF EXISTS idx_team_members_role;
DROP INDEX IF EXISTS idx_team_members_agent;
DROP INDEX IF EXISTS idx_team_members_team;

DROP TABLE IF EXISTS team_members;

DROP TRIGGER IF EXISTS update_teams_timestamp;

DROP INDEX IF EXISTS idx_teams_default_assignee;
DROP INDEX IF EXISTS idx_teams_name;

DROP TABLE IF EXISTS teams;
