//! Multi-lane log model for heterogeneous harness output.
//!
//! Normalizes lines from different harness types (Codex, Claude, Opencode, Pi)
//! into a unified lane model. Each line retains raw fidelity while being tagged
//! with a semantic lane for filtering and styling.
//!
//! Lanes: `Thinking`, `Tool`, `Stdout`, `Stderr`, `Event`, `Unknown`.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// LogLane — semantic lane classification
// ---------------------------------------------------------------------------

/// Semantic lane for a single log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogLane {
    /// Model thinking / reasoning output.
    Thinking,
    /// Tool invocation or tool result output.
    Tool,
    /// Standard output from the harness process.
    Stdout,
    /// Standard error from the harness process.
    Stderr,
    /// Lifecycle events (start, stop, error, status changes).
    Event,
    /// Unclassified line (raw passthrough).
    Unknown,
}

impl LogLane {
    /// All lanes in display order.
    pub const ALL: [LogLane; 6] = [
        LogLane::Thinking,
        LogLane::Tool,
        LogLane::Stdout,
        LogLane::Stderr,
        LogLane::Event,
        LogLane::Unknown,
    ];

    /// Human-readable label for the lane.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Thinking => "thinking",
            Self::Tool => "tool",
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::Event => "event",
            Self::Unknown => "unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// LanedLogLine — a single classified log line
// ---------------------------------------------------------------------------

/// A single log line tagged with its semantic lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanedLogLine {
    /// The raw text of the line (preserved exactly as received).
    pub text: String,
    /// Semantic lane classification.
    pub lane: LogLane,
    /// Zero-based index in the original stream (insertion order).
    pub index: usize,
}

// ---------------------------------------------------------------------------
// LanedLogModel — the multi-lane log container
// ---------------------------------------------------------------------------

/// Multi-lane log model that holds all lines with lane tags.
///
/// Supports appending new lines, bulk loading, lane-based filtering,
/// and lane-count statistics. Raw line ordering is always preserved.
#[derive(Debug, Clone, Default)]
pub struct LanedLogModel {
    lines: Vec<LanedLogLine>,
}

impl LanedLogModel {
    /// Create an empty model.
    #[must_use]
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Create a model from pre-classified lines.
    #[must_use]
    pub fn from_lines(lines: Vec<LanedLogLine>) -> Self {
        Self { lines }
    }

    /// Append a single classified line. Returns the assigned index.
    pub fn push(&mut self, text: String, lane: LogLane) -> usize {
        let index = self.lines.len();
        self.lines.push(LanedLogLine { text, lane, index });
        index
    }

    /// Total number of lines across all lanes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the model contains no lines.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// All lines in insertion order.
    #[must_use]
    pub fn all_lines(&self) -> &[LanedLogLine] {
        &self.lines
    }

    /// Filter lines to a single lane, preserving insertion order.
    #[must_use]
    pub fn lines_for_lane(&self, lane: LogLane) -> Vec<&LanedLogLine> {
        self.lines.iter().filter(|l| l.lane == lane).collect()
    }

    /// Filter lines to a set of lanes, preserving insertion order.
    #[must_use]
    pub fn lines_for_lanes(&self, lanes: &[LogLane]) -> Vec<&LanedLogLine> {
        self.lines
            .iter()
            .filter(|l| lanes.contains(&l.lane))
            .collect()
    }

    /// Extract just the text of all lines (raw fidelity export).
    #[must_use]
    pub fn raw_texts(&self) -> Vec<&str> {
        self.lines.iter().map(|l| l.text.as_str()).collect()
    }

    /// Extract just the text of lines matching a lane.
    #[must_use]
    pub fn texts_for_lane(&self, lane: LogLane) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| l.lane == lane)
            .map(|l| l.text.as_str())
            .collect()
    }

    /// Per-lane line counts.
    #[must_use]
    pub fn lane_counts(&self) -> HashMap<LogLane, usize> {
        let mut counts = HashMap::new();
        for line in &self.lines {
            *counts.entry(line.lane).or_insert(0) += 1;
        }
        counts
    }

    /// Clear all lines.
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

// ---------------------------------------------------------------------------
// classify_line — heuristic lane classifier
// ---------------------------------------------------------------------------

/// Classify a raw log line into a semantic lane using format heuristics.
///
/// The classifier recognizes common patterns from Codex, Claude, Opencode,
/// and Pi harness outputs. Lines that don't match any known pattern are
/// classified as `Unknown`.
#[must_use]
pub fn classify_line(line: &str) -> LogLane {
    let trimmed = line.trim();

    // Empty lines are unknown.
    if trimmed.is_empty() {
        return LogLane::Unknown;
    }

    // Event markers: lifecycle events, status transitions, timestamps with
    // brackets like "[2026-02-10T...] starting", "[EVENT]", etc.
    if trimmed.starts_with("[EVENT]")
        || trimmed.starts_with("[event]")
        || trimmed.starts_with("[STATUS]")
        || trimmed.starts_with("[status]")
        || trimmed.starts_with("[START]")
        || trimmed.starts_with("[STOP]")
        || trimmed.starts_with("[ERROR]")
        || trimmed.starts_with("[DONE]")
    {
        return LogLane::Event;
    }

    // Stderr markers: common error/warning prefixes.
    if trimmed.starts_with("STDERR:")
        || trimmed.starts_with("stderr:")
        || trimmed.starts_with("Error:")
        || trimmed.starts_with("error:")
        || trimmed.starts_with("Warning:")
        || trimmed.starts_with("warning:")
        || trimmed.starts_with("WARN:")
        || trimmed.starts_with("ERR:")
        || trimmed.starts_with("panic:")
        || trimmed.starts_with("thread '")
    {
        return LogLane::Stderr;
    }

    // Tool markers: tool calls, function invocations, file operations.
    if trimmed.starts_with("Tool:")
        || trimmed.starts_with("tool:")
        || trimmed.starts_with("TOOL:")
        || trimmed.starts_with(">>> tool")
        || trimmed.starts_with("<<< tool")
        || trimmed.starts_with("Running:")
        || trimmed.starts_with("Executing:")
        || trimmed.starts_with("$ ")
    {
        return LogLane::Tool;
    }

    // Thinking markers: reasoning, chain-of-thought.
    if trimmed.starts_with("Thinking:")
        || trimmed.starts_with("thinking:")
        || trimmed.starts_with("THINKING:")
        || trimmed.starts_with(">>> thinking")
        || trimmed.starts_with("<<< thinking")
        || trimmed.starts_with("Reasoning:")
        || trimmed.starts_with("> ")
    {
        return LogLane::Thinking;
    }

    // Stdout markers.
    if trimmed.starts_with("STDOUT:") || trimmed.starts_with("stdout:") {
        return LogLane::Stdout;
    }

    // Default: treat as stdout (the most common lane for undecorated output).
    LogLane::Stdout
}

/// Bulk-classify a slice of raw lines into a `LanedLogModel`.
#[must_use]
pub fn classify_lines(lines: &[String]) -> LanedLogModel {
    let classified: Vec<LanedLogLine> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| LanedLogLine {
            text: line.clone(),
            lane: classify_line(line),
            index: i,
        })
        .collect();
    LanedLogModel::from_lines(classified)
}

// ---------------------------------------------------------------------------
// LogLayer → LogLane mapping
// ---------------------------------------------------------------------------

/// Map the existing `LogLayer` filter to lane(s) for filtering.
///
/// This bridges the existing UI layer-cycling UX with the new lane model.
/// `Raw` maps to all lanes (no filter), while the others map to specific
/// lane subsets.
#[must_use]
pub fn lanes_for_layer(layer: crate::app::LogLayer) -> Vec<LogLane> {
    use crate::app::LogLayer;
    match layer {
        LogLayer::Raw => LogLane::ALL.to_vec(),
        LogLayer::Events => vec![LogLane::Event],
        LogLayer::Errors => vec![LogLane::Stderr],
        LogLayer::Tools => vec![LogLane::Tool],
        LogLayer::Diff => vec![LogLane::Thinking, LogLane::Stdout],
    }
}

/// Filter a laned model by the active `LogLayer`, returning matching line texts.
#[must_use]
pub fn filter_by_layer(model: &LanedLogModel, layer: crate::app::LogLayer) -> Vec<&str> {
    let lanes = lanes_for_layer(layer);
    model
        .lines_for_lanes(&lanes)
        .into_iter()
        .map(|l| l.text.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::LogLayer;

    // -- LogLane basics --

    #[test]
    fn lane_labels_are_stable() {
        assert_eq!(LogLane::Thinking.label(), "thinking");
        assert_eq!(LogLane::Tool.label(), "tool");
        assert_eq!(LogLane::Stdout.label(), "stdout");
        assert_eq!(LogLane::Stderr.label(), "stderr");
        assert_eq!(LogLane::Event.label(), "event");
        assert_eq!(LogLane::Unknown.label(), "unknown");
    }

    #[test]
    fn all_lanes_count() {
        assert_eq!(LogLane::ALL.len(), 6);
    }

    // -- LanedLogModel basics --

    #[test]
    fn empty_model() {
        let model = LanedLogModel::new();
        assert!(model.is_empty());
        assert_eq!(model.len(), 0);
        assert!(model.all_lines().is_empty());
    }

    #[test]
    fn push_and_retrieve() {
        let mut model = LanedLogModel::new();
        let idx0 = model.push("hello".to_owned(), LogLane::Stdout);
        let idx1 = model.push("error!".to_owned(), LogLane::Stderr);

        assert_eq!(idx0, 0);
        assert_eq!(idx1, 1);
        assert_eq!(model.len(), 2);
        assert_eq!(model.all_lines()[0].text, "hello");
        assert_eq!(model.all_lines()[0].lane, LogLane::Stdout);
        assert_eq!(model.all_lines()[1].text, "error!");
        assert_eq!(model.all_lines()[1].lane, LogLane::Stderr);
    }

    #[test]
    fn filter_by_single_lane() {
        let mut model = LanedLogModel::new();
        model.push("stdout1".to_owned(), LogLane::Stdout);
        model.push("event1".to_owned(), LogLane::Event);
        model.push("stdout2".to_owned(), LogLane::Stdout);
        model.push("tool1".to_owned(), LogLane::Tool);

        let stdout = model.lines_for_lane(LogLane::Stdout);
        assert_eq!(stdout.len(), 2);
        assert_eq!(stdout[0].text, "stdout1");
        assert_eq!(stdout[1].text, "stdout2");

        let events = model.lines_for_lane(LogLane::Event);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn filter_by_multiple_lanes() {
        let mut model = LanedLogModel::new();
        model.push("stdout1".to_owned(), LogLane::Stdout);
        model.push("event1".to_owned(), LogLane::Event);
        model.push("stderr1".to_owned(), LogLane::Stderr);
        model.push("tool1".to_owned(), LogLane::Tool);

        let result = model.lines_for_lanes(&[LogLane::Stdout, LogLane::Stderr]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "stdout1");
        assert_eq!(result[1].text, "stderr1");
    }

    #[test]
    fn raw_texts_preserves_order() {
        let mut model = LanedLogModel::new();
        model.push("line0".to_owned(), LogLane::Stdout);
        model.push("line1".to_owned(), LogLane::Stderr);
        model.push("line2".to_owned(), LogLane::Tool);

        let texts = model.raw_texts();
        assert_eq!(texts, vec!["line0", "line1", "line2"]);
    }

    #[test]
    fn texts_for_lane() {
        let mut model = LanedLogModel::new();
        model.push("t1".to_owned(), LogLane::Tool);
        model.push("s1".to_owned(), LogLane::Stdout);
        model.push("t2".to_owned(), LogLane::Tool);

        assert_eq!(model.texts_for_lane(LogLane::Tool), vec!["t1", "t2"]);
        assert_eq!(model.texts_for_lane(LogLane::Stdout), vec!["s1"]);
        assert!(model.texts_for_lane(LogLane::Thinking).is_empty());
    }

    #[test]
    fn lane_counts() {
        let mut model = LanedLogModel::new();
        model.push("a".to_owned(), LogLane::Stdout);
        model.push("b".to_owned(), LogLane::Stdout);
        model.push("c".to_owned(), LogLane::Stderr);
        model.push("d".to_owned(), LogLane::Tool);

        let counts = model.lane_counts();
        assert_eq!(counts.get(&LogLane::Stdout), Some(&2));
        assert_eq!(counts.get(&LogLane::Stderr), Some(&1));
        assert_eq!(counts.get(&LogLane::Tool), Some(&1));
        assert_eq!(counts.get(&LogLane::Thinking), None);
    }

    #[test]
    fn clear_empties_model() {
        let mut model = LanedLogModel::new();
        model.push("hello".to_owned(), LogLane::Stdout);
        assert!(!model.is_empty());
        model.clear();
        assert!(model.is_empty());
    }

    // -- classify_line heuristics --

    #[test]
    fn classify_empty() {
        assert_eq!(classify_line(""), LogLane::Unknown);
        assert_eq!(classify_line("   "), LogLane::Unknown);
    }

    #[test]
    fn classify_events() {
        assert_eq!(classify_line("[EVENT] loop started"), LogLane::Event);
        assert_eq!(classify_line("[event] connected"), LogLane::Event);
        assert_eq!(classify_line("[STATUS] running"), LogLane::Event);
        assert_eq!(classify_line("[START] harness"), LogLane::Event);
        assert_eq!(classify_line("[STOP] graceful"), LogLane::Event);
        assert_eq!(classify_line("[ERROR] timeout"), LogLane::Event);
        assert_eq!(classify_line("[DONE] exit 0"), LogLane::Event);
    }

    #[test]
    fn classify_stderr() {
        assert_eq!(classify_line("STDERR: oops"), LogLane::Stderr);
        assert_eq!(classify_line("stderr: bad input"), LogLane::Stderr);
        assert_eq!(classify_line("Error: file not found"), LogLane::Stderr);
        assert_eq!(classify_line("error: compile failed"), LogLane::Stderr);
        assert_eq!(classify_line("Warning: deprecated"), LogLane::Stderr);
        assert_eq!(classify_line("warning: unused var"), LogLane::Stderr);
        assert_eq!(classify_line("panic: oh no"), LogLane::Stderr);
        assert_eq!(classify_line("thread 'main' panicked at"), LogLane::Stderr);
    }

    #[test]
    fn classify_tool() {
        assert_eq!(classify_line("Tool: read_file"), LogLane::Tool);
        assert_eq!(classify_line("tool: bash"), LogLane::Tool);
        assert_eq!(classify_line("TOOL: write"), LogLane::Tool);
        assert_eq!(classify_line(">>> tool call: grep"), LogLane::Tool);
        assert_eq!(classify_line("<<< tool result: ok"), LogLane::Tool);
        assert_eq!(classify_line("Running: cargo test"), LogLane::Tool);
        assert_eq!(classify_line("Executing: npm install"), LogLane::Tool);
        assert_eq!(classify_line("$ ls -la"), LogLane::Tool);
    }

    #[test]
    fn classify_thinking() {
        assert_eq!(
            classify_line("Thinking: let me consider"),
            LogLane::Thinking
        );
        assert_eq!(classify_line("thinking: hmm"), LogLane::Thinking);
        assert_eq!(classify_line(">>> thinking"), LogLane::Thinking);
        assert_eq!(classify_line("<<< thinking done"), LogLane::Thinking);
        assert_eq!(classify_line("Reasoning: step 1"), LogLane::Thinking);
        assert_eq!(classify_line("> quoted reasoning"), LogLane::Thinking);
    }

    #[test]
    fn classify_stdout_explicit() {
        assert_eq!(classify_line("STDOUT: hello"), LogLane::Stdout);
        assert_eq!(classify_line("stdout: world"), LogLane::Stdout);
    }

    #[test]
    fn classify_undecorated_as_stdout() {
        assert_eq!(classify_line("just some output"), LogLane::Stdout);
        assert_eq!(classify_line("test passed"), LogLane::Stdout);
        assert_eq!(classify_line("fn main() {"), LogLane::Stdout);
    }

    // -- classify_lines bulk --

    #[test]
    fn classify_lines_bulk() {
        let lines = vec![
            "[EVENT] start".to_owned(),
            "hello world".to_owned(),
            "Error: bad".to_owned(),
            "Tool: grep".to_owned(),
            "".to_owned(),
        ];
        let model = classify_lines(&lines);
        assert_eq!(model.len(), 5);
        assert_eq!(model.all_lines()[0].lane, LogLane::Event);
        assert_eq!(model.all_lines()[1].lane, LogLane::Stdout);
        assert_eq!(model.all_lines()[2].lane, LogLane::Stderr);
        assert_eq!(model.all_lines()[3].lane, LogLane::Tool);
        assert_eq!(model.all_lines()[4].lane, LogLane::Unknown);
    }

    #[test]
    fn classify_lines_preserves_raw_text() {
        let lines = vec!["  [EVENT] start  ".to_owned()];
        let model = classify_lines(&lines);
        assert_eq!(model.all_lines()[0].text, "  [EVENT] start  ");
    }

    // -- LogLayer → lanes mapping --

    #[test]
    fn raw_layer_includes_all_lanes() {
        let lanes = lanes_for_layer(LogLayer::Raw);
        assert_eq!(lanes.len(), 6);
    }

    #[test]
    fn events_layer_maps_to_event_lane() {
        let lanes = lanes_for_layer(LogLayer::Events);
        assert_eq!(lanes, vec![LogLane::Event]);
    }

    #[test]
    fn errors_layer_maps_to_stderr_lane() {
        let lanes = lanes_for_layer(LogLayer::Errors);
        assert_eq!(lanes, vec![LogLane::Stderr]);
    }

    #[test]
    fn tools_layer_maps_to_tool_lane() {
        let lanes = lanes_for_layer(LogLayer::Tools);
        assert_eq!(lanes, vec![LogLane::Tool]);
    }

    #[test]
    fn diff_layer_maps_to_thinking_and_stdout() {
        let lanes = lanes_for_layer(LogLayer::Diff);
        assert_eq!(lanes, vec![LogLane::Thinking, LogLane::Stdout]);
    }

    // -- filter_by_layer integration --

    #[test]
    fn filter_by_layer_raw_returns_all() {
        let mut model = LanedLogModel::new();
        model.push("event".to_owned(), LogLane::Event);
        model.push("out".to_owned(), LogLane::Stdout);
        model.push("err".to_owned(), LogLane::Stderr);

        let result = filter_by_layer(&model, LogLayer::Raw);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn filter_by_layer_events_only() {
        let mut model = LanedLogModel::new();
        model.push("event".to_owned(), LogLane::Event);
        model.push("out".to_owned(), LogLane::Stdout);
        model.push("err".to_owned(), LogLane::Stderr);

        let result = filter_by_layer(&model, LogLayer::Events);
        assert_eq!(result, vec!["event"]);
    }

    #[test]
    fn filter_by_layer_errors_only() {
        let mut model = LanedLogModel::new();
        model.push("event".to_owned(), LogLane::Event);
        model.push("out".to_owned(), LogLane::Stdout);
        model.push("err".to_owned(), LogLane::Stderr);

        let result = filter_by_layer(&model, LogLayer::Errors);
        assert_eq!(result, vec!["err"]);
    }

    #[test]
    fn filter_by_layer_tools() {
        let lines = vec![
            "Tool: read_file".to_owned(),
            "just output".to_owned(),
            "$ cargo test".to_owned(),
        ];
        let model = classify_lines(&lines);
        let result = filter_by_layer(&model, LogLayer::Tools);
        assert_eq!(result, vec!["Tool: read_file", "$ cargo test"]);
    }

    // -- from_lines constructor --

    #[test]
    fn from_lines_preserves_order() {
        let lines = vec![
            LanedLogLine {
                text: "first".to_owned(),
                lane: LogLane::Stdout,
                index: 0,
            },
            LanedLogLine {
                text: "second".to_owned(),
                lane: LogLane::Stderr,
                index: 1,
            },
        ];
        let model = LanedLogModel::from_lines(lines);
        assert_eq!(model.len(), 2);
        assert_eq!(model.all_lines()[0].text, "first");
        assert_eq!(model.all_lines()[1].text, "second");
    }
}
