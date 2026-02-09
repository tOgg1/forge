//! Event streaming infrastructure for ForgedService StreamEvents RPC.
//!
//! Provides cursor-based replay and real-time event streaming with parity to
//! Go daemon (`internal/forged/server.go`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use tokio::sync::mpsc;

use forge_rpc::forged::v1 as proto;

/// Maximum number of events kept in the circular buffer for replay.
const MAX_STORED_EVENTS: usize = 1000;

/// Buffer size for per-subscriber event channels.
const EVENT_CHANNEL_BUFFER: usize = 100;

/// A stored event with its monotonic ID for cursor-based lookup.
#[derive(Clone)]
struct StoredEvent {
    id: i64,
    event: proto::Event,
}

/// Filter criteria for an event subscriber.
struct SubscriberFilters {
    /// If non-empty, only pass events of these types.
    event_types: Option<HashMap<i32, bool>>,
    /// If non-empty, only pass events for these agent IDs.
    agent_ids: Option<HashMap<String, bool>>,
    /// If non-empty, only pass events for these workspace IDs.
    workspace_ids: Option<HashMap<String, bool>>,
}

/// An active subscriber receiving events via a channel.
struct EventSubscriber {
    filters: SubscriberFilters,
    tx: mpsc::Sender<proto::Event>,
}

/// Thread-safe event bus managing storage, subscriptions, and broadcasting.
///
/// Parity with Go's `events []storedEvent`, `eventSubs`, `publishEvent`.
pub struct EventBus {
    events: RwLock<Vec<StoredEvent>>,
    next_event_id: AtomicI64,
    subscribers: RwLock<HashMap<String, EventSubscriber>>,
    sub_id_seq: AtomicI64,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            events: RwLock::new(Vec::with_capacity(MAX_STORED_EVENTS)),
            next_event_id: AtomicI64::new(0),
            subscribers: RwLock::new(HashMap::new()),
            sub_id_seq: AtomicI64::new(0),
        }
    }

    /// Publish an event: assign an ID, store in circular buffer, broadcast to
    /// matching subscribers.
    ///
    /// Parity with Go `publishEvent`.
    pub fn publish(&self, mut event: proto::Event) {
        let id = self.next_event_id.fetch_add(1, Ordering::SeqCst);
        event.id = id.to_string();

        let stored = StoredEvent {
            id,
            event: event.clone(),
        };

        // Store in circular buffer.
        {
            let mut events = write_events(&self.events);
            if events.len() >= MAX_STORED_EVENTS {
                events.remove(0);
            }
            events.push(stored);
        }

        // Broadcast to matching subscribers.
        {
            let subs = read_subscribers(&self.subscribers);
            for sub in subs.values() {
                if event_matches_filter(&event, &sub.filters) {
                    // Non-blocking send; drop event if channel full (Go parity).
                    let _ = sub.tx.try_send(event.clone());
                }
            }
        }
    }

    /// Register a new subscriber and return (subscriber_id, receiver).
    ///
    /// If `cursor > 0`, replayed events (from that cursor onwards, matching
    /// filters) are collected and returned in the second tuple element.
    #[allow(clippy::result_large_err)]
    pub fn subscribe(
        &self,
        req: &proto::StreamEventsRequest,
    ) -> Result<(String, mpsc::Receiver<proto::Event>, Vec<proto::Event>), tonic::Status> {
        // Build filters from request.
        let event_types = if req.types.is_empty() {
            None
        } else {
            let mut m = HashMap::new();
            for t in &req.types {
                m.insert(*t, true);
            }
            Some(m)
        };

        let agent_ids = if req.agent_ids.is_empty() {
            None
        } else {
            let mut m = HashMap::new();
            for id in &req.agent_ids {
                m.insert(id.clone(), true);
            }
            Some(m)
        };

        let workspace_ids = if req.workspace_ids.is_empty() {
            None
        } else {
            let mut m = HashMap::new();
            for id in &req.workspace_ids {
                m.insert(id.clone(), true);
            }
            Some(m)
        };

        // Parse cursor.
        let cursor: i64 = if req.cursor.is_empty() {
            0
        } else {
            parse_cursor(&req.cursor)?
        };

        let filters = SubscriberFilters {
            event_types,
            agent_ids,
            workspace_ids,
        };

        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_BUFFER);

        // Register subscriber and collect replay events under a single lock.
        let sub_id = {
            let seq = self.sub_id_seq.fetch_add(1, Ordering::SeqCst) + 1;
            format!("sub-{seq}")
        };

        let mut replay = Vec::new();

        {
            let events = read_events(&self.events);
            if cursor > 0 {
                for stored in events.iter() {
                    if stored.id >= cursor && event_matches_filter(&stored.event, &filters) {
                        replay.push(stored.event.clone());
                    }
                }
            }
        }

        {
            let mut subs = write_subscribers(&self.subscribers);
            subs.insert(sub_id.clone(), EventSubscriber { filters, tx });
        }

        Ok((sub_id, rx, replay))
    }

    /// Remove a subscriber by ID. Called when the stream ends.
    pub fn unsubscribe(&self, sub_id: &str) {
        let mut subs = write_subscribers(&self.subscribers);
        subs.remove(sub_id);
    }

    /// Number of stored events (for testing).
    #[cfg(test)]
    pub fn stored_count(&self) -> usize {
        read_events(&self.events).len()
    }

    /// Number of active subscribers (for testing).
    #[cfg(test)]
    pub fn subscriber_count(&self) -> usize {
        read_subscribers(&self.subscribers).len()
    }

    // -- Convenience publish helpers (parity with Go helpers) --

    /// Publish an agent state changed event.
    pub fn publish_agent_state_changed(
        &self,
        agent_id: &str,
        workspace_id: &str,
        prev_state: i32,
        new_state: i32,
        reason: &str,
    ) {
        self.publish(proto::Event {
            id: String::new(),
            r#type: proto::EventType::AgentStateChanged as i32,
            timestamp: Some(datetime_to_timestamp(Utc::now())),
            agent_id: agent_id.to_string(),
            workspace_id: workspace_id.to_string(),
            payload: Some(proto::event::Payload::AgentStateChanged(
                proto::AgentStateChangedEvent {
                    previous_state: prev_state,
                    new_state,
                    reason: reason.to_string(),
                },
            )),
        });
    }

    /// Publish an error event.
    pub fn publish_error(
        &self,
        agent_id: &str,
        workspace_id: &str,
        code: &str,
        message: &str,
        recoverable: bool,
    ) {
        self.publish(proto::Event {
            id: String::new(),
            r#type: proto::EventType::Error as i32,
            timestamp: Some(datetime_to_timestamp(Utc::now())),
            agent_id: agent_id.to_string(),
            workspace_id: workspace_id.to_string(),
            payload: Some(proto::event::Payload::Error(proto::ErrorEvent {
                code: code.to_string(),
                message: message.to_string(),
                recoverable,
            })),
        });
    }

    /// Publish a pane content changed event.
    pub fn publish_pane_content_changed(
        &self,
        agent_id: &str,
        workspace_id: &str,
        content_hash: &str,
        lines_changed: i32,
    ) {
        self.publish(proto::Event {
            id: String::new(),
            r#type: proto::EventType::PaneContentChanged as i32,
            timestamp: Some(datetime_to_timestamp(Utc::now())),
            agent_id: agent_id.to_string(),
            workspace_id: workspace_id.to_string(),
            payload: Some(proto::event::Payload::PaneContentChanged(
                proto::PaneContentChangedEvent {
                    content_hash: content_hash.to_string(),
                    lines_changed,
                },
            )),
        });
    }

    /// Publish a resource violation event.
    pub fn publish_resource_violation(
        &self,
        agent_id: &str,
        workspace_id: &str,
        resource_type: i32,
        current_value: f64,
        limit_value: f64,
        violation_count: i32,
        action_taken: i32,
    ) {
        self.publish(proto::Event {
            id: String::new(),
            r#type: proto::EventType::ResourceViolation as i32,
            timestamp: Some(datetime_to_timestamp(Utc::now())),
            agent_id: agent_id.to_string(),
            workspace_id: workspace_id.to_string(),
            payload: Some(proto::event::Payload::ResourceViolation(
                proto::ResourceViolationEvent {
                    resource_type,
                    current_value,
                    limit_value,
                    violation_count,
                    action_taken,
                },
            )),
        });
    }
}

/// Check if an event matches a subscriber's filters.
///
/// Parity with Go `eventMatchesFilter`.
fn event_matches_filter(event: &proto::Event, filters: &SubscriberFilters) -> bool {
    // Check event type filter.
    if let Some(ref types) = filters.event_types {
        if !types.contains_key(&event.r#type) {
            return false;
        }
    }

    // Check agent ID filter.
    if let Some(ref agent_ids) = filters.agent_ids {
        if !event.agent_id.is_empty() && !agent_ids.contains_key(&event.agent_id) {
            return false;
        }
    }

    // Check workspace ID filter.
    if let Some(ref workspace_ids) = filters.workspace_ids {
        if !event.workspace_id.is_empty() && !workspace_ids.contains_key(&event.workspace_id) {
            return false;
        }
    }

    true
}

/// Convert chrono DateTime to prost Timestamp.
fn datetime_to_timestamp(dt: DateTime<Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

/// Parse a cursor string to i64. Parity with Go `parseInt64`.
#[allow(clippy::result_large_err)]
fn parse_cursor(s: &str) -> Result<i64, tonic::Status> {
    s.parse::<i64>()
        .map_err(|e| tonic::Status::invalid_argument(format!("invalid cursor: {e}")))
}

// -- RwLock helpers with poison recovery --

fn read_events(
    lock: &RwLock<Vec<StoredEvent>>,
) -> std::sync::RwLockReadGuard<'_, Vec<StoredEvent>> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn write_events(
    lock: &RwLock<Vec<StoredEvent>>,
) -> std::sync::RwLockWriteGuard<'_, Vec<StoredEvent>> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn read_subscribers(
    lock: &RwLock<HashMap<String, EventSubscriber>>,
) -> std::sync::RwLockReadGuard<'_, HashMap<String, EventSubscriber>> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn write_subscribers(
    lock: &RwLock<HashMap<String, EventSubscriber>>,
) -> std::sync::RwLockWriteGuard<'_, HashMap<String, EventSubscriber>> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_bus() -> EventBus {
        EventBus::new()
    }

    fn make_event(event_type: i32, agent_id: &str, workspace_id: &str) -> proto::Event {
        proto::Event {
            id: String::new(),
            r#type: event_type,
            timestamp: Some(datetime_to_timestamp(Utc::now())),
            agent_id: agent_id.to_string(),
            workspace_id: workspace_id.to_string(),
            payload: None,
        }
    }

    // -- EventBus basics --

    #[test]
    fn new_bus_is_empty() {
        let bus = make_bus();
        assert_eq!(bus.stored_count(), 0);
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[test]
    fn publish_assigns_monotonic_ids() {
        let bus = make_bus();
        bus.publish(make_event(1, "a1", "ws1"));
        bus.publish(make_event(2, "a2", "ws1"));
        bus.publish(make_event(1, "a1", "ws2"));

        assert_eq!(bus.stored_count(), 3);
        let events = bus.events.read().unwrap();
        assert_eq!(events[0].id, 0);
        assert_eq!(events[0].event.id, "0");
        assert_eq!(events[1].id, 1);
        assert_eq!(events[1].event.id, "1");
        assert_eq!(events[2].id, 2);
        assert_eq!(events[2].event.id, "2");
    }

    #[test]
    fn publish_enforces_circular_buffer_limit() {
        let bus = make_bus();
        for i in 0..(MAX_STORED_EVENTS + 50) {
            bus.publish(make_event(1, &format!("a{i}"), "ws1"));
        }
        assert_eq!(bus.stored_count(), MAX_STORED_EVENTS);
        let events = bus.events.read().unwrap();
        // Oldest event should be #50 (first 50 evicted).
        assert_eq!(events[0].id, 50);
        assert_eq!(events[events.len() - 1].id, (MAX_STORED_EVENTS + 49) as i64);
    }

    // -- Subscriber + replay --

    #[tokio::test]
    async fn subscribe_no_cursor_no_replay() {
        let bus = make_bus();
        bus.publish(make_event(1, "a1", "ws1"));
        bus.publish(make_event(2, "a2", "ws1"));

        let req = proto::StreamEventsRequest {
            cursor: String::new(),
            types: vec![],
            agent_ids: vec![],
            workspace_ids: vec![],
        };

        let (sub_id, _rx, replay) = bus.subscribe(&req).unwrap();
        assert!(replay.is_empty());
        assert!(!sub_id.is_empty());
        assert_eq!(bus.subscriber_count(), 1);

        bus.unsubscribe(&sub_id);
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn subscribe_with_cursor_replays_from_position() {
        let bus = make_bus();
        bus.publish(make_event(1, "a1", "ws1")); // id=0
        bus.publish(make_event(2, "a2", "ws1")); // id=1
        bus.publish(make_event(1, "a3", "ws2")); // id=2

        let req = proto::StreamEventsRequest {
            cursor: "1".to_string(),
            types: vec![],
            agent_ids: vec![],
            workspace_ids: vec![],
        };

        let (_sub_id, _rx, replay) = bus.subscribe(&req).unwrap();
        assert_eq!(replay.len(), 2);
        assert_eq!(replay[0].id, "1");
        assert_eq!(replay[1].id, "2");

        bus.unsubscribe(&_sub_id);
    }

    #[tokio::test]
    async fn subscribe_cursor_zero_string_no_replay() {
        let bus = make_bus();
        bus.publish(make_event(1, "a1", "ws1"));

        let req = proto::StreamEventsRequest {
            cursor: "0".to_string(),
            types: vec![],
            agent_ids: vec![],
            workspace_ids: vec![],
        };

        let (_sub_id, _rx, replay) = bus.subscribe(&req).unwrap();
        // cursor=0 means no replay per Go parity (cursor > 0 check).
        assert!(replay.is_empty());
        bus.unsubscribe(&_sub_id);
    }

    #[tokio::test]
    async fn subscribe_invalid_cursor_returns_error() {
        let bus = make_bus();
        let req = proto::StreamEventsRequest {
            cursor: "not-a-number".to_string(),
            types: vec![],
            agent_ids: vec![],
            workspace_ids: vec![],
        };

        let result = bus.subscribe(&req);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("invalid cursor"));
    }

    // -- Filtering --

    #[tokio::test]
    async fn subscribe_filters_by_event_type() {
        let bus = make_bus();
        bus.publish(make_event(1, "a1", "ws1")); // id=0
        bus.publish(make_event(2, "a2", "ws1")); // id=1
        bus.publish(make_event(1, "a3", "ws2")); // id=2

        let req = proto::StreamEventsRequest {
            cursor: "0".to_string(), // Won't replay (cursor > 0 check)
            types: vec![1],
            agent_ids: vec![],
            workspace_ids: vec![],
        };

        // Subscribe and verify type filter on replay with cursor=1.
        let req2 = proto::StreamEventsRequest {
            cursor: "1".to_string(),
            types: vec![1],
            agent_ids: vec![],
            workspace_ids: vec![],
        };
        let (_sub_id, _rx, replay) = bus.subscribe(&req2).unwrap();
        // Only event id=2 (type=1) should match, id=1 (type=2) should be filtered.
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].id, "2");
        bus.unsubscribe(&_sub_id);

        // Also test that the original request with cursor "0" returns no replay.
        let (_sub_id2, _rx2, replay2) = bus.subscribe(&req).unwrap();
        assert!(replay2.is_empty());
        bus.unsubscribe(&_sub_id2);
    }

    #[tokio::test]
    async fn subscribe_filters_by_agent_id() {
        let bus = make_bus();
        bus.publish(make_event(1, "a1", "ws1")); // id=0
        bus.publish(make_event(1, "a2", "ws1")); // id=1
        bus.publish(make_event(1, "a1", "ws2")); // id=2

        let req = proto::StreamEventsRequest {
            cursor: "0".to_string(), // replay from id >= 0... but cursor > 0 check
            types: vec![],
            agent_ids: vec!["a1".to_string()],
            workspace_ids: vec![],
        };

        // Use cursor=1 to test.
        let req2 = proto::StreamEventsRequest {
            cursor: "1".to_string(),
            types: vec![],
            agent_ids: vec!["a1".to_string()],
            workspace_ids: vec![],
        };
        let (_sub_id, _rx, replay) = bus.subscribe(&req2).unwrap();
        // Only id=2 (agent=a1) should match; id=1 (agent=a2) filtered out.
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].agent_id, "a1");
        bus.unsubscribe(&_sub_id);

        let (_sub_id2, _rx2, _) = bus.subscribe(&req).unwrap();
        bus.unsubscribe(&_sub_id2);
    }

    #[tokio::test]
    async fn subscribe_filters_by_workspace_id() {
        let bus = make_bus();
        bus.publish(make_event(1, "a1", "ws1")); // id=0
        bus.publish(make_event(1, "a1", "ws2")); // id=1
        bus.publish(make_event(1, "a2", "ws1")); // id=2

        let req = proto::StreamEventsRequest {
            cursor: "1".to_string(),
            types: vec![],
            agent_ids: vec![],
            workspace_ids: vec!["ws1".to_string()],
        };

        let (_sub_id, _rx, replay) = bus.subscribe(&req).unwrap();
        // id=1 is ws2 (filtered), id=2 is ws1 (matches).
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].workspace_id, "ws1");
        bus.unsubscribe(&_sub_id);
    }

    #[tokio::test]
    async fn subscribe_combined_filters() {
        let bus = make_bus();
        bus.publish(make_event(1, "a1", "ws1")); // id=0
        bus.publish(make_event(2, "a1", "ws1")); // id=1 - type mismatch
        bus.publish(make_event(1, "a2", "ws1")); // id=2 - agent mismatch
        bus.publish(make_event(1, "a1", "ws2")); // id=3 - workspace mismatch
        bus.publish(make_event(1, "a1", "ws1")); // id=4 - matches all

        let req = proto::StreamEventsRequest {
            cursor: "1".to_string(),
            types: vec![1],
            agent_ids: vec!["a1".to_string()],
            workspace_ids: vec!["ws1".to_string()],
        };

        let (_sub_id, _rx, replay) = bus.subscribe(&req).unwrap();
        // Only id=4 should match all 3 filters.
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].id, "4");
        bus.unsubscribe(&_sub_id);
    }

    // -- Live broadcasting --

    #[tokio::test]
    async fn published_events_sent_to_subscribers() {
        let bus = make_bus();

        let req = proto::StreamEventsRequest {
            cursor: String::new(),
            types: vec![],
            agent_ids: vec![],
            workspace_ids: vec![],
        };

        let (sub_id, mut rx, _replay) = bus.subscribe(&req).unwrap();

        bus.publish(make_event(1, "a1", "ws1"));
        bus.publish(make_event(2, "a2", "ws1"));

        let ev1 = rx.try_recv().unwrap();
        assert_eq!(ev1.id, "0");
        assert_eq!(ev1.agent_id, "a1");

        let ev2 = rx.try_recv().unwrap();
        assert_eq!(ev2.id, "1");
        assert_eq!(ev2.agent_id, "a2");

        bus.unsubscribe(&sub_id);
    }

    #[tokio::test]
    async fn subscriber_filter_applied_to_live_events() {
        let bus = make_bus();

        let req = proto::StreamEventsRequest {
            cursor: String::new(),
            types: vec![1],
            agent_ids: vec![],
            workspace_ids: vec![],
        };

        let (sub_id, mut rx, _) = bus.subscribe(&req).unwrap();

        bus.publish(make_event(2, "a1", "ws1")); // type=2, filtered
        bus.publish(make_event(1, "a2", "ws1")); // type=1, passes

        // Only the second event should arrive.
        let ev = rx.try_recv().unwrap();
        assert_eq!(ev.agent_id, "a2");
        assert!(rx.try_recv().is_err());

        bus.unsubscribe(&sub_id);
    }

    #[tokio::test]
    async fn unsubscribe_stops_delivery() {
        let bus = make_bus();

        let req = proto::StreamEventsRequest {
            cursor: String::new(),
            types: vec![],
            agent_ids: vec![],
            workspace_ids: vec![],
        };

        let (sub_id, mut rx, _) = bus.subscribe(&req).unwrap();
        bus.unsubscribe(&sub_id);

        bus.publish(make_event(1, "a1", "ws1"));
        assert!(rx.try_recv().is_err());
    }

    // -- Multiple subscribers --

    #[tokio::test]
    async fn multiple_subscribers_each_get_matching_events() {
        let bus = make_bus();

        let req1 = proto::StreamEventsRequest {
            cursor: String::new(),
            types: vec![1],
            agent_ids: vec![],
            workspace_ids: vec![],
        };
        let req2 = proto::StreamEventsRequest {
            cursor: String::new(),
            types: vec![2],
            agent_ids: vec![],
            workspace_ids: vec![],
        };

        let (id1, mut rx1, _) = bus.subscribe(&req1).unwrap();
        let (id2, mut rx2, _) = bus.subscribe(&req2).unwrap();

        bus.publish(make_event(1, "a1", "ws1"));
        bus.publish(make_event(2, "a2", "ws1"));

        // Sub1 only gets type=1.
        let e = rx1.try_recv().unwrap();
        assert_eq!(e.r#type, 1);
        assert!(rx1.try_recv().is_err());

        // Sub2 only gets type=2.
        let e = rx2.try_recv().unwrap();
        assert_eq!(e.r#type, 2);
        assert!(rx2.try_recv().is_err());

        bus.unsubscribe(&id1);
        bus.unsubscribe(&id2);
    }

    // -- Convenience publish helpers --

    #[test]
    fn publish_agent_state_changed_stores_event() {
        let bus = make_bus();
        bus.publish_agent_state_changed("a1", "ws1", 2, 3, "cooldown");
        assert_eq!(bus.stored_count(), 1);

        let events = bus.events.read().unwrap();
        let e = &events[0].event;
        assert_eq!(e.r#type, proto::EventType::AgentStateChanged as i32);
        assert_eq!(e.agent_id, "a1");
        assert_eq!(e.workspace_id, "ws1");
        match &e.payload {
            Some(proto::event::Payload::AgentStateChanged(p)) => {
                assert_eq!(p.previous_state, 2);
                assert_eq!(p.new_state, 3);
                assert_eq!(p.reason, "cooldown");
            }
            _ => panic!("expected AgentStateChanged payload"),
        }
    }

    #[test]
    fn publish_error_stores_event() {
        let bus = make_bus();
        bus.publish_error("a1", "ws1", "INTERNAL", "something failed", true);
        assert_eq!(bus.stored_count(), 1);

        let events = bus.events.read().unwrap();
        let e = &events[0].event;
        assert_eq!(e.r#type, proto::EventType::Error as i32);
        match &e.payload {
            Some(proto::event::Payload::Error(p)) => {
                assert_eq!(p.code, "INTERNAL");
                assert_eq!(p.message, "something failed");
                assert!(p.recoverable);
            }
            _ => panic!("expected Error payload"),
        }
    }

    #[test]
    fn publish_pane_content_changed_stores_event() {
        let bus = make_bus();
        bus.publish_pane_content_changed("a1", "ws1", "abc123", 42);
        assert_eq!(bus.stored_count(), 1);

        let events = bus.events.read().unwrap();
        let e = &events[0].event;
        assert_eq!(e.r#type, proto::EventType::PaneContentChanged as i32);
        match &e.payload {
            Some(proto::event::Payload::PaneContentChanged(p)) => {
                assert_eq!(p.content_hash, "abc123");
                assert_eq!(p.lines_changed, 42);
            }
            _ => panic!("expected PaneContentChanged payload"),
        }
    }

    #[test]
    fn publish_resource_violation_stores_event() {
        let bus = make_bus();
        bus.publish_resource_violation("a1", "ws1", 1, 95.0, 80.0, 3, 2);
        assert_eq!(bus.stored_count(), 1);

        let events = bus.events.read().unwrap();
        let e = &events[0].event;
        assert_eq!(e.r#type, proto::EventType::ResourceViolation as i32);
        match &e.payload {
            Some(proto::event::Payload::ResourceViolation(p)) => {
                assert_eq!(p.resource_type, 1);
                assert!((p.current_value - 95.0).abs() < f64::EPSILON);
                assert!((p.limit_value - 80.0).abs() < f64::EPSILON);
                assert_eq!(p.violation_count, 3);
                assert_eq!(p.action_taken, 2);
            }
            _ => panic!("expected ResourceViolation payload"),
        }
    }

    // -- parse_cursor --

    #[test]
    fn parse_cursor_valid() {
        assert_eq!(parse_cursor("0").unwrap(), 0);
        assert_eq!(parse_cursor("42").unwrap(), 42);
        assert_eq!(parse_cursor("999").unwrap(), 999);
    }

    #[test]
    fn parse_cursor_invalid() {
        assert!(parse_cursor("abc").is_err());
        assert!(parse_cursor("12.5").is_err());
        assert!(parse_cursor("").is_err());
    }

    // -- event_matches_filter unit tests --

    #[test]
    fn filter_no_criteria_matches_all() {
        let filters = SubscriberFilters {
            event_types: None,
            agent_ids: None,
            workspace_ids: None,
        };
        let event = make_event(1, "a1", "ws1");
        assert!(event_matches_filter(&event, &filters));
    }

    #[test]
    fn filter_event_type_match() {
        let mut types = HashMap::new();
        types.insert(1, true);
        let filters = SubscriberFilters {
            event_types: Some(types),
            agent_ids: None,
            workspace_ids: None,
        };
        assert!(event_matches_filter(&make_event(1, "a1", "ws1"), &filters));
        assert!(!event_matches_filter(&make_event(2, "a1", "ws1"), &filters));
    }

    #[test]
    fn filter_agent_id_match() {
        let mut agents = HashMap::new();
        agents.insert("a1".to_string(), true);
        let filters = SubscriberFilters {
            event_types: None,
            agent_ids: Some(agents),
            workspace_ids: None,
        };
        assert!(event_matches_filter(&make_event(1, "a1", "ws1"), &filters));
        assert!(!event_matches_filter(&make_event(1, "a2", "ws1"), &filters));
        // Empty agent_id passes (Go parity: only filter non-empty agent_id).
        assert!(event_matches_filter(&make_event(1, "", "ws1"), &filters));
    }

    #[test]
    fn filter_workspace_id_match() {
        let mut ws = HashMap::new();
        ws.insert("ws1".to_string(), true);
        let filters = SubscriberFilters {
            event_types: None,
            agent_ids: None,
            workspace_ids: Some(ws),
        };
        assert!(event_matches_filter(&make_event(1, "a1", "ws1"), &filters));
        assert!(!event_matches_filter(&make_event(1, "a1", "ws2"), &filters));
        // Empty workspace_id passes (Go parity).
        assert!(event_matches_filter(&make_event(1, "a1", ""), &filters));
    }

    // -- Integration: subscribe + publish + receive flow --

    #[tokio::test]
    async fn full_subscribe_replay_then_live_flow() {
        let bus = make_bus();

        // Publish some initial events.
        bus.publish(make_event(1, "a1", "ws1")); // id=0
        bus.publish(make_event(1, "a2", "ws1")); // id=1
        bus.publish(make_event(2, "a1", "ws1")); // id=2

        // Subscribe with cursor=1, type filter=1.
        let req = proto::StreamEventsRequest {
            cursor: "1".to_string(),
            types: vec![1],
            agent_ids: vec![],
            workspace_ids: vec![],
        };

        let (sub_id, mut rx, replay) = bus.subscribe(&req).unwrap();

        // Replay: id=1 matches (type=1), id=2 filtered (type=2).
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].id, "1");

        // Live event: type=1 should arrive, type=2 should not.
        bus.publish(make_event(2, "a3", "ws1")); // filtered
        bus.publish(make_event(1, "a3", "ws1")); // passes

        let live = rx.try_recv().unwrap();
        assert_eq!(live.agent_id, "a3");
        assert_eq!(live.r#type, 1);
        assert!(rx.try_recv().is_err());

        bus.unsubscribe(&sub_id);
    }
}
