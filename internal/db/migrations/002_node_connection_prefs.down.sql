-- Migration: 002_node_connection_prefs (DOWN)
-- Description: Remove node connection preference columns
-- Created: 2025-12-22

-- SQLite does not support DROP COLUMN; rebuild the table without the new columns.
CREATE TABLE nodes_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    ssh_target TEXT,
    ssh_backend TEXT NOT NULL DEFAULT 'auto' CHECK (ssh_backend IN ('native', 'system', 'auto')),
    ssh_key_path TEXT,
    status TEXT NOT NULL DEFAULT 'unknown' CHECK (status IN ('online', 'offline', 'unknown')),
    is_local INTEGER NOT NULL DEFAULT 0,
    last_seen_at TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO nodes_new (
    id, name, ssh_target, ssh_backend, ssh_key_path, status,
    is_local, last_seen_at, metadata_json, created_at, updated_at
)
SELECT
    id, name, ssh_target, ssh_backend, ssh_key_path, status,
    is_local, last_seen_at, metadata_json, created_at, updated_at
FROM nodes;

DROP TABLE nodes;
ALTER TABLE nodes_new RENAME TO nodes;

CREATE INDEX IF NOT EXISTS idx_nodes_name ON nodes(name);
CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes(status);

CREATE TRIGGER IF NOT EXISTS update_nodes_timestamp
AFTER UPDATE ON nodes
BEGIN
    UPDATE nodes SET updated_at = datetime('now') WHERE id = NEW.id;
END;
