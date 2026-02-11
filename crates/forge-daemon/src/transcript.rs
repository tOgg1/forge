//! In-memory transcript storage per agent.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

/// Entry types matching proto `TranscriptEntryType` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptEntryType {
    Command,
    Output,
    Error,
    StateChange,
    Approval,
    UserInput,
}

impl TranscriptEntryType {
    pub fn to_proto_i32(self) -> i32 {
        match self {
            Self::Command => 1,
            Self::Output => 2,
            Self::Error => 3,
            Self::StateChange => 4,
            Self::Approval => 5,
            Self::UserInput => 6,
        }
    }

    pub fn from_proto_i32(v: i32) -> Option<Self> {
        match v {
            1 => Some(Self::Command),
            2 => Some(Self::Output),
            3 => Some(Self::Error),
            4 => Some(Self::StateChange),
            5 => Some(Self::Approval),
            6 => Some(Self::UserInput),
            _ => None,
        }
    }
}

/// A single transcript entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptEntry {
    pub entry_type: TranscriptEntryType,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

/// Append-only transcript store with monotonic cursor IDs.
pub struct TranscriptStore {
    entries: Vec<TranscriptEntry>,
    next_id: i64,
}

impl TranscriptStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(100),
            next_id: 0,
        }
    }

    pub fn add(&mut self, entry: TranscriptEntry) {
        self.entries.push(entry);
        self.next_id += 1;
    }

    pub fn entries(&self) -> &[TranscriptEntry] {
        &self.entries
    }

    pub fn next_id(&self) -> i64 {
        self.next_id
    }
}

impl Default for TranscriptStore {
    fn default() -> Self {
        Self::new()
    }
}
