//! Raw PTY attach mode primitives for terminal-stream inspection from TUI.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawPtyTransport {
    TmuxPane { pane_id: String },
    LocalSocket { socket_path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPtyAttachRequest {
    pub agent_id: String,
    pub transport: RawPtyTransport,
    pub include_scrollback_lines: usize,
    pub follow_stream: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPtyAttachPlan {
    pub supported: bool,
    pub keypress_count: u8,
    pub commands: Vec<String>,
    pub detach_command: Option<String>,
    pub fallback_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPtySessionState {
    pub attached_agent_id: Option<String>,
    pub transport: Option<RawPtyTransport>,
    pub follow_stream: bool,
    pub max_bytes: usize,
    pub raw_bytes: Vec<u8>,
    pub last_sequence: u64,
}

impl Default for RawPtySessionState {
    fn default() -> Self {
        Self {
            attached_agent_id: None,
            transport: None,
            follow_stream: false,
            max_bytes: 64 * 1024,
            raw_bytes: Vec::new(),
            last_sequence: 0,
        }
    }
}

#[must_use]
pub fn build_raw_pty_attach_plan(request: &RawPtyAttachRequest) -> RawPtyAttachPlan {
    let agent_id = normalize_token(&request.agent_id);
    if agent_id.is_empty() {
        return unsupported_plan("agent id is required");
    }

    match &request.transport {
        RawPtyTransport::TmuxPane { pane_id } => {
            let pane_id = normalize_token(pane_id);
            if pane_id.is_empty() {
                return unsupported_plan("tmux pane id is required");
            }
            let scrollback = request.include_scrollback_lines.clamp(0, 10_000);
            let mut commands = vec![format!(
                "tmux capture-pane -e -p -t '{}' -S -{}",
                shell_single_quote(&pane_id),
                scrollback
            )];
            if request.follow_stream {
                commands.push(format!(
                    "tmux pipe-pane -o -t '{}' 'cat'",
                    shell_single_quote(&pane_id)
                ));
            }
            RawPtyAttachPlan {
                supported: true,
                keypress_count: 1,
                commands,
                detach_command: Some(format!(
                    "tmux pipe-pane -t '{}'",
                    shell_single_quote(&pane_id)
                )),
                fallback_message: String::new(),
            }
        }
        RawPtyTransport::LocalSocket { socket_path } => {
            let socket_path = socket_path.trim();
            if socket_path.is_empty() {
                return unsupported_plan("local PTY socket path is required");
            }
            let mut args = vec![format!(
                "forge pty attach --agent '{}' --socket '{}'",
                shell_single_quote(&agent_id),
                shell_single_quote(socket_path)
            )];
            if request.follow_stream {
                args.push("--follow".to_owned());
            }
            RawPtyAttachPlan {
                supported: true,
                keypress_count: 1,
                commands: vec![args.join(" ")],
                detach_command: Some("forge pty detach".to_owned()),
                fallback_message: String::new(),
            }
        }
    }
}

pub fn begin_raw_pty_session(
    state: &mut RawPtySessionState,
    request: &RawPtyAttachRequest,
) -> Result<RawPtyAttachPlan, String> {
    let plan = build_raw_pty_attach_plan(request);
    if !plan.supported {
        return Err(plan.fallback_message.clone());
    }
    state.attached_agent_id = Some(normalize_token(&request.agent_id));
    state.transport = Some(request.transport.clone());
    state.follow_stream = request.follow_stream;
    state.raw_bytes.clear();
    state.last_sequence = 0;
    Ok(plan)
}

pub fn detach_raw_pty_session(state: &mut RawPtySessionState) {
    state.attached_agent_id = None;
    state.transport = None;
    state.follow_stream = false;
    state.raw_bytes.clear();
    state.last_sequence = 0;
}

pub fn ingest_raw_pty_chunk(state: &mut RawPtySessionState, sequence: u64, chunk: &[u8]) -> bool {
    if state.attached_agent_id.is_none() || chunk.is_empty() || sequence <= state.last_sequence {
        return false;
    }
    state.last_sequence = sequence;
    state.raw_bytes.extend_from_slice(chunk);
    if state.raw_bytes.len() > state.max_bytes {
        let drop_len = state.raw_bytes.len().saturating_sub(state.max_bytes);
        state.raw_bytes.drain(0..drop_len);
    }
    true
}

#[must_use]
pub fn render_raw_pty_overlay(
    state: &RawPtySessionState,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }

    let mut rows = Vec::with_capacity(max_rows);
    let header = if let Some(agent_id) = state.attached_agent_id.as_deref() {
        format!(
            "raw-pty {} agent={} bytes={} seq={}",
            if state.follow_stream {
                "attached"
            } else {
                "snapshot"
            },
            agent_id,
            state.raw_bytes.len(),
            state.last_sequence
        )
    } else {
        "raw-pty detached".to_owned()
    };
    rows.push(fit_width(&header, width));

    if rows.len() >= max_rows {
        return rows;
    }

    if state.raw_bytes.is_empty() {
        rows.push(fit_width("(no terminal stream data)", width));
        rows.truncate(max_rows);
        return rows;
    }

    let text = sanitize_raw_bytes(&state.raw_bytes);
    let lines = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let tail_capacity = max_rows.saturating_sub(rows.len());
    let tail_start = lines.len().saturating_sub(tail_capacity);
    for line in lines.iter().skip(tail_start) {
        rows.push(fit_width(line, width));
    }

    rows.truncate(max_rows);
    rows
}

fn unsupported_plan(message: &str) -> RawPtyAttachPlan {
    RawPtyAttachPlan {
        supported: false,
        keypress_count: 0,
        commands: Vec::new(),
        detach_command: None,
        fallback_message: message.to_owned(),
    }
}

fn shell_single_quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "'\\''")
}

fn normalize_token(value: &str) -> String {
    value.trim().to_owned()
}

fn fit_width(text: &str, width: usize) -> String {
    if text.len() <= width {
        return text.to_owned();
    }
    text.chars().take(width).collect()
}

fn sanitize_raw_bytes(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len());
    let mut escape = false;
    let mut csi = false;

    for byte in input {
        if csi {
            if (0x40..=0x7e).contains(byte) {
                csi = false;
            }
            continue;
        }
        if escape {
            escape = false;
            if *byte == b'[' {
                csi = true;
            }
            continue;
        }
        if *byte == 0x1b {
            escape = true;
            continue;
        }

        match *byte {
            b'\n' => out.push('\n'),
            b'\r' => {}
            b'\t' => out.push('\t'),
            0x20..=0x7e => out.push(*byte as char),
            _ => out.push(' '),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{
        begin_raw_pty_session, build_raw_pty_attach_plan, detach_raw_pty_session,
        ingest_raw_pty_chunk, render_raw_pty_overlay, RawPtyAttachRequest, RawPtySessionState,
        RawPtyTransport,
    };

    #[test]
    fn tmux_plan_includes_capture_and_follow_commands() {
        let plan = build_raw_pty_attach_plan(&RawPtyAttachRequest {
            agent_id: "agent-a".to_owned(),
            transport: RawPtyTransport::TmuxPane {
                pane_id: "%12".to_owned(),
            },
            include_scrollback_lines: 500,
            follow_stream: true,
        });

        assert!(plan.supported);
        assert_eq!(plan.keypress_count, 1);
        assert_eq!(plan.commands.len(), 2);
        assert!(plan.commands[0].contains("capture-pane"));
        assert!(plan.commands[1].contains("pipe-pane"));
        let detach = match plan.detach_command.as_deref() {
            Some(detach) => detach,
            None => panic!("detach command should be present"),
        };
        assert!(detach.contains("pipe-pane"));
    }

    #[test]
    fn empty_tmux_pane_is_rejected() {
        let plan = build_raw_pty_attach_plan(&RawPtyAttachRequest {
            agent_id: "agent-a".to_owned(),
            transport: RawPtyTransport::TmuxPane {
                pane_id: " ".to_owned(),
            },
            include_scrollback_lines: 20,
            follow_stream: false,
        });

        assert!(!plan.supported);
        assert_eq!(plan.fallback_message, "tmux pane id is required");
    }

    #[test]
    fn session_ingest_requires_monotonic_sequence() {
        let mut state = RawPtySessionState::default();
        begin_raw_pty_session(
            &mut state,
            &RawPtyAttachRequest {
                agent_id: "agent-a".to_owned(),
                transport: RawPtyTransport::TmuxPane {
                    pane_id: "%9".to_owned(),
                },
                include_scrollback_lines: 80,
                follow_stream: true,
            },
        )
        .unwrap_or_else(|err| panic!("attach should succeed: {err}"));

        assert!(ingest_raw_pty_chunk(&mut state, 1, b"one"));
        assert!(!ingest_raw_pty_chunk(&mut state, 1, b"duplicate"));
        assert!(!ingest_raw_pty_chunk(&mut state, 0, b"older"));
        assert!(ingest_raw_pty_chunk(&mut state, 2, b"two"));
        assert_eq!(state.last_sequence, 2);
    }

    #[test]
    fn session_buffer_is_trimmed_to_max_bytes() {
        let mut state = RawPtySessionState {
            max_bytes: 6,
            ..RawPtySessionState::default()
        };
        begin_raw_pty_session(
            &mut state,
            &RawPtyAttachRequest {
                agent_id: "agent-a".to_owned(),
                transport: RawPtyTransport::TmuxPane {
                    pane_id: "%9".to_owned(),
                },
                include_scrollback_lines: 80,
                follow_stream: true,
            },
        )
        .unwrap_or_else(|err| panic!("attach should succeed: {err}"));

        assert!(ingest_raw_pty_chunk(&mut state, 1, b"1234"));
        assert!(ingest_raw_pty_chunk(&mut state, 2, b"5678"));
        assert_eq!(state.raw_bytes, b"345678");
    }

    #[test]
    fn render_overlay_sanitizes_ansi_and_shows_tail() {
        let mut state = RawPtySessionState::default();
        begin_raw_pty_session(
            &mut state,
            &RawPtyAttachRequest {
                agent_id: "agent-a".to_owned(),
                transport: RawPtyTransport::TmuxPane {
                    pane_id: "%9".to_owned(),
                },
                include_scrollback_lines: 80,
                follow_stream: true,
            },
        )
        .unwrap_or_else(|err| panic!("attach should succeed: {err}"));

        ingest_raw_pty_chunk(&mut state, 1, b"prep\n\x1b[31mERR\x1b[0m\nok\n");
        let rows = render_raw_pty_overlay(&state, 40, 4);
        assert_eq!(rows.len(), 4);
        assert!(rows[0].contains("raw-pty attached"));
        assert_eq!(rows[1], "prep");
        assert_eq!(rows[2], "ERR");
        assert_eq!(rows[3], "ok");
    }

    #[test]
    fn detach_clears_session() {
        let mut state = RawPtySessionState::default();
        begin_raw_pty_session(
            &mut state,
            &RawPtyAttachRequest {
                agent_id: "agent-a".to_owned(),
                transport: RawPtyTransport::LocalSocket {
                    socket_path: "/tmp/forge.sock".to_owned(),
                },
                include_scrollback_lines: 0,
                follow_stream: false,
            },
        )
        .unwrap_or_else(|err| panic!("attach should succeed: {err}"));
        assert!(ingest_raw_pty_chunk(&mut state, 1, b"hello"));

        detach_raw_pty_session(&mut state);
        let rows = render_raw_pty_overlay(&state, 50, 2);
        assert_eq!(rows[0], "raw-pty detached");
        assert_eq!(rows[1], "(no terminal stream data)");
    }
}
