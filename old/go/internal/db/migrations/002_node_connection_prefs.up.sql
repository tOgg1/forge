-- Migration: 002_node_connection_prefs (UP)
-- Description: Add node connection preference columns
-- Created: 2025-12-22

ALTER TABLE nodes ADD COLUMN ssh_agent_forwarding INTEGER NOT NULL DEFAULT 0;
ALTER TABLE nodes ADD COLUMN ssh_proxy_jump TEXT;
ALTER TABLE nodes ADD COLUMN ssh_control_master TEXT;
ALTER TABLE nodes ADD COLUMN ssh_control_path TEXT;
ALTER TABLE nodes ADD COLUMN ssh_control_persist TEXT;
ALTER TABLE nodes ADD COLUMN ssh_timeout_seconds INTEGER;
