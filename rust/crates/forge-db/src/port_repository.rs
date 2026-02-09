//! Port allocation repository — CRUD for the `port_allocations` table with full Go parity.

use std::collections::HashSet;

use rusqlite::{params, OptionalExtension};

use crate::{Db, DbError};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default start of the port range for OpenCode servers.
pub const DEFAULT_PORT_RANGE_START: i32 = 17000;

/// Default end of the port range for OpenCode servers.
pub const DEFAULT_PORT_RANGE_END: i32 = 17999;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// A port allocation record. Mirrors Go `PortAllocation`.
#[derive(Debug, Clone, Default)]
pub struct PortAllocation {
    pub id: i64,
    pub port: i32,
    pub node_id: String,
    pub agent_id: Option<String>,
    pub reason: String,
    pub allocated_at: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_rfc3339() -> String {
    let duration = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => std::time::Duration::from_secs(0),
    };
    let secs = duration.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_civil(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_civil(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn is_unique_constraint_error(err: &rusqlite::Error) -> bool {
    err.to_string().contains("UNIQUE constraint failed")
}

fn nullable_string(s: &str) -> Option<&str> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

// ---------------------------------------------------------------------------
// Row scanner
// ---------------------------------------------------------------------------

fn scan_allocation(row: &rusqlite::Row) -> rusqlite::Result<PortAllocation> {
    let id: i64 = row.get(0)?;
    let port: i32 = row.get(1)?;
    let node_id: String = row.get(2)?;
    let agent_id: Option<String> = row.get(3)?;
    let reason: Option<String> = row.get(4)?;
    let allocated_at: String = row.get(5)?;

    Ok(PortAllocation {
        id,
        port,
        node_id,
        agent_id,
        reason: reason.unwrap_or_default(),
        allocated_at,
    })
}

// ---------------------------------------------------------------------------
// PortRepository
// ---------------------------------------------------------------------------

/// Repository for port allocation persistence with Go-parity semantics.
pub struct PortRepository<'a> {
    db: &'a Db,
    range_start: i32,
    range_end: i32,
}

impl<'a> PortRepository<'a> {
    /// Creates a new PortRepository with default port range (17000-17999).
    pub fn new(db: &'a Db) -> Self {
        Self {
            db,
            range_start: DEFAULT_PORT_RANGE_START,
            range_end: DEFAULT_PORT_RANGE_END,
        }
    }

    /// Creates a new PortRepository with a custom port range.
    pub fn with_range(db: &'a Db, start: i32, end: i32) -> Self {
        Self {
            db,
            range_start: start,
            range_end: end,
        }
    }

    /// Allocate finds an available port for the given node and allocates it.
    /// Returns the allocated port number.
    pub fn allocate(&self, node_id: &str, agent_id: &str, reason: &str) -> Result<i32, DbError> {
        let port = self.find_available_port(node_id)?;

        let now = now_rfc3339();
        let agent_id_param = nullable_string(agent_id);

        self.db.conn().execute(
            "INSERT INTO port_allocations (port, node_id, agent_id, reason, allocated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![port, node_id, agent_id_param, reason, now],
        )?;

        Ok(port)
    }

    /// AllocateSpecific allocates a specific port for the given node.
    /// Returns an error if the port is already in use or outside the valid range.
    pub fn allocate_specific(
        &self,
        node_id: &str,
        port: i32,
        agent_id: &str,
        reason: &str,
    ) -> Result<(), DbError> {
        if port < self.range_start || port > self.range_end {
            return Err(DbError::Validation(format!(
                "port {} is outside valid range {}-{}",
                port, self.range_start, self.range_end
            )));
        }

        // Check if port is available
        let count: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM port_allocations WHERE node_id = ?1 AND port = ?2",
            params![node_id, port],
            |row| row.get(0),
        )?;

        if count > 0 {
            return Err(DbError::PortAlreadyAllocated);
        }

        let now = now_rfc3339();
        let agent_id_param = nullable_string(agent_id);

        let result = self.db.conn().execute(
            "INSERT INTO port_allocations (port, node_id, agent_id, reason, allocated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![port, node_id, agent_id_param, reason, now],
        );

        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                if is_unique_constraint_error(&err) {
                    Err(DbError::PortAlreadyAllocated)
                } else {
                    Err(DbError::Open(err))
                }
            }
        }
    }

    /// Release releases a port allocation by deleting it.
    pub fn release(&self, node_id: &str, port: i32) -> Result<(), DbError> {
        let rows_affected = self.db.conn().execute(
            "DELETE FROM port_allocations WHERE node_id = ?1 AND port = ?2",
            params![node_id, port],
        )?;

        if rows_affected == 0 {
            return Err(DbError::PortNotAllocated);
        }
        Ok(())
    }

    /// ReleaseByAgent releases all ports allocated to a specific agent.
    /// Returns the number of ports released.
    pub fn release_by_agent(&self, agent_id: &str) -> Result<i32, DbError> {
        let rows_affected = self.db.conn().execute(
            "DELETE FROM port_allocations WHERE agent_id = ?1",
            params![agent_id],
        )?;

        Ok(rows_affected as i32)
    }

    /// GetByAgent retrieves the most recent active port allocation for an agent.
    pub fn get_by_agent(&self, agent_id: &str) -> Result<PortAllocation, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, port, node_id, agent_id, reason, allocated_at
                 FROM port_allocations
                 WHERE agent_id = ?1
                 ORDER BY allocated_at DESC
                 LIMIT 1",
                params![agent_id],
                scan_allocation,
            )
            .optional()?;

        result.ok_or(DbError::PortNotAllocated)
    }

    /// GetByNodeAndPort retrieves an active port allocation by node and port.
    pub fn get_by_node_and_port(
        &self,
        node_id: &str,
        port: i32,
    ) -> Result<PortAllocation, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, port, node_id, agent_id, reason, allocated_at
                 FROM port_allocations
                 WHERE node_id = ?1 AND port = ?2",
                params![node_id, port],
                scan_allocation,
            )
            .optional()?;

        result.ok_or(DbError::PortNotAllocated)
    }

    /// ListActiveByNode retrieves all active port allocations for a node, ordered by port.
    pub fn list_active_by_node(&self, node_id: &str) -> Result<Vec<PortAllocation>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, port, node_id, agent_id, reason, allocated_at
             FROM port_allocations
             WHERE node_id = ?1
             ORDER BY port",
        )?;

        let rows = stmt.query_map(params![node_id], scan_allocation)?;

        let mut allocations = Vec::new();
        for row in rows {
            allocations.push(row?);
        }
        Ok(allocations)
    }

    /// CountActiveByNode returns the number of active port allocations for a node.
    pub fn count_active_by_node(&self, node_id: &str) -> Result<i32, DbError> {
        let count: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM port_allocations WHERE node_id = ?1",
            params![node_id],
            |row| row.get(0),
        )?;
        Ok(count as i32)
    }

    /// IsPortAvailable checks if a specific port is available on a node.
    pub fn is_port_available(&self, node_id: &str, port: i32) -> Result<bool, DbError> {
        let count: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM port_allocations WHERE node_id = ?1 AND port = ?2",
            params![node_id, port],
            |row| row.get(0),
        )?;
        Ok(count == 0)
    }

    /// CleanupExpired releases allocations for agents that no longer exist.
    /// Returns the number of allocations cleaned up.
    pub fn cleanup_expired(&self) -> Result<i32, DbError> {
        let rows_affected = self.db.conn().execute(
            "DELETE FROM port_allocations
             WHERE agent_id IS NOT NULL
             AND agent_id NOT IN (SELECT id FROM agents)",
            [],
        )?;

        Ok(rows_affected as i32)
    }

    /// Finds the first available port in the configured range for a node.
    fn find_available_port(&self, node_id: &str) -> Result<i32, DbError> {
        let mut stmt = self
            .db
            .conn()
            .prepare("SELECT port FROM port_allocations WHERE node_id = ?1 ORDER BY port")?;

        let rows = stmt.query_map(params![node_id], |row| row.get::<_, i32>(0))?;

        let mut allocated = HashSet::new();
        for row in rows {
            allocated.insert(row?);
        }

        for port in self.range_start..=self.range_end {
            if !allocated.contains(&port) {
                return Ok(port);
            }
        }

        Err(DbError::NoAvailablePorts)
    }
}
