//! Queue model types for loop and agent messaging.
//!
//! Mirrors Go `internal/models/queue.go` and `internal/models/loop_queue.go`.

use std::fmt;

/// Type of a loop queue item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoopQueueItemType {
    Message,
    NextPromptOverride,
    Pause,
    StopGraceful,
    KillNow,
    SteerMessage,
}

impl fmt::Display for LoopQueueItemType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Message => "message_append",
            Self::NextPromptOverride => "next_prompt_override",
            Self::Pause => "pause",
            Self::StopGraceful => "stop_graceful",
            Self::KillNow => "kill_now",
            Self::SteerMessage => "steer_message",
        };
        f.write_str(s)
    }
}

/// Processing status of a loop queue item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoopQueueItemStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

/// Type of an agent queue item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueueItemType {
    Message,
    Pause,
    Conditional,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_queue_item_type_display() {
        assert_eq!(LoopQueueItemType::Message.to_string(), "message_append");
        assert_eq!(LoopQueueItemType::KillNow.to_string(), "kill_now");
        assert_eq!(LoopQueueItemType::StopGraceful.to_string(), "stop_graceful");
    }

    #[test]
    fn queue_item_status_variants() {
        let statuses = [
            LoopQueueItemStatus::Pending,
            LoopQueueItemStatus::Processing,
            LoopQueueItemStatus::Completed,
            LoopQueueItemStatus::Failed,
        ];
        // Ensure all variants are distinct.
        for (i, a) in statuses.iter().enumerate() {
            for (j, b) in statuses.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }
}
