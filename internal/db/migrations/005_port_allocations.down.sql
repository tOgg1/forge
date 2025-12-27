-- Migration: 005_port_allocations (down)
-- Description: Remove port allocation tracking

DROP INDEX IF EXISTS idx_port_allocations_active;
DROP INDEX IF EXISTS idx_port_allocations_agent;
DROP INDEX IF EXISTS idx_port_allocations_node_available;
DROP TABLE IF EXISTS port_allocations;
