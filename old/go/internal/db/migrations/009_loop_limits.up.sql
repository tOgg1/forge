-- Migration: 009_loop_limits
-- Description: Add max iteration/runtime limits to loops
-- Created: 2026-01-08

ALTER TABLE loops ADD COLUMN max_iterations INTEGER NOT NULL DEFAULT 0;
ALTER TABLE loops ADD COLUMN max_runtime_seconds INTEGER NOT NULL DEFAULT 0;
