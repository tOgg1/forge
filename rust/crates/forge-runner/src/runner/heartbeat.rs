use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::ring::LineRing;
use crate::sink::EventSink;

use super::state::State;
use super::types::{HeartbeatData, RunnerEvent, EVENT_TYPE_HEARTBEAT, MAX_EVENT_LINE_LENGTH};
use super::util::{format_idle_for, truncate_lines};

#[derive(Clone)]
pub struct HeartbeatRuntime {
    pub state: Arc<State>,
    pub sink: Arc<dyn EventSink>,
    pub workspace_id: String,
    pub agent_id: String,
    pub output: Arc<LineRing>,
    pub heartbeat_interval: Duration,
    pub now: fn() -> DateTime<Utc>,
}

impl HeartbeatRuntime {
    pub fn heartbeat_loop(self, stop: Arc<std::sync::atomic::AtomicBool>) {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            thread::sleep(self.heartbeat_interval);
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            let last = self.state.get_last_activity((self.now)());
            let now = (self.now)();
            let idle_for = now
                .signed_duration_since(last)
                .to_std()
                .unwrap_or_else(|_| Duration::from_secs(0));
            let tail = truncate_lines(&self.output.snapshot(), MAX_EVENT_LINE_LENGTH);

            let data = serde_json::to_value(HeartbeatData {
                last_activity: last.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                idle_for: format_idle_for(idle_for),
                tail,
            })
            .ok();

            let event = RunnerEvent {
                event_type: EVENT_TYPE_HEARTBEAT.to_string(),
                timestamp: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                workspace_id: self.workspace_id.clone(),
                agent_id: self.agent_id.clone(),
                data,
            };
            let _ = self.sink.emit(&event);
        }
    }
}
