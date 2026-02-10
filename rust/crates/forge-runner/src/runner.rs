use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use regex::Regex;

use crate::ring::LineRing;
use crate::sink::{EventSink, NoopSink};

mod control;
mod heartbeat;
mod state;
mod types;
mod util;

pub use types::{
    BusyData, ControlCommand, ControlData, ControlErrorData, ExitData, HeartbeatData,
    InputSentData, OutputLineData, PromptReadyData, RunnerError, RunnerEvent, DEFAULT_BUSY_REGEX,
    DEFAULT_HEARTBEAT_INTERVAL, DEFAULT_PROMPT_REGEX, DEFAULT_TAIL_BYTES, DEFAULT_TAIL_LINES,
    EVENT_TYPE_BUSY, EVENT_TYPE_CONTROL_ERROR, EVENT_TYPE_COOLDOWN, EVENT_TYPE_EXIT,
    EVENT_TYPE_HEARTBEAT, EVENT_TYPE_INPUT_SENT, EVENT_TYPE_OUTPUT_LINE, EVENT_TYPE_PAUSE,
    EVENT_TYPE_PROMPT_READY, EVENT_TYPE_SWAP_ACCOUNT, MAX_EVENT_LINE_LENGTH, MAX_PENDING_BYTES,
};

pub use util::parse_go_duration_to_nanos;

pub(crate) use state::State;

use control::ControlRuntime;
use heartbeat::HeartbeatRuntime;
use util::{cap_pending_bytes, contains_non_whitespace, split_lines, truncate_text};

pub struct Runner {
    pub workspace_id: String,
    pub agent_id: String,
    pub command: Vec<String>,

    pub prompt_regex: Option<Regex>,
    pub busy_regex: Option<Regex>,

    pub heartbeat_interval: Duration,
    pub tail_lines: usize,
    pub tail_bytes: usize,

    pub event_sink: Arc<dyn EventSink>,
    pub control_reader: Option<Box<dyn Read + Send>>,
    pub output_writer: Box<dyn Write + Send>,
    pub now: Option<fn() -> DateTime<Utc>>,

    state: Arc<State>,
    output: Arc<LineRing>,
}

impl Runner {
    pub fn new(workspace_id: &str, agent_id: &str, command: Vec<String>) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            agent_id: agent_id.to_string(),
            command,
            prompt_regex: None,
            busy_regex: None,
            heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,
            tail_lines: DEFAULT_TAIL_LINES,
            tail_bytes: DEFAULT_TAIL_BYTES,
            event_sink: Arc::new(NoopSink),
            control_reader: None,
            output_writer: Box::new(std::io::sink()),
            now: None,
            state: Arc::new(State::new()),
            output: Arc::new(LineRing::new(DEFAULT_TAIL_LINES)),
        }
    }

    pub fn run(&mut self) -> Result<(), RunnerError> {
        self.validate()?;
        self.apply_defaults();

        self.state.set_last_activity(self.now_utc());

        let mut cmd = Command::new(&self.command[0]);
        cmd.args(&self.command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|err| RunnerError::Spawn(err.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RunnerError::Spawn("missing stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RunnerError::Spawn("missing stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| RunnerError::Spawn("missing stderr".to_string()))?;

        let stdin = Arc::new(Mutex::new(stdin));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let (tx, rx) = mpsc::channel::<OutputChunk>();
        spawn_pipe_reader(stdout, tx.clone());
        spawn_pipe_reader(stderr, tx.clone());

        if let Some(reader) = self.control_reader.take() {
            let control = ControlRuntime {
                state: self.state.clone(),
                sink: self.event_sink.clone(),
                workspace_id: self.workspace_id.clone(),
                agent_id: self.agent_id.clone(),
                now: self.now_fn(),
                stdin: stdin.clone(),
            };
            let stop = stop.clone();
            thread::spawn(move || control.control_loop(reader, stop));
        }

        if self.heartbeat_interval > Duration::from_secs(0) {
            let heartbeat = HeartbeatRuntime {
                state: self.state.clone(),
                sink: self.event_sink.clone(),
                workspace_id: self.workspace_id.clone(),
                agent_id: self.agent_id.clone(),
                output: self.output.clone(),
                heartbeat_interval: self.heartbeat_interval,
                now: self.now_fn(),
            };
            let stop = stop.clone();
            thread::spawn(move || heartbeat.heartbeat_loop(stop));
        }

        let mut pending: Vec<u8> = Vec::with_capacity(4096);
        let mut tail: Vec<u8> = Vec::with_capacity(self.tail_bytes);
        let mut eof_count = 0usize;

        while eof_count < 2 {
            let chunk = rx.recv().map_err(|err| RunnerError::Io(err.to_string()))?;
            if chunk.eof {
                eof_count += 1;
                continue;
            }
            if chunk.data.is_empty() {
                continue;
            }

            let _ = self.output_writer.write_all(&chunk.data);
            let _ = self.output_writer.flush();

            self.state.set_last_activity(self.now_utc());

            tail.extend_from_slice(&chunk.data);
            if tail.len() > self.tail_bytes {
                let drain = tail.len() - self.tail_bytes;
                tail.drain(0..drain);
            }

            pending.extend_from_slice(&chunk.data);
            let (lines, remainder) = split_lines(&pending);
            pending = cap_pending_bytes(remainder);

            for line in lines {
                self.handle_line(&line);
            }

            self.detect_state(&tail, &chunk.data);
        }

        let status = child
            .wait()
            .map_err(|err| RunnerError::Io(err.to_string()))?;
        stop.store(true, std::sync::atomic::Ordering::Relaxed);

        let (exit_code, exit_err) = match status.code() {
            Some(code) => (code, String::new()),
            None => (1, "process terminated by signal".to_string()),
        };

        self.emit(
            EVENT_TYPE_EXIT,
            serde_json::to_value(ExitData {
                exit_code,
                error: exit_err,
            })
            .ok(),
        );
        let _ = self.event_sink.close();
        Ok(())
    }

    fn validate(&self) -> Result<(), RunnerError> {
        if self.workspace_id.trim().is_empty() {
            return Err(RunnerError::MissingWorkspaceID);
        }
        if self.agent_id.trim().is_empty() {
            return Err(RunnerError::MissingAgentID);
        }
        if self.command.is_empty() || self.command[0].trim().is_empty() {
            return Err(RunnerError::MissingCommand);
        }
        Ok(())
    }

    fn apply_defaults(&mut self) {
        if self.heartbeat_interval == Duration::from_secs(0) {
            self.heartbeat_interval = DEFAULT_HEARTBEAT_INTERVAL;
        }
        if self.tail_lines == 0 {
            self.tail_lines = DEFAULT_TAIL_LINES;
        }
        if self.tail_bytes == 0 {
            self.tail_bytes = DEFAULT_TAIL_BYTES;
        }
        if self.prompt_regex.is_none() {
            self.prompt_regex = Regex::new(DEFAULT_PROMPT_REGEX).ok();
        }
        if self.busy_regex.is_none() {
            self.busy_regex = Regex::new(DEFAULT_BUSY_REGEX).ok();
        }
        self.output = Arc::new(LineRing::new(self.tail_lines));
    }

    fn now_fn(&self) -> fn() -> DateTime<Utc> {
        self.now.unwrap_or(Utc::now)
    }

    fn now_utc(&self) -> DateTime<Utc> {
        (self.now_fn())()
    }

    fn handle_line(&self, line: &str) {
        self.output.add(line);
        let (preview, truncated) = truncate_text(line, MAX_EVENT_LINE_LENGTH);
        self.emit(
            EVENT_TYPE_OUTPUT_LINE,
            serde_json::to_value(OutputLineData {
                line: preview,
                truncated: if truncated { Some(true) } else { None },
            })
            .ok(),
        );
    }

    fn detect_state(&self, tail: &[u8], chunk: &[u8]) {
        let tail_text = String::from_utf8_lossy(tail);

        if let Some(re) = &self.prompt_regex {
            if re.is_match(&tail_text) {
                if self.state.set_ready(true) {
                    self.emit(
                        EVENT_TYPE_PROMPT_READY,
                        serde_json::to_value(PromptReadyData {
                            reason: "prompt_match".to_string(),
                        })
                        .ok(),
                    );
                }
                return;
            }
        }

        if let Some(re) = &self.busy_regex {
            if re.is_match(&tail_text) {
                if self.state.set_ready(false) {
                    self.emit(
                        EVENT_TYPE_BUSY,
                        serde_json::to_value(BusyData {
                            reason: "busy_match".to_string(),
                        })
                        .ok(),
                    );
                }
                return;
            }
        }

        if self.state.is_ready() && contains_non_whitespace(chunk) && self.state.set_ready(false) {
            self.emit(
                EVENT_TYPE_BUSY,
                serde_json::to_value(BusyData {
                    reason: "output_received".to_string(),
                })
                .ok(),
            );
        }
    }

    fn emit(&self, event_type: &str, data: Option<serde_json::Value>) {
        let event = RunnerEvent {
            event_type: event_type.to_string(),
            timestamp: self
                .now_utc()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            workspace_id: self.workspace_id.clone(),
            agent_id: self.agent_id.clone(),
            data,
        };
        let _ = self.event_sink.emit(&event);
    }
}

#[derive(Debug)]
struct OutputChunk {
    data: Vec<u8>,
    eof: bool,
}

fn spawn_pipe_reader<R: Read + Send + 'static>(mut reader: R, tx: mpsc::Sender<OutputChunk>) {
    thread::spawn(move || {
        let mut buf = vec![0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let _ = tx.send(OutputChunk {
                        data: Vec::new(),
                        eof: true,
                    });
                    return;
                }
                Ok(n) => {
                    let _ = tx.send(OutputChunk {
                        data: buf[..n].to_vec(),
                        eof: false,
                    });
                }
                Err(_) => {
                    let _ = tx.send(OutputChunk {
                        data: Vec::new(),
                        eof: true,
                    });
                    return;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{
        Runner, RunnerEvent, EVENT_TYPE_EXIT, EVENT_TYPE_INPUT_SENT, EVENT_TYPE_OUTPUT_LINE,
        EVENT_TYPE_PROMPT_READY,
    };
    use crate::sink::EventSink;

    #[derive(Default)]
    struct MemorySink {
        events: Mutex<Vec<RunnerEvent>>,
    }

    impl MemorySink {
        fn snapshot(&self) -> Vec<RunnerEvent> {
            match self.events.lock() {
                Ok(guard) => guard.clone(),
                Err(poisoned) => poisoned.into_inner().clone(),
            }
        }
    }

    impl EventSink for MemorySink {
        fn emit(&self, event: &RunnerEvent) -> Result<(), String> {
            let mut guard = self
                .events
                .lock()
                .map_err(|_| "memory sink lock poisoned".to_string())?;
            guard.push(event.clone());
            Ok(())
        }

        fn close(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn runner_rejects_missing_fields() {
        let mut r = Runner::new("", "a", vec!["echo".to_string()]);
        assert!(r.run().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn runner_emits_lifecycle_events_for_spawn_monitor_and_stop() {
        use std::io::Cursor;
        use std::os::unix::fs::PermissionsExt;
        use std::time::Duration;

        use tempfile::tempdir;

        let dir = match tempdir() {
            Ok(dir) => dir,
            Err(err) => panic!("tempdir: {err}"),
        };
        let script_path = dir.path().join("fake-agent.sh");
        let script = "\
#!/bin/sh
printf 'ready> \n'
while IFS= read -r line; do
  if [ \"$line\" = \"exit\" ]; then
    echo \"bye\"
    exit 0
  fi
  echo \"working on: $line\"
  printf 'ready> \n'
done
";
        if let Err(err) = std::fs::write(&script_path, script) {
            panic!("write script: {err}");
        }
        if let Err(err) =
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
        {
            panic!("chmod script: {err}");
        }

        let sink = Arc::new(MemorySink::default());
        let mut runner = Runner::new(
            "ws_123",
            "agent_456",
            vec![script_path.to_string_lossy().to_string()],
        );
        runner.event_sink = sink.clone();
        runner.heartbeat_interval = Duration::from_millis(5);
        runner.control_reader = Some(Box::new(Cursor::new(
            b"hello\n{\"type\":\"send_message\",\"text\":\"exit\"}\n".to_vec(),
        )));

        if let Err(err) = runner.run() {
            panic!("runner failed: {err}");
        }

        let events = sink.snapshot();
        assert!(events
            .iter()
            .any(|event| event.event_type == EVENT_TYPE_PROMPT_READY));
        assert!(events
            .iter()
            .any(|event| event.event_type == EVENT_TYPE_INPUT_SENT));
        assert!(events
            .iter()
            .any(|event| event.event_type == EVENT_TYPE_EXIT));
        assert!(
            events
                .iter()
                .filter(|event| event.event_type == EVENT_TYPE_OUTPUT_LINE)
                .any(|event| {
                    event
                        .data
                        .as_ref()
                        .and_then(|data| data.get("line"))
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|line| line.contains("working on: hello"))
                }),
            "expected output_line containing work output",
        );

        let exit_code = events
            .iter()
            .find(|event| event.event_type == EVENT_TYPE_EXIT)
            .and_then(|event| event.data.as_ref())
            .and_then(|data| data.get("exit_code"))
            .and_then(serde_json::Value::as_i64);
        assert_eq!(exit_code, Some(0));
    }

    #[cfg(unix)]
    #[test]
    fn runner_can_start_again_after_prior_stop() {
        use std::io::Cursor;
        use std::os::unix::fs::PermissionsExt;
        use tempfile::tempdir;

        let dir = match tempdir() {
            Ok(dir) => dir,
            Err(err) => panic!("tempdir: {err}"),
        };
        let script_path = dir.path().join("fake-agent.sh");
        let script = "\
#!/bin/sh
while IFS= read -r line; do
  if [ \"$line\" = \"exit\" ]; then
    echo \"bye\"
    exit 0
  fi
done
";
        if let Err(err) = std::fs::write(&script_path, script) {
            panic!("write script: {err}");
        }
        if let Err(err) =
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
        {
            panic!("chmod script: {err}");
        }

        let run_once = || {
            let sink = Arc::new(MemorySink::default());
            let mut runner = Runner::new(
                "ws_123",
                "agent_456",
                vec![script_path.to_string_lossy().to_string()],
            );
            runner.event_sink = sink.clone();
            runner.control_reader = Some(Box::new(Cursor::new(
                b"{\"type\":\"send_message\",\"text\":\"exit\"}\n".to_vec(),
            )));
            if let Err(err) = runner.run() {
                panic!("runner failed: {err}");
            }
            sink.snapshot()
        };

        let first = run_once();
        let second = run_once();

        assert!(first
            .iter()
            .any(|event| event.event_type == EVENT_TYPE_EXIT));
        assert!(second
            .iter()
            .any(|event| event.event_type == EVENT_TYPE_EXIT));
    }
}
