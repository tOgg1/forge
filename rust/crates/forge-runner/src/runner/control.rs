use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use crate::sink::EventSink;

use super::types::{
    BusyData, ControlCommand, ControlData, ControlErrorData, InputSentData, RunnerEvent,
    EVENT_TYPE_BUSY, EVENT_TYPE_CONTROL_ERROR, EVENT_TYPE_COOLDOWN, EVENT_TYPE_INPUT_SENT,
    EVENT_TYPE_PAUSE, EVENT_TYPE_SWAP_ACCOUNT, MAX_EVENT_LINE_LENGTH,
};
use super::util::{parse_cooldown, parse_positive_duration, truncate_text};
use super::State;

#[derive(Clone)]
pub struct ControlRuntime {
    pub state: Arc<State>,
    pub sink: Arc<dyn EventSink>,
    pub workspace_id: String,
    pub agent_id: String,
    pub now: fn() -> DateTime<Utc>,
    pub stdin: Arc<Mutex<std::process::ChildStdin>>,
}

impl ControlRuntime {
    pub fn control_loop(
        self,
        mut reader: Box<dyn Read + Send>,
        stop: Arc<std::sync::atomic::AtomicBool>,
    ) {
        let mut buf = String::new();
        let mut local = [0u8; 4096];

        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            match reader.read(&mut local) {
                Ok(0) => return,
                Ok(n) => {
                    buf.push_str(&String::from_utf8_lossy(&local[..n]));
                    while let Some(pos) = buf.find('\n') {
                        let line = buf[..pos].trim_end_matches('\r').to_string();
                        buf.drain(..pos + 1);
                        let trimmed = line.trim().to_string();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if let Some(cmd) = parse_control_command(&trimmed) {
                            self.handle_control(&cmd, &trimmed);
                        } else {
                            let _ = self.send_input(&line);
                        }
                    }
                }
                Err(_) => {
                    self.emit_control_error("control reader error", "");
                    return;
                }
            }
        }
    }

    fn send_input(&self, text: &str) -> Result<(), String> {
        self.state.wait_for_resume(self.now);

        let mut payload = text.to_string();
        if !payload.ends_with('\n') {
            payload.push('\n');
        }

        let mut guard = self
            .stdin
            .lock()
            .map_err(|_| "stdin lock poisoned".to_string())?;
        guard
            .write_all(payload.as_bytes())
            .map_err(|err| format!("write input: {err}"))?;
        guard.flush().ok();
        drop(guard);

        self.state.set_last_activity((self.now)());
        let (preview, truncated) = truncate_text(text, MAX_EVENT_LINE_LENGTH);
        if !preview.is_empty() || text.is_empty() {
            self.emit(
                EVENT_TYPE_INPUT_SENT,
                Some(
                    serde_json::to_value(InputSentData {
                        text: preview,
                        truncated: if truncated { Some(true) } else { None },
                    })
                    .unwrap_or(serde_json::Value::Null),
                ),
            );
        }
        if self.state.set_ready(false) {
            self.emit(
                EVENT_TYPE_BUSY,
                serde_json::to_value(BusyData {
                    reason: "input_sent".to_string(),
                })
                .ok(),
            );
        }

        Ok(())
    }

    fn handle_control(&self, cmd: &ControlCommand, raw: &str) {
        match cmd.command_type.trim() {
            "send_message" | "send" => {
                let mut text = cmd.text.trim().to_string();
                if text.is_empty() {
                    text = cmd.message.trim().to_string();
                }
                if text.is_empty() {
                    self.emit_control_error("send_message requires text", raw);
                    return;
                }
                let _ = self.send_input(&text);
            }
            "pause" => match parse_positive_duration(&cmd.duration) {
                Ok(dur) => {
                    let until = (self.now)() + chrono::Duration::from_std(dur).unwrap_or_default();
                    self.state.set_paused_until(until);
                    self.emit(
                        EVENT_TYPE_PAUSE,
                        serde_json::to_value(ControlData {
                            action: "pause".to_string(),
                            duration: cmd.duration.trim().to_string(),
                            until: until.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                            account_id: String::new(),
                        })
                        .ok(),
                    );
                }
                Err(err) => self.emit_control_error(&err, raw),
            },
            "cooldown" => match parse_cooldown(&cmd.until, &cmd.duration, (self.now)()) {
                Ok(until) => {
                    self.state.set_paused_until(until);
                    self.emit(
                        EVENT_TYPE_COOLDOWN,
                        serde_json::to_value(ControlData {
                            action: "cooldown".to_string(),
                            duration: String::new(),
                            until: until.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                            account_id: String::new(),
                        })
                        .ok(),
                    );
                }
                Err(err) => self.emit_control_error(&err, raw),
            },
            "swap_account" => {
                self.emit(
                    EVENT_TYPE_SWAP_ACCOUNT,
                    serde_json::to_value(ControlData {
                        action: "swap_account".to_string(),
                        duration: String::new(),
                        until: String::new(),
                        account_id: cmd.account_id.trim().to_string(),
                    })
                    .ok(),
                );
            }
            _ => self.emit_control_error("unknown control command", raw),
        }
    }

    fn emit_control_error(&self, error: &str, raw: &str) {
        self.emit(
            EVENT_TYPE_CONTROL_ERROR,
            serde_json::to_value(ControlErrorData {
                error: error.to_string(),
                raw: raw.to_string(),
            })
            .ok(),
        );
    }

    fn emit(&self, event_type: &str, data: Option<serde_json::Value>) {
        let now = (self.now)();
        let event = RunnerEvent {
            event_type: event_type.to_string(),
            timestamp: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            workspace_id: self.workspace_id.clone(),
            agent_id: self.agent_id.clone(),
            data,
        };
        let _ = self.sink.emit(&event);
    }
}

fn parse_control_command(raw: &str) -> Option<ControlCommand> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let probe = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    probe.get("type")?.as_str()?;
    serde_json::from_str::<ControlCommand>(trimmed).ok()
}
