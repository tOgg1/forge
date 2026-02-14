//! Team repository — CRUD for `teams` and `team_members`.

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamRole {
    Leader,
    Member,
}

impl TeamRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Leader => "leader",
            Self::Member => "member",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DbError> {
        match value {
            "leader" => Ok(Self::Leader),
            "member" => Ok(Self::Member),
            other => Err(DbError::Validation(format!("invalid team role: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub delegation_rules_json: String,
    pub default_assignee: String,
    pub heartbeat_interval_seconds: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TeamMember {
    pub id: String,
    pub team_id: String,
    pub agent_id: String,
    pub role: String,
    pub created_at: String,
}

fn validate_team(team: &Team) -> Result<(), DbError> {
    if team.name.trim().is_empty() {
        return Err(DbError::Validation("team name is required".to_owned()));
    }
    if team.heartbeat_interval_seconds <= 0 {
        return Err(DbError::Validation(
            "heartbeat interval must be > 0".to_owned(),
        ));
    }
    if !team.default_assignee.trim().is_empty()
        && team.default_assignee.contains(char::is_whitespace)
    {
        return Err(DbError::Validation(
            "default assignee must not contain whitespace".to_owned(),
        ));
    }
    if !team.delegation_rules_json.trim().is_empty() {
        let parsed = serde_json::from_str::<serde_json::Value>(&team.delegation_rules_json)
            .map_err(|err| DbError::Validation(format!("invalid delegation rules json: {err}")))?;
        if !parsed.is_object() {
            return Err(DbError::Validation(
                "delegation rules must be a JSON object".to_owned(),
            ));
        }
    }
    Ok(())
}

fn normalize_delegation_rules_json(raw: &str) -> Result<String, DbError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let parsed = serde_json::from_str::<serde_json::Value>(trimmed)
        .map_err(|err| DbError::Validation(format!("invalid delegation rules json: {err}")))?;
    if !parsed.is_object() {
        return Err(DbError::Validation(
            "delegation rules must be a JSON object".to_owned(),
        ));
    }
    serde_json::to_string(&parsed)
        .map_err(|err| DbError::Validation(format!("serialize delegation rules: {err}")))
}

fn validate_member(member: &TeamMember) -> Result<(), DbError> {
    if member.team_id.trim().is_empty() {
        return Err(DbError::Validation("team_id is required".to_owned()));
    }
    if member.agent_id.trim().is_empty() {
        return Err(DbError::Validation("agent_id is required".to_owned()));
    }
    TeamRole::parse(member.role.trim())?;
    Ok(())
}

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

fn is_fk_constraint_error(err: &rusqlite::Error) -> bool {
    err.to_string().contains("FOREIGN KEY constraint failed")
}

pub struct TeamRepository<'a> {
    db: &'a Db,
}

impl<'a> TeamRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    pub fn create_team(&self, team: &mut Team) -> Result<(), DbError> {
        if team.id.trim().is_empty() {
            team.id = Uuid::new_v4().to_string();
        }
        team.name = team.name.trim().to_owned();
        team.default_assignee = team.default_assignee.trim().to_owned();
        team.delegation_rules_json = normalize_delegation_rules_json(&team.delegation_rules_json)?;
        validate_team(team)?;

        let now = now_rfc3339();
        team.created_at = now.clone();
        team.updated_at = now;

        let delegation_rules_json = nullable_string(&team.delegation_rules_json);
        let default_assignee = nullable_string(&team.default_assignee);

        let result = self.db.conn().execute(
            "INSERT INTO teams (
                id, name, delegation_rules_json, default_assignee, heartbeat_interval_seconds, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                team.id,
                team.name,
                delegation_rules_json,
                default_assignee,
                team.heartbeat_interval_seconds,
                team.created_at,
                team.updated_at,
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                if is_unique_constraint_error(&err) {
                    Err(DbError::TeamAlreadyExists)
                } else {
                    Err(DbError::Open(err))
                }
            }
        }
    }

    pub fn get_team(&self, id: &str) -> Result<Team, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, name, delegation_rules_json, default_assignee, heartbeat_interval_seconds, created_at, updated_at
                 FROM teams
                 WHERE id = ?1",
                params![id],
                scan_team,
            )
            .optional()?;

        result.ok_or(DbError::TeamNotFound)
    }

    pub fn get_team_by_name(&self, name: &str) -> Result<Team, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, name, delegation_rules_json, default_assignee, heartbeat_interval_seconds, created_at, updated_at
                 FROM teams
                 WHERE name = ?1",
                params![name],
                scan_team,
            )
            .optional()?;
        result.ok_or(DbError::TeamNotFound)
    }

    pub fn list_teams(&self) -> Result<Vec<Team>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, name, delegation_rules_json, default_assignee, heartbeat_interval_seconds, created_at, updated_at
             FROM teams
             ORDER BY name",
        )?;

        let rows = stmt.query_map([], scan_team)?;
        let mut teams = Vec::new();
        for row in rows {
            teams.push(row?);
        }
        Ok(teams)
    }

    pub fn update_team(&self, team: &mut Team) -> Result<(), DbError> {
        if team.id.trim().is_empty() {
            return Err(DbError::Validation("team id is required".to_owned()));
        }
        team.name = team.name.trim().to_owned();
        team.default_assignee = team.default_assignee.trim().to_owned();
        team.delegation_rules_json = normalize_delegation_rules_json(&team.delegation_rules_json)?;
        validate_team(team)?;
        team.updated_at = now_rfc3339();

        let result = self.db.conn().execute(
            "UPDATE teams
             SET name = ?1, delegation_rules_json = ?2, default_assignee = ?3, heartbeat_interval_seconds = ?4, updated_at = ?5
             WHERE id = ?6",
            params![
                team.name,
                nullable_string(&team.delegation_rules_json),
                nullable_string(&team.default_assignee),
                team.heartbeat_interval_seconds,
                team.updated_at,
                team.id,
            ],
        );

        match result {
            Ok(0) => Err(DbError::TeamNotFound),
            Ok(_) => Ok(()),
            Err(err) => {
                if is_unique_constraint_error(&err) {
                    Err(DbError::TeamAlreadyExists)
                } else {
                    Err(DbError::Open(err))
                }
            }
        }
    }

    pub fn delete_team(&self, id: &str) -> Result<(), DbError> {
        let rows = self
            .db
            .conn()
            .execute("DELETE FROM teams WHERE id = ?1", params![id])?;
        if rows == 0 {
            return Err(DbError::TeamNotFound);
        }
        Ok(())
    }

    pub fn add_member(&self, member: &mut TeamMember) -> Result<(), DbError> {
        if member.id.trim().is_empty() {
            member.id = Uuid::new_v4().to_string();
        }
        member.team_id = member.team_id.trim().to_owned();
        member.agent_id = member.agent_id.trim().to_owned();
        member.role = TeamRole::parse(member.role.trim())?.as_str().to_owned();
        if member.created_at.trim().is_empty() {
            member.created_at = now_rfc3339();
        }
        validate_member(member)?;

        let result = self.db.conn().execute(
            "INSERT INTO team_members (id, team_id, agent_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                member.id,
                member.team_id,
                member.agent_id,
                member.role,
                member.created_at,
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                if is_unique_constraint_error(&err) {
                    Err(DbError::TeamMemberAlreadyExists)
                } else if is_fk_constraint_error(&err) {
                    Err(DbError::TeamNotFound)
                } else {
                    Err(DbError::Open(err))
                }
            }
        }
    }

    pub fn remove_member(&self, team_id: &str, agent_id: &str) -> Result<(), DbError> {
        let rows = self.db.conn().execute(
            "DELETE FROM team_members WHERE team_id = ?1 AND agent_id = ?2",
            params![team_id, agent_id],
        )?;
        if rows == 0 {
            return Err(DbError::TeamMemberNotFound);
        }
        Ok(())
    }

    pub fn list_members(&self, team_id: &str) -> Result<Vec<TeamMember>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, team_id, agent_id, role, created_at
             FROM team_members
             WHERE team_id = ?1
             ORDER BY CASE role WHEN 'leader' THEN 0 ELSE 1 END, created_at, agent_id",
        )?;

        let rows = stmt.query_map(params![team_id], scan_team_member)?;
        let mut members = Vec::new();
        for row in rows {
            members.push(row?);
        }
        Ok(members)
    }
}

pub struct TeamService<'a> {
    repo: TeamRepository<'a>,
}

impl<'a> TeamService<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self {
            repo: TeamRepository::new(db),
        }
    }

    pub fn create_team(
        &self,
        name: &str,
        delegation_rules_json: &str,
        default_assignee: &str,
        heartbeat_interval_seconds: i64,
    ) -> Result<Team, DbError> {
        let mut team = Team {
            id: String::new(),
            name: name.to_owned(),
            delegation_rules_json: delegation_rules_json.to_owned(),
            default_assignee: default_assignee.to_owned(),
            heartbeat_interval_seconds,
            created_at: String::new(),
            updated_at: String::new(),
        };
        self.repo.create_team(&mut team)?;
        Ok(team)
    }

    pub fn list_teams(&self) -> Result<Vec<Team>, DbError> {
        self.repo.list_teams()
    }

    pub fn show_team(&self, reference: &str) -> Result<Team, DbError> {
        if let Ok(team) = self.repo.get_team(reference) {
            return Ok(team);
        }
        self.repo.get_team_by_name(reference)
    }

    pub fn delete_team(&self, reference: &str) -> Result<(), DbError> {
        let team = self.show_team(reference)?;
        self.repo.delete_team(&team.id)
    }

    pub fn add_member(
        &self,
        team_reference: &str,
        agent_id: &str,
        role: TeamRole,
    ) -> Result<TeamMember, DbError> {
        let team = self.show_team(team_reference)?;
        let mut member = TeamMember {
            id: String::new(),
            team_id: team.id,
            agent_id: agent_id.to_owned(),
            role: role.as_str().to_owned(),
            created_at: String::new(),
        };
        self.repo.add_member(&mut member)?;
        Ok(member)
    }

    pub fn list_members(&self, team_reference: &str) -> Result<Vec<TeamMember>, DbError> {
        let team = self.show_team(team_reference)?;
        self.repo.list_members(&team.id)
    }
}

fn nullable_string(value: &str) -> Option<&str> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn scan_team(row: &rusqlite::Row<'_>) -> rusqlite::Result<Team> {
    Ok(Team {
        id: row.get(0)?,
        name: row.get(1)?,
        delegation_rules_json: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
        default_assignee: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        heartbeat_interval_seconds: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn scan_team_member(row: &rusqlite::Row<'_>) -> rusqlite::Result<TeamMember> {
    Ok(TeamMember {
        id: row.get(0)?,
        team_id: row.get(1)?,
        agent_id: row.get(2)?,
        role: row.get(3)?,
        created_at: row.get(4)?,
    })
}
