-- Migration: 001_initial_schema (DOWN)
-- Description: Rollback initial schema
-- Created: 2025-12-22

-- Drop triggers first
DROP TRIGGER IF EXISTS update_accounts_timestamp;
DROP TRIGGER IF EXISTS update_agents_timestamp;
DROP TRIGGER IF EXISTS update_workspaces_timestamp;
DROP TRIGGER IF EXISTS update_nodes_timestamp;

-- Drop tables in reverse dependency order
DROP TABLE IF EXISTS approvals;
DROP TABLE IF EXISTS transcripts;
DROP TABLE IF EXISTS alerts;
DROP TABLE IF EXISTS events;
DROP TABLE IF EXISTS queue_items;
DROP TABLE IF EXISTS agents;
DROP TABLE IF EXISTS accounts;
DROP TABLE IF EXISTS workspaces;
DROP TABLE IF EXISTS nodes;
