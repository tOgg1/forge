//! Data polling pipeline primitives with bounded buffering and jittered cadence.

use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollingConfig {
    pub base_interval_ms: u64,
    pub max_jitter_ms: u64,
    pub max_pending_snapshots: usize,
    pub backpressure_step_ms: u64,
    pub max_backpressure_ms: u64,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            base_interval_ms: 2_000,
            max_jitter_ms: 250,
            max_pending_snapshots: 4,
            backpressure_step_ms: 400,
            max_backpressure_ms: 2_000,
        }
    }
}

impl PollingConfig {
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut config = self.clone();
        if config.base_interval_ms == 0 {
            config.base_interval_ms = 2_000;
        }
        if config.max_pending_snapshots == 0 {
            config.max_pending_snapshots = 1;
        }
        if config.backpressure_step_ms == 0 {
            config.backpressure_step_ms = 400;
        }
        if config.max_backpressure_ms < config.backpressure_step_ms {
            config.max_backpressure_ms = config.backpressure_step_ms;
        }
        config
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePressureEvent {
    Enqueued,
    DroppedOldest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollingQueue<T> {
    max_pending: usize,
    items: VecDeque<T>,
    dropped_total: u64,
    max_depth_seen: usize,
}

impl<T> PollingQueue<T> {
    #[must_use]
    pub fn new(max_pending: usize) -> Self {
        let max_pending = max_pending.max(1);
        Self {
            max_pending,
            items: VecDeque::new(),
            dropped_total: 0,
            max_depth_seen: 0,
        }
    }

    pub fn push(&mut self, item: T) -> QueuePressureEvent {
        let mut event = QueuePressureEvent::Enqueued;
        if self.items.len() >= self.max_pending {
            let _ = self.items.pop_front();
            self.dropped_total = self.dropped_total.saturating_add(1);
            event = QueuePressureEvent::DroppedOldest;
        }
        self.items.push_back(item);
        self.max_depth_seen = self.max_depth_seen.max(self.items.len());
        event
    }

    pub fn drain_latest(&mut self) -> Option<T> {
        let latest = self.items.pop_back();
        self.items.clear();
        latest
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn dropped_total(&self) -> u64 {
        self.dropped_total
    }

    #[must_use]
    pub fn max_depth_seen(&self) -> usize {
        self.max_depth_seen
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollScheduler {
    config: PollingConfig,
    seed: u64,
    tick: u64,
}

impl PollScheduler {
    #[must_use]
    pub fn new(config: PollingConfig, pipeline_key: &str) -> Self {
        Self {
            config: config.normalized(),
            seed: stable_seed(pipeline_key),
            tick: 0,
        }
    }

    #[must_use]
    pub fn config(&self) -> &PollingConfig {
        &self.config
    }

    #[must_use]
    pub fn next_interval(&mut self, backlog: usize) -> Duration {
        let jitter_ms = deterministic_jitter_ms(self.seed, self.tick, self.config.max_jitter_ms);
        self.tick = self.tick.saturating_add(1);

        let backlog_steps = backlog.saturating_sub(1) as u64;
        let backpressure_ms = backlog_steps
            .saturating_mul(self.config.backpressure_step_ms)
            .min(self.config.max_backpressure_ms);

        let interval_ms = self
            .config
            .base_interval_ms
            .saturating_add(jitter_ms)
            .saturating_add(backpressure_ms);

        Duration::from_millis(interval_ms)
    }
}

#[must_use]
pub fn deterministic_jitter_ms(seed: u64, tick: u64, max_jitter_ms: u64) -> u64 {
    if max_jitter_ms == 0 {
        return 0;
    }
    let mixed = splitmix64(seed ^ tick.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    if max_jitter_ms == u64::MAX {
        mixed
    } else {
        mixed % (max_jitter_ms + 1)
    }
}

fn stable_seed(key: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::{
        deterministic_jitter_ms, PollScheduler, PollingConfig, PollingQueue, QueuePressureEvent,
    };

    #[test]
    fn queue_drops_oldest_when_capacity_reached() {
        let mut queue = PollingQueue::new(2);
        assert_eq!(queue.push(1), QueuePressureEvent::Enqueued);
        assert_eq!(queue.push(2), QueuePressureEvent::Enqueued);
        assert_eq!(queue.push(3), QueuePressureEvent::DroppedOldest);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.dropped_total(), 1);
        assert_eq!(queue.max_depth_seen(), 2);
        assert_eq!(queue.drain_latest(), Some(3));
    }

    #[test]
    fn drain_latest_collapses_backlog() {
        let mut queue = PollingQueue::new(4);
        let _ = queue.push("a");
        let _ = queue.push("b");
        let _ = queue.push("c");
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.drain_latest(), Some("c"));
        assert_eq!(queue.len(), 0);
        assert_eq!(queue.drain_latest(), None);
    }

    #[test]
    fn scheduler_applies_backpressure_and_caps_penalty() {
        let config = PollingConfig {
            base_interval_ms: 2_000,
            max_jitter_ms: 0,
            max_pending_snapshots: 4,
            backpressure_step_ms: 500,
            max_backpressure_ms: 1_200,
        };
        let mut scheduler = PollScheduler::new(config, "worker-a");

        assert_eq!(scheduler.next_interval(0).as_millis(), 2_000);
        assert_eq!(scheduler.next_interval(2).as_millis(), 2_500);
        assert_eq!(scheduler.next_interval(5).as_millis(), 3_200);
    }

    #[test]
    fn jitter_sequence_is_deterministic_per_key() {
        let config = PollingConfig {
            max_jitter_ms: 250,
            ..PollingConfig::default()
        };
        let mut left = PollScheduler::new(config.clone(), "seed-a");
        let mut right = PollScheduler::new(config, "seed-a");
        let left_seq = (0..8)
            .map(|_| left.next_interval(0).as_millis())
            .collect::<Vec<_>>();
        let right_seq = (0..8)
            .map(|_| right.next_interval(0).as_millis())
            .collect::<Vec<_>>();
        assert_eq!(left_seq, right_seq);
    }

    #[test]
    fn jitter_stays_within_configured_bound() {
        let max_jitter_ms = 300;
        for tick in 0..64 {
            let jitter = deterministic_jitter_ms(42, tick, max_jitter_ms);
            assert!(jitter <= max_jitter_ms);
        }
    }

    #[test]
    fn normalized_config_enforces_sane_minimums() {
        let config = PollingConfig {
            base_interval_ms: 0,
            max_jitter_ms: 0,
            max_pending_snapshots: 0,
            backpressure_step_ms: 0,
            max_backpressure_ms: 0,
        }
        .normalized();

        assert_eq!(config.base_interval_ms, 2_000);
        assert_eq!(config.max_pending_snapshots, 1);
        assert_eq!(config.backpressure_step_ms, 400);
        assert_eq!(config.max_backpressure_ms, 400);
    }
}
