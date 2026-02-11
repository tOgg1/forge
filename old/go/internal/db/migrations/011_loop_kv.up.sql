-- Migration: 011_loop_kv
-- Description: Generic per-loop key/value memory (prompt-injected)
-- Created: 2026-02-07

CREATE TABLE IF NOT EXISTS loop_kv (
    id TEXT PRIMARY KEY,
    loop_id TEXT NOT NULL REFERENCES loops(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(loop_id, key)
);

CREATE INDEX IF NOT EXISTS idx_loop_kv_loop_id ON loop_kv(loop_id);

CREATE TRIGGER IF NOT EXISTS update_loop_kv_timestamp
AFTER UPDATE ON loop_kv
BEGIN
    UPDATE loop_kv SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = NEW.id;
END;

