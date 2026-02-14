//! Session recording + replay core primitives.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedFrame {
    pub at_ms: u64,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedInput {
    pub at_ms: u64,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecording {
    pub id: String,
    pub started_at_epoch_s: i64,
    pub frames: Vec<RecordedFrame>,
    pub inputs: Vec<RecordedInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaySnapshot {
    pub at_ms: u64,
    pub frame_index: usize,
    pub frame_lines: Vec<String>,
    pub recent_actions: Vec<String>,
}

#[must_use]
pub fn start_session_recording(id: &str, started_at_epoch_s: i64) -> SessionRecording {
    SessionRecording {
        id: normalize_id(id),
        started_at_epoch_s: started_at_epoch_s.max(0),
        frames: Vec::new(),
        inputs: Vec::new(),
    }
}

pub fn append_recorded_frame(
    recording: &mut SessionRecording,
    at_ms: u64,
    lines: Vec<String>,
) -> Result<(), String> {
    if recording
        .frames
        .last()
        .is_some_and(|last| at_ms < last.at_ms)
    {
        return Err("frame timestamp must be monotonic".to_owned());
    }
    recording.frames.push(RecordedFrame { at_ms, lines });
    Ok(())
}

pub fn append_recorded_input(
    recording: &mut SessionRecording,
    at_ms: u64,
    action: &str,
) -> Result<(), String> {
    let action = action.trim();
    if action.is_empty() {
        return Err("recorded action cannot be empty".to_owned());
    }
    if recording
        .inputs
        .last()
        .is_some_and(|last| at_ms < last.at_ms)
    {
        return Err("input timestamp must be monotonic".to_owned());
    }
    recording.inputs.push(RecordedInput {
        at_ms,
        action: action.to_owned(),
    });
    Ok(())
}

pub fn compact_recording(recording: &mut SessionRecording) {
    let mut compacted = Vec::with_capacity(recording.frames.len());
    for frame in &recording.frames {
        if compacted
            .last()
            .is_some_and(|last: &RecordedFrame| last.lines == frame.lines)
        {
            continue;
        }
        compacted.push(frame.clone());
    }
    recording.frames = compacted;
}

#[must_use]
pub fn replay_snapshot_at(
    recording: &SessionRecording,
    at_ms: u64,
    recent_limit: usize,
) -> ReplaySnapshot {
    if recording.frames.is_empty() {
        return ReplaySnapshot {
            at_ms,
            frame_index: 0,
            frame_lines: vec!["<no recorded frames>".to_owned()],
            recent_actions: Vec::new(),
        };
    }

    let mut frame_index = 0usize;
    for (index, frame) in recording.frames.iter().enumerate() {
        if frame.at_ms <= at_ms {
            frame_index = index;
        } else {
            break;
        }
    }

    let frame_lines = recording.frames[frame_index].lines.clone();
    let mut recent_actions = recording
        .inputs
        .iter()
        .filter(|input| input.at_ms <= at_ms)
        .rev()
        .take(recent_limit.max(1))
        .map(|input| format!("{}ms {}", input.at_ms, input.action))
        .collect::<Vec<_>>();
    recent_actions.reverse();

    ReplaySnapshot {
        at_ms,
        frame_index,
        frame_lines,
        recent_actions,
    }
}

#[must_use]
pub fn render_replay_lines(
    snapshot: &ReplaySnapshot,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }
    let mut lines = vec![trim_line(
        &format!(
            "replay at={}ms frame#={} recent_actions={}",
            snapshot.at_ms,
            snapshot.frame_index,
            snapshot.recent_actions.len()
        ),
        width,
    )];
    for line in &snapshot.frame_lines {
        if lines.len() >= max_rows {
            break;
        }
        lines.push(trim_line(line, width));
    }
    for action in &snapshot.recent_actions {
        if lines.len() >= max_rows {
            break;
        }
        lines.push(trim_line(&format!("> {action}"), width));
    }
    lines
}

fn normalize_id(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "recording".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn trim_line(line: &str, width: usize) -> String {
    if line.chars().count() <= width {
        return line.to_owned();
    }
    line.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        append_recorded_frame, append_recorded_input, compact_recording, render_replay_lines,
        replay_snapshot_at, start_session_recording,
    };

    #[test]
    fn append_rejects_non_monotonic_timestamps() {
        let mut recording = start_session_recording("demo", 100);
        if let Err(err) = append_recorded_frame(&mut recording, 10, vec!["a".to_owned()]) {
            panic!("frame 1 should append: {err}");
        }
        let err = match append_recorded_frame(&mut recording, 9, vec!["b".to_owned()]) {
            Ok(_) => panic!("non-monotonic timestamp should fail"),
            Err(err) => err,
        };
        assert!(err.contains("monotonic"));
    }

    #[test]
    fn compact_recording_removes_duplicate_consecutive_frames() {
        let mut recording = start_session_recording("demo", 100);
        if let Err(err) = append_recorded_frame(&mut recording, 0, vec!["same".to_owned()]) {
            panic!("f0 should append: {err}");
        }
        if let Err(err) = append_recorded_frame(&mut recording, 1, vec!["same".to_owned()]) {
            panic!("f1 should append: {err}");
        }
        if let Err(err) = append_recorded_frame(&mut recording, 2, vec!["diff".to_owned()]) {
            panic!("f2 should append: {err}");
        }
        compact_recording(&mut recording);
        assert_eq!(recording.frames.len(), 2);
        assert_eq!(recording.frames[0].lines[0], "same");
        assert_eq!(recording.frames[1].lines[0], "diff");
    }

    #[test]
    fn replay_snapshot_selects_latest_frame_before_time() {
        let mut recording = start_session_recording("demo", 100);
        if let Err(err) = append_recorded_frame(&mut recording, 5, vec!["f0".to_owned()]) {
            panic!("f0 should append: {err}");
        }
        if let Err(err) = append_recorded_frame(&mut recording, 15, vec!["f1".to_owned()]) {
            panic!("f1 should append: {err}");
        }
        if let Err(err) = append_recorded_input(&mut recording, 6, "j") {
            panic!("input j should append: {err}");
        }
        if let Err(err) = append_recorded_input(&mut recording, 14, "k") {
            panic!("input k should append: {err}");
        }

        let snapshot = replay_snapshot_at(&recording, 12, 4);
        assert_eq!(snapshot.frame_index, 0);
        assert_eq!(snapshot.frame_lines[0], "f0");
        assert_eq!(snapshot.recent_actions, vec!["6ms j".to_owned()]);
    }

    #[test]
    fn replay_render_includes_header_frame_and_actions() {
        let mut recording = start_session_recording("demo", 100);
        if let Err(err) = append_recorded_frame(
            &mut recording,
            5,
            vec!["line-a".to_owned(), "line-b".to_owned()],
        ) {
            panic!("frame should append: {err}");
        }
        if let Err(err) = append_recorded_input(&mut recording, 5, "enter") {
            panic!("input should append: {err}");
        }

        let snapshot = replay_snapshot_at(&recording, 5, 4);
        let lines = render_replay_lines(&snapshot, 120, 8);
        assert!(lines[0].contains("replay at=5ms"));
        assert!(lines.iter().any(|line| line.contains("line-a")));
        assert!(lines.iter().any(|line| line.contains("> 5ms enter")));
    }
}
