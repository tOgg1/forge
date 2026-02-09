use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};

#[derive(Debug)]
pub struct State {
    inner: Mutex<StateInner>,
}

#[derive(Debug, Default)]
struct StateInner {
    ready: bool,
    paused_until: Option<DateTime<Utc>>,
    last_activity: Option<DateTime<Utc>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(StateInner::default()),
        }
    }

    pub fn set_ready(&self, value: bool) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        if inner.ready == value {
            return false;
        }
        inner.ready = value;
        true
    }

    pub fn is_ready(&self) -> bool {
        let Ok(inner) = self.inner.lock() else {
            return false;
        };
        inner.ready
    }

    pub fn set_last_activity(&self, ts: DateTime<Utc>) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        inner.last_activity = Some(ts);
    }

    pub fn get_last_activity(&self, default: DateTime<Utc>) -> DateTime<Utc> {
        let Ok(inner) = self.inner.lock() else {
            return default;
        };
        inner.last_activity.unwrap_or(default)
    }

    pub fn set_paused_until(&self, until: DateTime<Utc>) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if inner.paused_until.map_or(true, |prev| until > prev) {
            inner.paused_until = Some(until);
        }
    }

    pub fn get_paused_until(&self) -> Option<DateTime<Utc>> {
        let Ok(inner) = self.inner.lock() else {
            return None;
        };
        inner.paused_until
    }

    pub fn wait_for_resume(&self, now: fn() -> DateTime<Utc>) {
        loop {
            let Some(until) = self.get_paused_until() else {
                return;
            };
            let now = now();
            if until <= now {
                return;
            }
            let sleep_for = (until - now)
                .to_std()
                .unwrap_or_else(|_| Duration::from_millis(10));
            thread::sleep(sleep_for);
        }
    }
}
