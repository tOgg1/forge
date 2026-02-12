-- Migration: 013_persistent_agents (rollback)
-- Description: Remove persistent agent lifecycle + events tables

DROP TRIGGER IF EXISTS update_persistent_agents_timestamp;

DROP INDEX IF EXISTS idx_persistent_agent_events_agent_ts;
DROP INDEX IF EXISTS idx_persistent_agent_events_timestamp;
DROP INDEX IF EXISTS idx_persistent_agent_events_kind;
DROP INDEX IF EXISTS idx_persistent_agent_events_agent;

DROP TABLE IF EXISTS persistent_agent_events;

DROP INDEX IF EXISTS idx_persistent_agents_harness;
DROP INDEX IF EXISTS idx_persistent_agents_updated;
DROP INDEX IF EXISTS idx_persistent_agents_parent;
DROP INDEX IF EXISTS idx_persistent_agents_state;
DROP INDEX IF EXISTS idx_persistent_agents_workspace;

DROP TABLE IF EXISTS persistent_agents;
