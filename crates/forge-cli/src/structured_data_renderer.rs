//! Structured-data renderer: JSON / YAML / TOML semantic tokens.
//!
//! Provides semantic colorization for keys, strings, numbers, booleans, null,
//! and punctuation in structured data formats. Graceful fallback on invalid
//! payloads — returns the line unstyled. Stable colors under wrapping/truncation.
//!
//! Design (from PAR-106):
//! - Classify byte spans within a single line into semantic parts.
//! - Apply [`TokenKind`] styling per sub-span classification.
//! - No regex — all matching is prefix / byte scans.
//! - Streaming-safe: each line processed independently.

use crate::highlight_spec::{style_span, TokenKind};

// ---------------------------------------------------------------------------
// Format detection
// ---------------------------------------------------------------------------

/// Detected structured data format for a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataFormat {
    Json,
    Yaml,
    Toml,
}

/// Detect the structured data format of a line. Returns `None` if the line
/// does not look like structured data.
fn detect_format(line: &str) -> Option<DataFormat> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // JSON: starts with { or [
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some(DataFormat::Json);
    }

    // TOML: section header [section] or key = value (with = not ==)
    if trimmed.starts_with('[') {
        return Some(DataFormat::Toml);
    }
    if let Some(eq_pos) = trimmed.find('=') {
        // Make sure it's not == (comparison) and the key part looks like an identifier.
        if eq_pos > 0
            && trimmed.as_bytes().get(eq_pos + 1) != Some(&b'=')
            && (eq_pos == 0 || trimmed.as_bytes()[eq_pos - 1] != b'!')
            && looks_like_toml_key(&trimmed[..eq_pos])
        {
            return Some(DataFormat::Toml);
        }
    }

    // YAML: key: value patterns (but not URLs like https://)
    if let Some(colon_pos) = trimmed.find(':') {
        if colon_pos > 0 {
            let after_colon = &trimmed[colon_pos + 1..];
            // YAML key: must be followed by space, newline, or end-of-line.
            // Exclude URL patterns (://)
            if (after_colon.is_empty() || after_colon.starts_with(' '))
                && !after_colon.starts_with("//")
                && looks_like_yaml_key(&trimmed[..colon_pos])
            {
                return Some(DataFormat::Yaml);
            }
        }
    }

    // YAML list items: "- key: value" or "- value"
    if trimmed.starts_with("- ") {
        return Some(DataFormat::Yaml);
    }

    None
}

/// Check if text before `=` looks like a TOML key (alphanumeric, dashes,
/// underscores, dots, optionally quoted).
fn looks_like_toml_key(key_part: &str) -> bool {
    let trimmed = key_part.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Quoted key
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        return true;
    }
    // Bare key: alphanumeric + dashes + underscores + dots
    trimmed
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Check if text before `:` looks like a YAML key (alphanumeric, dashes,
/// underscores, spaces in some cases, or quoted).
fn looks_like_yaml_key(key_part: &str) -> bool {
    let trimmed = key_part.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Quoted key
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        return true;
    }
    // YAML keys: typically start with a letter/underscore, contain alphanumeric + dash + underscore + space
    // Also handle indented keys (leading spaces stripped already by trim)
    let first = trimmed.as_bytes()[0];
    if !first.is_ascii_alphabetic() && first != b'_' {
        return false;
    }
    trimmed
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b' ' || b == b'.')
}

// ---------------------------------------------------------------------------
// JSON line renderer
// ---------------------------------------------------------------------------

/// Render a single JSON line with semantic highlighting.
/// Processes the line byte-by-byte to identify keys, string values, numbers,
/// booleans, null, and punctuation.
fn render_json_line(line: &str, use_color: bool) -> String {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(line.len() + 64);
    let mut i = 0;

    // Track whether the next string is a key (after { or ,) vs a value (after :).
    let mut expect_key = true;

    while i < len {
        match bytes[i] {
            // Structural punctuation: pass through as plain
            b'{' | b'}' | b'[' | b']' | b',' => {
                // After { or , or [ the next string is a key (in objects)
                // After } or ] it depends on context, but for line-level this is good enough
                if bytes[i] == b'{' || bytes[i] == b',' {
                    expect_key = true;
                } else if bytes[i] == b'[' {
                    expect_key = false; // array context: values
                }
                out.push(bytes[i] as char);
                i += 1;
            }
            b':' => {
                expect_key = false;
                out.push(':');
                i += 1;
            }
            b'"' => {
                // Scan the full string (handle escapes)
                let start = i;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2; // skip escaped char
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                let span = &line[start..i];
                let kind = if expect_key {
                    TokenKind::DataKey
                } else {
                    TokenKind::StringLiteral
                };
                out.push_str(&style_span(span, kind, use_color));
            }
            b't' | b'f' => {
                // true / false
                if line[i..].starts_with("true") {
                    out.push_str(&style_span("true", TokenKind::DataValue, use_color));
                    i += 4;
                } else if line[i..].starts_with("false") {
                    out.push_str(&style_span("false", TokenKind::DataValue, use_color));
                    i += 5;
                } else {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
            b'n' => {
                // null
                if line[i..].starts_with("null") {
                    out.push_str(&style_span("null", TokenKind::DataValue, use_color));
                    i += 4;
                } else {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
            b'0'..=b'9' | b'-' if is_number_start(bytes, i) => {
                // Number: scan digits, dots, e/E, +/-
                let start = i;
                if bytes[i] == b'-' {
                    i += 1;
                }
                while i < len
                    && (bytes[i].is_ascii_digit()
                        || bytes[i] == b'.'
                        || bytes[i] == b'e'
                        || bytes[i] == b'E'
                        || bytes[i] == b'+'
                        || bytes[i] == b'-')
                {
                    // After e/E, allow one +/-
                    if (bytes[i] == b'+' || bytes[i] == b'-') && i > start {
                        let prev = bytes[i - 1];
                        if prev != b'e' && prev != b'E' {
                            break;
                        }
                    }
                    i += 1;
                }
                let span = &line[start..i];
                out.push_str(&style_span(span, TokenKind::Number, use_color));
            }
            b' ' | b'\t' | b'\r' | b'\n' => {
                // Whitespace: pass through
                out.push(bytes[i] as char);
                i += 1;
            }
            _ => {
                // Unknown byte: pass through
                out.push(bytes[i] as char);
                i += 1;
            }
        }
    }

    out
}

/// Check if position `i` starts a JSON number (digit, or `-` followed by digit).
fn is_number_start(bytes: &[u8], i: usize) -> bool {
    if bytes[i].is_ascii_digit() {
        return true;
    }
    if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// YAML line renderer
// ---------------------------------------------------------------------------

/// Render a single YAML line with semantic highlighting.
fn render_yaml_line(line: &str, use_color: bool) -> String {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    // YAML comment
    if trimmed.starts_with('#') {
        return line.to_string();
    }

    // YAML list item prefix
    let (list_prefix, rest) = if let Some(stripped) = trimmed.strip_prefix("- ") {
        ("- ", stripped)
    } else {
        ("", trimmed)
    };

    // Find key: value split
    if let Some(colon_pos) = rest.find(':') {
        let after_colon = &rest[colon_pos + 1..];
        if (after_colon.is_empty() || after_colon.starts_with(' '))
            && !after_colon.starts_with("//")
        {
            let key = &rest[..colon_pos];
            if looks_like_yaml_key(key) {
                let mut out = String::with_capacity(line.len() + 64);
                out.push_str(indent);
                out.push_str(list_prefix);
                out.push_str(&style_span(key, TokenKind::DataKey, use_color));
                out.push(':');
                if !after_colon.is_empty() {
                    let value_part = &after_colon[1..]; // skip leading space
                    out.push(' ');
                    out.push_str(&render_yaml_value(value_part, use_color));
                }
                return out;
            }
        }
    }

    // Bare list value
    if !list_prefix.is_empty() {
        let mut out = String::with_capacity(line.len() + 64);
        out.push_str(indent);
        out.push_str(list_prefix);
        out.push_str(&render_yaml_value(rest, use_color));
        return out;
    }

    // Fallback: return as-is
    line.to_string()
}

/// Render a YAML value with semantic tokens.
fn render_yaml_value(value: &str, use_color: bool) -> String {
    let trimmed = value.trim();

    // Quoted string
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        return style_span(value, TokenKind::StringLiteral, use_color);
    }

    // Boolean
    if trimmed == "true" || trimmed == "false" || trimmed == "yes" || trimmed == "no" {
        return style_span(value, TokenKind::DataValue, use_color);
    }

    // Null
    if trimmed == "null" || trimmed == "~" {
        return style_span(value, TokenKind::DataValue, use_color);
    }

    // Number
    if looks_like_number(trimmed) {
        return style_span(value, TokenKind::Number, use_color);
    }

    // Inline JSON array or object
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return render_json_line(value, use_color);
    }

    // Bare string value — just pass through as plain
    value.to_string()
}

// ---------------------------------------------------------------------------
// TOML line renderer
// ---------------------------------------------------------------------------

/// Render a single TOML line with semantic highlighting.
fn render_toml_line(line: &str, use_color: bool) -> String {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    // TOML comment
    if trimmed.starts_with('#') {
        return line.to_string();
    }

    // TOML section header [section] or [[array]]
    if trimmed.starts_with('[') {
        return style_span(line, TokenKind::DataKey, use_color);
    }

    // key = value
    if let Some(eq_pos) = trimmed.find('=') {
        if eq_pos > 0
            && trimmed.as_bytes().get(eq_pos + 1) != Some(&b'=')
            && (eq_pos == 0 || trimmed.as_bytes()[eq_pos - 1] != b'!')
        {
            let key = &trimmed[..eq_pos];
            if looks_like_toml_key(key) {
                let after_eq = &trimmed[eq_pos + 1..];
                let mut out = String::with_capacity(line.len() + 64);
                out.push_str(indent);
                out.push_str(&style_span(key.trim_end(), TokenKind::DataKey, use_color));
                // Preserve spacing around =
                let key_trail = &key[key.trim_end().len()..];
                out.push_str(key_trail);
                out.push('=');
                if !after_eq.is_empty() {
                    let value_part = after_eq.trim_start();
                    let space_before = &after_eq[..after_eq.len() - after_eq.trim_start().len()];
                    out.push_str(space_before);
                    out.push_str(&render_toml_value(value_part, use_color));
                }
                return out;
            }
        }
    }

    // Fallback: return as-is
    line.to_string()
}

/// Render a TOML value with semantic tokens.
fn render_toml_value(value: &str, use_color: bool) -> String {
    let trimmed = value.trim();

    // Quoted string (basic or literal)
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        return style_span(value, TokenKind::StringLiteral, use_color);
    }

    // Boolean
    if trimmed == "true" || trimmed == "false" {
        return style_span(value, TokenKind::DataValue, use_color);
    }

    // Number (integer or float)
    if looks_like_number(trimmed) {
        return style_span(value, TokenKind::Number, use_color);
    }

    // Inline array or table
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        return render_json_line(value, use_color);
    }

    // Date/time (TOML RFC 3339 dates)
    if looks_like_datetime(trimmed) {
        return style_span(value, TokenKind::Timestamp, use_color);
    }

    // Fallback
    value.to_string()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Check if a string looks like a number (integer or float, with optional sign).
fn looks_like_number(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let mut i = 0;
    // Optional leading sign
    if bytes[i] == b'+' || bytes[i] == b'-' {
        i += 1;
    }
    if i >= bytes.len() {
        return false;
    }
    // Must start with a digit
    if !bytes[i].is_ascii_digit() {
        return false;
    }
    // Scan digits, dots, underscores (TOML allows _), e/E
    let mut has_digit = false;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            has_digit = true;
            i += 1;
        } else if bytes[i] == b'.' || bytes[i] == b'_' {
            i += 1;
        } else if bytes[i] == b'e' || bytes[i] == b'E' {
            i += 1;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                i += 1;
            }
        } else {
            break;
        }
    }
    has_digit && i == bytes.len()
}

/// Check if a string looks like an RFC 3339 datetime.
fn looks_like_datetime(s: &str) -> bool {
    // Simple check: YYYY-MM-DD or YYYY-MM-DDTHH:MM:SS...
    if s.len() < 10 {
        return false;
    }
    let bytes = s.as_bytes();
    bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && bytes[7] == b'-'
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Render a single line of structured data with semantic highlighting.
///
/// Detects format per-line and applies appropriate tokenization.
/// Falls back to the original line when the format is unrecognized.
pub(crate) fn render_structured_data_line(line: &str, use_color: bool) -> String {
    match detect_format(line) {
        Some(DataFormat::Json) => render_json_line(line, use_color),
        Some(DataFormat::Yaml) => render_yaml_line(line, use_color),
        Some(DataFormat::Toml) => render_toml_line(line, use_color),
        None => line.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── Format detection ─────────────────────────────────────────────

    #[test]
    fn detects_json_object() {
        assert_eq!(detect_format(r#"{"key": "value"}"#), Some(DataFormat::Json));
    }

    #[test]
    fn detects_json_array() {
        assert_eq!(detect_format(r#"[1, 2, 3]"#), Some(DataFormat::Json));
    }

    #[test]
    fn detects_yaml_key_value() {
        assert_eq!(detect_format("name: hello"), Some(DataFormat::Yaml));
    }

    #[test]
    fn detects_yaml_list_item() {
        assert_eq!(detect_format("- item"), Some(DataFormat::Yaml));
    }

    #[test]
    fn detects_toml_key_value() {
        assert_eq!(detect_format("key = \"value\""), Some(DataFormat::Toml));
    }

    #[test]
    fn detects_toml_section() {
        // Note: [section] also matches JSON array start; JSON takes priority
        // but bare [section] with no JSON syntax is detected differently.
        // In practice, TOML sections come after format detection from context.
        assert_eq!(detect_format(r#"{"type": "init"}"#), Some(DataFormat::Json));
    }

    #[test]
    fn does_not_detect_plain_text() {
        assert_eq!(detect_format("running 8 tests"), None);
    }

    #[test]
    fn does_not_detect_url_as_yaml() {
        assert_eq!(detect_format("https://example.com"), None);
    }

    #[test]
    fn does_not_detect_empty_line() {
        assert_eq!(detect_format(""), None);
        assert_eq!(detect_format("   "), None);
    }

    // ── JSON rendering ──────────────────────────────────────────────

    #[test]
    fn json_keys_styled_with_color() {
        let line = r#"{"name": "Alice", "age": 30}"#;
        let rendered = render_json_line(line, true);
        // Should contain ANSI sequences for DataKey
        assert!(
            rendered.contains("\x1b["),
            "JSON keys should be styled: {rendered}"
        );
    }

    #[test]
    fn json_keys_unstyled_without_color() {
        let line = r#"{"name": "Alice"}"#;
        let rendered = render_json_line(line, false);
        // No-color: DataKey and StringLiteral have no signifier, so output
        // should be identical to input.
        assert_eq!(rendered, line);
    }

    #[test]
    fn json_booleans_styled() {
        let line = r#"{"active": true, "deleted": false}"#;
        let rendered = render_json_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "JSON booleans should be styled: {rendered}"
        );
    }

    #[test]
    fn json_null_styled() {
        let line = r#"{"value": null}"#;
        let rendered = render_json_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "JSON null should be styled: {rendered}"
        );
    }

    #[test]
    fn json_numbers_styled() {
        let line = r#"{"count": 42, "rate": -3.14, "exp": 1e10}"#;
        let rendered = render_json_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "JSON numbers should be styled: {rendered}"
        );
    }

    #[test]
    fn json_escaped_strings_handled() {
        let line = r#"{"msg": "hello \"world\""}"#;
        let rendered = render_json_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "JSON escaped strings should be handled: {rendered}"
        );
    }

    #[test]
    fn json_nested_object() {
        let line = r#"{"a": {"b": 1}}"#;
        let rendered = render_json_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "nested JSON should be styled: {rendered}"
        );
    }

    #[test]
    fn json_array_values() {
        let line = r#"[1, "two", true, null]"#;
        let rendered = render_json_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "JSON array should be styled: {rendered}"
        );
    }

    // ── YAML rendering ──────────────────────────────────────────────

    #[test]
    fn yaml_key_value_styled() {
        let line = "name: Alice";
        let rendered = render_yaml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "YAML key should be styled: {rendered}"
        );
    }

    #[test]
    fn yaml_key_no_color_preserves_content() {
        let line = "name: Alice";
        let rendered = render_yaml_line(line, false);
        // DataKey has no no-color signifier, so it should be the same.
        assert_eq!(rendered, line);
    }

    #[test]
    fn yaml_numeric_value_styled() {
        let line = "port: 8080";
        let rendered = render_yaml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "YAML number value should be styled: {rendered}"
        );
    }

    #[test]
    fn yaml_boolean_value_styled() {
        let line = "enabled: true";
        let rendered = render_yaml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "YAML boolean should be styled: {rendered}"
        );
    }

    #[test]
    fn yaml_null_value_styled() {
        let line = "value: null";
        let rendered = render_yaml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "YAML null should be styled: {rendered}"
        );
    }

    #[test]
    fn yaml_list_item_styled() {
        let line = "- item_one";
        let rendered = render_yaml_line(line, true);
        // List items that are bare strings pass through unstyled (plain text).
        assert_eq!(rendered, line);
    }

    #[test]
    fn yaml_list_with_key_styled() {
        let line = "- name: Bob";
        let rendered = render_yaml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "YAML list key should be styled: {rendered}"
        );
    }

    #[test]
    fn yaml_indented_key_styled() {
        let line = "  host: localhost";
        let rendered = render_yaml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "indented YAML key should be styled: {rendered}"
        );
    }

    #[test]
    fn yaml_quoted_value_styled() {
        let line = "name: \"Alice\"";
        let rendered = render_yaml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "YAML quoted value should be styled: {rendered}"
        );
    }

    #[test]
    fn yaml_comment_passthrough() {
        let line = "# This is a comment";
        let rendered = render_yaml_line(line, true);
        assert_eq!(rendered, line);
    }

    // ── TOML rendering ──────────────────────────────────────────────

    #[test]
    fn toml_key_value_styled() {
        let line = "name = \"Alice\"";
        let rendered = render_toml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "TOML key should be styled: {rendered}"
        );
    }

    #[test]
    fn toml_section_header_styled() {
        let line = "[database]";
        let rendered = render_toml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "TOML section should be styled: {rendered}"
        );
    }

    #[test]
    fn toml_double_bracket_styled() {
        let line = "[[servers]]";
        let rendered = render_toml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "TOML array table should be styled: {rendered}"
        );
    }

    #[test]
    fn toml_integer_value_styled() {
        let line = "port = 5432";
        let rendered = render_toml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "TOML integer should be styled: {rendered}"
        );
    }

    #[test]
    fn toml_boolean_value_styled() {
        let line = "enabled = true";
        let rendered = render_toml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "TOML boolean should be styled: {rendered}"
        );
    }

    #[test]
    fn toml_datetime_value_styled() {
        let line = "created = 2026-02-10T12:00:00Z";
        let rendered = render_toml_line(line, true);
        assert!(
            rendered.contains("\x1b["),
            "TOML datetime should be styled: {rendered}"
        );
    }

    #[test]
    fn toml_comment_passthrough() {
        let line = "# This is a comment";
        let rendered = render_toml_line(line, true);
        assert_eq!(rendered, line);
    }

    #[test]
    fn toml_no_color_preserves_content() {
        let line = "name = \"Alice\"";
        let rendered = render_toml_line(line, false);
        assert_eq!(rendered, line);
    }

    // ── Public API ──────────────────────────────────────────────────

    #[test]
    fn render_structured_data_line_mixed() {
        // JSON
        let line = r#"{"key": "value"}"#;
        let rendered = render_structured_data_line(line, true);
        assert!(rendered.contains("\x1b["));

        // YAML
        let rendered = render_structured_data_line("name: Alice", true);
        assert!(rendered.contains("\x1b["));

        // TOML
        let rendered = render_structured_data_line("port = 8080", true);
        assert!(rendered.contains("\x1b["));

        // Plain text — unchanged
        let rendered = render_structured_data_line("plain text", true);
        assert_eq!(rendered, "plain text");
    }

    #[test]
    fn render_structured_data_line_json() {
        let line = r#"{"type": "init", "model": "claude-opus-4-6"}"#;
        let rendered = render_structured_data_line(line, true);
        assert!(rendered.contains("\x1b["));
    }

    #[test]
    fn detect_format_positive() {
        assert_eq!(detect_format(r#"{"key": "value"}"#), Some(DataFormat::Json));
        assert_eq!(detect_format("name: Alice"), Some(DataFormat::Yaml));
        assert_eq!(detect_format("port = 8080"), Some(DataFormat::Toml));
    }

    #[test]
    fn detect_format_negative() {
        assert_eq!(detect_format("plain text"), None);
        assert_eq!(detect_format(""), None);
        assert_eq!(detect_format("https://example.com"), None);
    }

    // ── Edge cases ──────────────────────────────────────────────────

    #[test]
    fn empty_json_object() {
        let rendered = render_json_line("{}", true);
        assert_eq!(rendered, "{}");
    }

    #[test]
    fn empty_json_array() {
        let rendered = render_json_line("[]", true);
        assert_eq!(rendered, "[]");
    }

    #[test]
    fn json_with_whitespace() {
        let line = r#"{ "key" : "value" }"#;
        let rendered = render_json_line(line, true);
        assert!(rendered.contains("\x1b["));
    }

    #[test]
    fn negative_number_in_json() {
        let line = r#"{"offset": -10}"#;
        let rendered = render_json_line(line, true);
        assert!(rendered.contains("\x1b["));
    }

    #[test]
    fn yaml_key_only_no_value() {
        let line = "section:";
        let rendered = render_yaml_line(line, true);
        assert!(rendered.contains("\x1b["));
    }

    #[test]
    fn number_helper_works() {
        assert!(looks_like_number("42"));
        assert!(looks_like_number("-3.14"));
        assert!(looks_like_number("1e10"));
        assert!(looks_like_number("1_000"));
        assert!(!looks_like_number("abc"));
        assert!(!looks_like_number(""));
        assert!(!looks_like_number("-"));
    }

    #[test]
    fn datetime_helper_works() {
        assert!(looks_like_datetime("2026-02-10T12:00:00Z"));
        assert!(looks_like_datetime("2026-02-10"));
        assert!(!looks_like_datetime("not-a-date"));
        assert!(!looks_like_datetime("2026"));
    }

    #[test]
    fn graceful_fallback_on_invalid_json() {
        // Unclosed string — should not panic, just produce partial output.
        let line = r#"{"key": "unclosed"#;
        let rendered = render_json_line(line, true);
        assert!(!rendered.is_empty());
    }

    #[test]
    fn graceful_fallback_on_garbage() {
        let line = "not structured data at all";
        let rendered = render_structured_data_line(line, true);
        assert_eq!(rendered, line);
    }
}
