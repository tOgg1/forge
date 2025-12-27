-- Migration: 005_port_allocations
-- Description: Port allocation tracking for OpenCode server instances
-- Created: 2025-12-27

-- ============================================================================
-- PORT_ALLOCATIONS TABLE
-- ============================================================================
-- Tracks port allocations for OpenCode servers to prevent conflicts.
-- Each agent running OpenCode gets a dedicated port from a pool (17000-17999).

CREATE TABLE IF NOT EXISTS port_allocations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    
    -- The allocated port number (unique per node)
    port INTEGER NOT NULL,
    
    -- The node this port is allocated on (ports are node-local)
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    
    -- The agent using this port (nullable - port can be reserved but unassigned)
    agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
    
    -- Human-readable reason for allocation
    reason TEXT,
    
    -- When the allocation was created
    allocated_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    -- When the port was released (null if still in use)
    released_at TEXT,
    
    -- Unique constraint: only one active allocation per port per node
    UNIQUE(node_id, port)
);

-- Index for finding available ports on a node
CREATE INDEX IF NOT EXISTS idx_port_allocations_node_available 
    ON port_allocations(node_id, released_at);

-- Index for finding ports by agent
CREATE INDEX IF NOT EXISTS idx_port_allocations_agent 
    ON port_allocations(agent_id);

-- Index for finding active allocations
CREATE INDEX IF NOT EXISTS idx_port_allocations_active 
    ON port_allocations(node_id) WHERE released_at IS NULL;
