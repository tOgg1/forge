-- Migration: 011_loop_kv (DOWN)
-- Description: Remove per-loop key/value memory
-- Created: 2026-02-07

DROP TRIGGER IF EXISTS update_loop_kv_timestamp;
DROP INDEX IF EXISTS idx_loop_kv_loop_id;
DROP TABLE IF EXISTS loop_kv;

