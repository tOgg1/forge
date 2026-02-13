use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CorpusSpec {
    pattern_minimum: usize,
    files: BTreeMap<String, Vec<TokenSpan>>,
}

#[derive(Debug, Deserialize)]
struct TokenSpan {
    start: usize,
    end: usize,
    class: String,
}

#[test]
fn corpus_pack_meets_baseline_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/log_highlighting_corpus");
    let spec = load_spec(&root);

    let mut distinct_patterns = BTreeSet::new();
    let mut unknown_lines = Vec::new();
    let mut observed_classes = BTreeSet::new();

    for (file_name, spans) in &spec.files {
        let path = root.join(file_name);
        let content = match std::fs::read_to_string(&path) {
            Ok(value) => value,
            Err(err) => panic!("read fixture {}: {err}", path.display()),
        };
        let lines: Vec<&str> = content.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let classes = classify_line(line);
            for class in &classes {
                observed_classes.insert(*class);
            }
            if classes.contains("unknown") {
                unknown_lines.push(format!("{}:{}: {}", file_name, idx + 1, line));
            }
            if !line.trim().is_empty() {
                distinct_patterns.insert(pattern_signature(line));
            }
        }

        for span in spans {
            assert!(span.start >= 1, "{} span start must be >= 1", file_name);
            assert!(
                span.end >= span.start,
                "{} span end must be >= start ({})",
                file_name,
                span.class
            );
            assert!(
                span.end <= lines.len(),
                "{} span {}-{} out of range ({} lines)",
                file_name,
                span.start,
                span.end,
                lines.len()
            );
            for line_no in span.start..=span.end {
                let classes = classify_line(lines[line_no - 1]);
                assert!(
                    classes.contains(span.class.as_str()),
                    "{}:{} missing class '{}' (got {:?})",
                    file_name,
                    line_no,
                    span.class,
                    classes
                );
            }
        }
    }

    assert!(
        distinct_patterns.len() >= spec.pattern_minimum,
        "distinct pattern count {} < required {}",
        distinct_patterns.len(),
        spec.pattern_minimum
    );
    assert!(
        unknown_lines.is_empty(),
        "baseline scan found unknown classes:\n{}",
        unknown_lines.join("\n")
    );

    for required in [
        "success",
        "failure",
        "code_fence",
        "diff_add",
        "diff_del",
        "stack_frame",
        "tool_output",
    ] {
        assert!(
            observed_classes.contains(required),
            "required class '{}' missing from corpus",
            required
        );
    }
}

fn load_spec(root: &Path) -> CorpusSpec {
    let path = root.join("token_spans.json");
    let body = match std::fs::read_to_string(&path) {
        Ok(value) => value,
        Err(err) => panic!("read spec {}: {err}", path.display()),
    };
    match serde_json::from_str::<CorpusSpec>(&body) {
        Ok(value) => value,
        Err(err) => panic!("parse spec {}: {err}", path.display()),
    }
}

fn classify_line(line: &str) -> BTreeSet<&'static str> {
    let trimmed = line.trim();
    let mut classes = BTreeSet::new();
    if trimmed.is_empty() {
        return classes;
    }

    let lower = trimmed.to_ascii_lowercase();

    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        classes.insert("json_event");
    }

    if let Some(prefix) = timestamp_prefix(trimmed) {
        if looks_like_rfc3339(prefix) {
            classes.insert("timestamp");
        }
    }

    if matches!(
        trimmed,
        "thinking" | "user" | "assistant" | "system" | "codex" | "exec"
    ) {
        classes.insert("role_marker");
    }

    if trimmed.starts_with("OpenAI Codex")
        || trimmed.starts_with("OpenCode v")
        || trimmed.starts_with("Pi Coding Agent")
        || trimmed == "No models available."
    {
        classes.insert("harness_header");
    }

    if trimmed == "--------"
        || trimmed.starts_with('╭')
        || trimmed.starts_with('╰')
        || (trimmed.starts_with('│') && trimmed.ends_with('│'))
    {
        classes.insert("border");
    }

    if is_metadata_line(trimmed) {
        classes.insert("metadata");
    }

    if trimmed.starts_with("mcp:")
        || trimmed.starts_with("mcp startup:")
        || lower.starts_with("tool:")
        || lower.starts_with("action:")
        || trimmed.starts_with('●')
    {
        classes.insert("tool_output");
    }

    if trimmed.starts_with("**") && trimmed.ends_with("**") {
        classes.insert("emphasis");
    }

    if trimmed == "tokens used"
        || trimmed
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, ',' | '.'))
    {
        classes.insert("metric");
    }

    if trimmed.starts_with("```") || trimmed.contains("```") {
        classes.insert("code_fence");
    }

    if trimmed.starts_with("diff --git") || trimmed.starts_with("index ") {
        classes.insert("diff_header");
    }
    if trimmed.starts_with("@@") {
        classes.insert("diff_hunk");
    }
    if trimmed.starts_with("+++ ") || (trimmed.starts_with('+') && !trimmed.starts_with("++")) {
        classes.insert("diff_add");
    }
    if trimmed.starts_with("--- ") || (trimmed.starts_with('-') && !trimmed.starts_with("--")) {
        classes.insert("diff_del");
    }

    if lower.contains("warning") || lower.contains("approval required") || trimmed.starts_with("⚠")
    {
        classes.insert("warning");
    }

    if lower.contains("status: error")
        || lower.contains("\"is_error\":true")
        || lower.contains("exit_code: 1")
        || lower.contains("command failed")
    {
        classes.insert("failure");
    }

    if lower.contains("run complete")
        || lower.contains("\"subtype\":\"success\"")
        || lower.contains("approved by operator")
    {
        classes.insert("success");
    }

    if lower.contains("approval required")
        || lower.contains("approved by operator")
        || lower.contains("awaiting approval")
    {
        classes.insert("approval");
    }

    if lower.contains("error")
        || lower.contains("failed")
        || lower.contains("panic")
        || lower.contains("exception")
        || lower.contains("notfounderror")
    {
        classes.insert("error");
    }

    if is_numbered_item(trimmed) {
        classes.insert("numbered_item");
    }

    if looks_like_source_excerpt(trimmed) {
        classes.insert("source_excerpt");
    }

    if trimmed == "^" || trimmed.ends_with("^") {
        classes.insert("pointer");
    }

    if lower.starts_with('$')
        || lower.contains(" cargo test")
        || lower.contains(" go test")
        || lower.contains("run command")
        || lower.contains("anthropic_log=debug")
    {
        classes.insert("command");
    }

    if lower.contains("recovery:")
        || lower.starts_with("set an api key")
        || lower.starts_with("or create ")
    {
        classes.insert("recovery_hint");
    }

    if looks_like_stack_frame(trimmed) {
        classes.insert("stack_frame");
    }

    if contains_path_line(trimmed) {
        classes.insert("path_line");
    }

    if classes.is_empty() {
        if trimmed.chars().any(char::is_alphabetic) || is_code_punctuation(trimmed) {
            classes.insert("plain_text");
        } else {
            classes.insert("unknown");
        }
    }

    classes
}

fn is_code_punctuation(line: &str) -> bool {
    !line.is_empty()
        && line.chars().all(|ch| {
            matches!(
                ch,
                '{' | '}' | '(' | ')' | '[' | ']' | ';' | ',' | ' ' | '\t'
            )
        })
}

fn is_metadata_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let known_prefix = [
        "workdir:",
        "model:",
        "provider:",
        "approval:",
        "sandbox:",
        "reasoning effort:",
        "session id:",
        "profile:",
        "harness:",
        "status:",
        "exit_code:",
        "tool-output:",
        "data:",
    ];
    known_prefix.iter().any(|prefix| lower.starts_with(prefix))
}

fn is_numbered_item(line: &str) -> bool {
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_digit() {
        return false;
    }
    let Some(second) = chars.next() else {
        return false;
    };
    second == '.'
}

fn looks_like_source_excerpt(line: &str) -> bool {
    let mut parts = line.split('|');
    let Some(left) = parts.next() else {
        return false;
    };
    if parts.next().is_none() {
        return false;
    }
    left.trim().chars().all(|ch| ch.is_ascii_digit())
}

fn looks_like_stack_frame(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.contains("thread '") || lower.contains("panicked at") || lower.starts_with("at ") {
        return true;
    }
    line.contains(" at ") && contains_path_line(line)
}

fn contains_path_line(line: &str) -> bool {
    let path_ext = [".rs", ".go", ".ts", ".sh", ".json", ".yaml", ".toml", ".py"];
    let has_path_ext = path_ext.iter().any(|ext| line.contains(ext));
    if !has_path_ext {
        return false;
    }

    for token in line.split([' ', '\t', ',', ')', '(', '`']) {
        if token.is_empty() {
            continue;
        }
        let parts: Vec<&str> = token.split(':').collect();
        if parts.len() >= 2 {
            let numeric_suffix = parts.iter().skip(1).all(|segment| {
                !segment.is_empty() && segment.chars().all(|ch| ch.is_ascii_digit())
            });
            if numeric_suffix && parts[0].contains('/') {
                return true;
            }
        }
    }

    line.contains('/')
}

fn timestamp_prefix(line: &str) -> Option<&str> {
    if !line.starts_with('[') {
        return None;
    }
    let end = line.find(']')?;
    Some(&line[1..end])
}

fn looks_like_rfc3339(value: &str) -> bool {
    value.len() >= 20
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.contains('T')
        && value.ends_with('Z')
}

fn pattern_signature(line: &str) -> String {
    let mut out = String::new();
    let mut saw_digit = false;
    let mut saw_space = false;

    for ch in line.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_digit() {
            if !saw_digit {
                out.push('#');
            }
            saw_digit = true;
            saw_space = false;
            continue;
        }

        saw_digit = false;
        if ch.is_whitespace() {
            if !saw_space {
                out.push(' ');
            }
            saw_space = true;
            continue;
        }

        saw_space = false;
        out.push(ch);
    }

    out.trim().to_string()
}
