-- Migration: 012_loop_work_state (DOWN)
-- Description: Remove loop work context
-- Created: 2026-02-07

DROP TRIGGER IF EXISTS update_loop_work_state_timestamp;
DROP INDEX IF EXISTS idx_loop_work_state_loop_updated;
DROP INDEX IF EXISTS idx_loop_work_state_loop_current;
DROP INDEX IF EXISTS idx_loop_work_state_loop_id;
DROP TABLE IF EXISTS loop_work_state;

