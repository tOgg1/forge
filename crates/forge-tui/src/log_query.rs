//! Log query language for filtering and drill-down on `LanedLogLine` entries.
//!
//! ## Syntax
//!
//! | Form                  | Meaning                                      |
//! |-----------------------|----------------------------------------------|
//! | `word`                | Substring match on line text (case-insensitive) |
//! | `"exact phrase"`      | Quoted substring match (case-insensitive)     |
//! | `lane:tool`           | Lane equals `tool`                           |
//! | `text:substring`      | Explicit text substring match                |
//! | `text:/regex/`        | Regex match on text (with length guardrail)  |
//! | `index:>N`            | Index comparison (>, <, >=, <=, =)           |
//! | `NOT expr`            | Boolean negation                             |
//! | `expr AND expr`       | Boolean conjunction (also implicit)          |
//! | `expr OR expr`        | Boolean disjunction                          |
//! | `(expr)`              | Grouping                                     |
//!
//! Implicit `AND` when terms are juxtaposed without an operator.

use crate::lane_model::{LanedLogLine, LanedLogModel, LogLane};

// ---------------------------------------------------------------------------
// Query AST
// ---------------------------------------------------------------------------

/// A parsed log query expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogQuery {
    /// Substring match on line text (case-insensitive).
    TextContains(String),
    /// Lane equality filter.
    LaneEq(LogLane),
    /// Regex match on line text.
    TextRegex(String),
    /// Numeric comparison on line index.
    IndexCmp(CmpOp, usize),
    /// Boolean NOT.
    Not(Box<LogQuery>),
    /// Boolean AND.
    And(Box<LogQuery>, Box<LogQuery>),
    /// Boolean OR.
    Or(Box<LogQuery>, Box<LogQuery>),
}

/// Comparison operator for index filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Gt,
    Lt,
    Gte,
    Lte,
}

// ---------------------------------------------------------------------------
// Regex guardrails
// ---------------------------------------------------------------------------

/// Maximum allowed regex pattern length to prevent catastrophic backtracking.
const MAX_REGEX_LEN: usize = 256;

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// A bare word (unquoted).
    Word(String),
    /// A quoted string (content without quotes).
    Quoted(String),
    /// Field prefix like `lane:`, `text:`, `index:`.
    Field(String),
    /// Regex literal like `/pattern/`.
    Regex(String),
    /// Comparison operators with a number: `>5`, `<10`, `>=3`, `<=7`, `=2`.
    Cmp(CmpOp, usize),
    /// Boolean AND keyword.
    And,
    /// Boolean OR keyword.
    Or,
    /// Boolean NOT keyword.
    Not,
    /// Open parenthesis.
    LParen,
    /// Close parenthesis.
    RParen,
}

/// Error produced during parsing or evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    /// Human-readable error message.
    pub message: String,
    /// Byte offset into the input where the error was detected (if available).
    pub offset: Option<usize>,
    /// A hint for how to fix the issue.
    pub hint: Option<String>,
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(off) = self.offset {
            write!(f, " (at byte {off})")?;
        }
        if let Some(hint) = &self.hint {
            write!(f, " — hint: {hint}")?;
        }
        Ok(())
    }
}

fn tokenize(input: &str) -> Result<Vec<Token>, QueryError> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace.
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Parentheses.
        if bytes[i] == b'(' {
            tokens.push(Token::LParen);
            i += 1;
            continue;
        }
        if bytes[i] == b')' {
            tokens.push(Token::RParen);
            i += 1;
            continue;
        }

        // Quoted string.
        if bytes[i] == b'"' {
            i += 1; // skip opening quote
            let start = i;
            while i < len && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < len {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i >= len {
                return Err(QueryError {
                    message: "unterminated quoted string".to_owned(),
                    offset: Some(start.saturating_sub(1)),
                    hint: Some("close with a matching '\"'".to_owned()),
                });
            }
            let content = String::from_utf8_lossy(&bytes[start..i]).to_string();
            tokens.push(Token::Quoted(content));
            i += 1; // skip closing quote
            continue;
        }

        // Collect a word-like token (until whitespace, parens, or end).
        // Special case: if the word contains a field prefix followed by `/`, scan
        // for the closing `/` (regex literal can contain spaces).
        let word_start = i;
        while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b'(' && bytes[i] != b')' {
            i += 1;
        }
        let mut word = String::from_utf8_lossy(&bytes[word_start..i]).to_string();

        // Check if the word looks like `field:/regex-start` without a closing `/`.
        // If so, continue scanning past whitespace until we find the closing `/`.
        if let Some(colon_pos) = word.find(':') {
            let field_part = &word[..colon_pos];
            let value_part = &word[colon_pos + 1..];
            if (field_part.eq_ignore_ascii_case("text")
                || field_part.eq_ignore_ascii_case("lane")
                || field_part.eq_ignore_ascii_case("index"))
                && value_part.starts_with('/')
                && !value_part.ends_with('/')
            {
                // Scan for closing `/`.
                while i < len && bytes[i] != b'/' {
                    i += 1;
                }
                if i < len && bytes[i] == b'/' {
                    i += 1; // include closing `/`
                }
                word = String::from_utf8_lossy(&bytes[word_start..i]).to_string();
            }
        }

        // Check for field prefix (e.g. "lane:tool", "text:/regex/", "index:>5").
        if let Some(colon_pos) = word.find(':') {
            let field = word[..colon_pos].to_ascii_lowercase();
            let value = &word[colon_pos + 1..];

            if field == "lane" || field == "text" || field == "index" {
                // Push field token first.
                tokens.push(Token::Field(field.clone()));

                if value.is_empty() {
                    return Err(QueryError {
                        message: format!("field `{field}:` requires a value"),
                        offset: Some(word_start + colon_pos + 1),
                        hint: Some(format!("e.g. `{field}:tool` or `{field}:error`")),
                    });
                }

                // Regex literal: /pattern/
                if value.starts_with('/') && value.ends_with('/') && value.len() >= 2 {
                    let pattern = &value[1..value.len() - 1];
                    if pattern.len() > MAX_REGEX_LEN {
                        return Err(QueryError {
                            message: format!(
                                "regex pattern too long ({} chars, max {MAX_REGEX_LEN})",
                                pattern.len()
                            ),
                            offset: Some(word_start + colon_pos + 2),
                            hint: Some("simplify the pattern".to_owned()),
                        });
                    }
                    tokens.push(Token::Regex(pattern.to_owned()));
                    continue;
                }

                // Comparison: >N, <N, >=N, <=N, =N
                if field == "index" {
                    if let Some(cmp_tok) = parse_cmp(value, word_start + colon_pos + 1)? {
                        tokens.push(cmp_tok);
                        continue;
                    }
                }

                // Plain value word.
                tokens.push(Token::Word(value.to_owned()));
                continue;
            }
        }

        // Check for keywords (case-insensitive).
        match word.to_ascii_uppercase().as_str() {
            "AND" => tokens.push(Token::And),
            "OR" => tokens.push(Token::Or),
            "NOT" => tokens.push(Token::Not),
            _ => tokens.push(Token::Word(word)),
        }
    }

    Ok(tokens)
}

fn parse_cmp(s: &str, offset: usize) -> Result<Option<Token>, QueryError> {
    let (op, rest) = if let Some(rest) = s.strip_prefix(">=") {
        (CmpOp::Gte, rest)
    } else if let Some(rest) = s.strip_prefix("<=") {
        (CmpOp::Lte, rest)
    } else if let Some(rest) = s.strip_prefix('>') {
        (CmpOp::Gt, rest)
    } else if let Some(rest) = s.strip_prefix('<') {
        (CmpOp::Lt, rest)
    } else if let Some(rest) = s.strip_prefix('=') {
        (CmpOp::Eq, rest)
    } else {
        // Try parse as plain number (equality).
        return match s.parse::<usize>() {
            Ok(n) => Ok(Some(Token::Cmp(CmpOp::Eq, n))),
            Err(_) => Ok(None),
        };
    };

    let n = rest.parse::<usize>().map_err(|_| QueryError {
        message: format!("expected a number after comparison operator in `{s}`"),
        offset: Some(offset),
        hint: Some("e.g. `index:>5` or `index:<=100`".to_owned()),
    })?;

    Ok(Some(Token::Cmp(op, n)))
}

// ---------------------------------------------------------------------------
// Parser (recursive descent)
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Parse a full query expression.
    fn parse_query(&mut self) -> Result<LogQuery, QueryError> {
        let expr = self.parse_or()?;
        if !self.at_end() {
            return Err(QueryError {
                message: "unexpected token after expression".to_owned(),
                offset: None,
                hint: Some("use AND/OR to combine terms, or wrap in parentheses".to_owned()),
            });
        }
        Ok(expr)
    }

    /// OR has lowest precedence.
    fn parse_or(&mut self) -> Result<LogQuery, QueryError> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.advance(); // consume OR
            let right = self.parse_and()?;
            left = LogQuery::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// AND has higher precedence than OR. Implicit AND when atoms are juxtaposed.
    fn parse_and(&mut self) -> Result<LogQuery, QueryError> {
        let mut left = self.parse_not()?;
        loop {
            // Explicit AND.
            if matches!(self.peek(), Some(Token::And)) {
                self.advance(); // consume AND
                let right = self.parse_not()?;
                left = LogQuery::And(Box::new(left), Box::new(right));
                continue;
            }
            // Implicit AND: if next token could start an atom, treat as AND.
            if matches!(
                self.peek(),
                Some(
                    Token::Word(_)
                        | Token::Quoted(_)
                        | Token::Field(_)
                        | Token::Not
                        | Token::LParen
                )
            ) {
                let right = self.parse_not()?;
                left = LogQuery::And(Box::new(left), Box::new(right));
                continue;
            }
            break;
        }
        Ok(left)
    }

    /// NOT has higher precedence than AND.
    fn parse_not(&mut self) -> Result<LogQuery, QueryError> {
        if matches!(self.peek(), Some(Token::Not)) {
            self.advance(); // consume NOT
            let inner = self.parse_not()?; // NOT is right-associative
            return Ok(LogQuery::Not(Box::new(inner)));
        }
        self.parse_atom()
    }

    /// Atom: field filter, bare word, quoted string, or parenthesized group.
    fn parse_atom(&mut self) -> Result<LogQuery, QueryError> {
        match self.peek() {
            Some(Token::LParen) => {
                self.advance(); // consume '('
                let inner = self.parse_or()?;
                match self.advance() {
                    Some(Token::RParen) => Ok(inner),
                    _ => Err(QueryError {
                        message: "expected closing ')'".to_owned(),
                        offset: None,
                        hint: Some("add a matching ')' to close the group".to_owned()),
                    }),
                }
            }
            Some(Token::Field(_)) => {
                let field = match self.advance() {
                    Some(Token::Field(f)) => f,
                    _ => unreachable!(),
                };
                self.parse_field_value(&field)
            }
            Some(Token::Quoted(_)) => match self.advance() {
                Some(Token::Quoted(s)) => Ok(LogQuery::TextContains(s)),
                _ => unreachable!(),
            },
            Some(Token::Word(_)) => match self.advance() {
                Some(Token::Word(w)) => Ok(LogQuery::TextContains(w)),
                _ => unreachable!(),
            },
            Some(Token::Not) => {
                // Should be handled by parse_not, but just in case.
                self.parse_not()
            }
            Some(other) => Err(QueryError {
                message: format!("unexpected token: {other:?}"),
                offset: None,
                hint: Some("expected a search term, field filter, or '('".to_owned()),
            }),
            None => Err(QueryError {
                message: "unexpected end of query".to_owned(),
                offset: None,
                hint: Some("provide a search term".to_owned()),
            }),
        }
    }

    /// Parse the value part after a field prefix.
    fn parse_field_value(&mut self, field: &str) -> Result<LogQuery, QueryError> {
        match self.peek() {
            Some(Token::Regex(_)) => {
                let pattern = match self.advance() {
                    Some(Token::Regex(p)) => p,
                    _ => unreachable!(),
                };
                if field != "text" {
                    return Err(QueryError {
                        message: format!(
                            "regex is only supported on `text:` field, not `{field}:`"
                        ),
                        offset: None,
                        hint: Some("use `text:/pattern/`".to_owned()),
                    });
                }
                // Validate regex compiles.
                validate_regex(&pattern)?;
                Ok(LogQuery::TextRegex(pattern))
            }
            Some(Token::Cmp(_, _)) => {
                let (op, n) = match self.advance() {
                    Some(Token::Cmp(op, n)) => (op, n),
                    _ => unreachable!(),
                };
                if field != "index" {
                    return Err(QueryError {
                        message: format!(
                            "comparison operators are only supported on `index:` field, not `{field}:`"
                        ),
                        offset: None,
                        hint: Some("use `index:>5` or `index:<=100`".to_owned()),
                    });
                }
                Ok(LogQuery::IndexCmp(op, n))
            }
            Some(Token::Word(_)) => {
                let value = match self.advance() {
                    Some(Token::Word(w)) => w,
                    _ => unreachable!(),
                };
                match field {
                    "lane" => {
                        let lane = parse_lane(&value)?;
                        Ok(LogQuery::LaneEq(lane))
                    }
                    "text" => Ok(LogQuery::TextContains(value)),
                    "index" => {
                        // Plain number = equality.
                        let n = value.parse::<usize>().map_err(|_| QueryError {
                            message: format!("expected a number for `index:`, got `{value}`"),
                            offset: None,
                            hint: Some("e.g. `index:5` or `index:>10`".to_owned()),
                        })?;
                        Ok(LogQuery::IndexCmp(CmpOp::Eq, n))
                    }
                    _ => Err(QueryError {
                        message: format!("unknown field `{field}`"),
                        offset: None,
                        hint: Some("supported fields: lane, text, index".to_owned()),
                    }),
                }
            }
            _ => Err(QueryError {
                message: format!("expected value after `{field}:`"),
                offset: None,
                hint: Some(format!("e.g. `{field}:tool`")),
            }),
        }
    }
}

fn parse_lane(value: &str) -> Result<LogLane, QueryError> {
    match value.to_ascii_lowercase().as_str() {
        "thinking" => Ok(LogLane::Thinking),
        "tool" => Ok(LogLane::Tool),
        "stdout" => Ok(LogLane::Stdout),
        "stderr" => Ok(LogLane::Stderr),
        "event" => Ok(LogLane::Event),
        "unknown" => Ok(LogLane::Unknown),
        _ => Err(QueryError {
            message: format!("unknown lane `{value}`"),
            offset: None,
            hint: Some("valid lanes: thinking, tool, stdout, stderr, event, unknown".to_owned()),
        }),
    }
}

fn validate_regex(pattern: &str) -> Result<(), QueryError> {
    if pattern.len() > MAX_REGEX_LEN {
        return Err(QueryError {
            message: format!(
                "regex pattern too long ({} chars, max {MAX_REGEX_LEN})",
                pattern.len()
            ),
            offset: None,
            hint: Some("simplify the pattern".to_owned()),
        });
    }
    // Use regex crate's default size limits for catastrophic backtracking protection.
    regex::Regex::new(pattern).map_err(|e| QueryError {
        message: format!("invalid regex: {e}"),
        offset: None,
        hint: Some("check your regex syntax".to_owned()),
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API: parse
// ---------------------------------------------------------------------------

/// Parse a query string into a `LogQuery` AST.
///
/// Returns a `QueryError` with diagnostics if the input is malformed.
pub fn parse_query(input: &str) -> Result<LogQuery, QueryError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(QueryError {
            message: "empty query".to_owned(),
            offset: Some(0),
            hint: Some("enter a search term, e.g. `error` or `lane:stderr`".to_owned()),
        });
    }
    let tokens = tokenize(trimmed)?;
    if tokens.is_empty() {
        return Err(QueryError {
            message: "empty query after tokenization".to_owned(),
            offset: Some(0),
            hint: Some("enter a search term".to_owned()),
        });
    }
    let mut parser = Parser::new(tokens);
    parser.parse_query()
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Evaluate a parsed query against a single log line. Returns `true` if the line matches.
pub fn eval_query(query: &LogQuery, line: &LanedLogLine) -> bool {
    match query {
        LogQuery::TextContains(s) => {
            let needle = s.to_ascii_lowercase();
            line.text.to_ascii_lowercase().contains(&needle)
        }
        LogQuery::LaneEq(lane) => line.lane == *lane,
        LogQuery::TextRegex(pattern) => {
            // Compile on each eval — for filtering a model, prefer `filter_model`.
            match regex::Regex::new(pattern) {
                Ok(re) => re.is_match(&line.text),
                Err(_) => false,
            }
        }
        LogQuery::IndexCmp(op, n) => match op {
            CmpOp::Eq => line.index == *n,
            CmpOp::Gt => line.index > *n,
            CmpOp::Lt => line.index < *n,
            CmpOp::Gte => line.index >= *n,
            CmpOp::Lte => line.index <= *n,
        },
        LogQuery::Not(inner) => !eval_query(inner, line),
        LogQuery::And(a, b) => eval_query(a, line) && eval_query(b, line),
        LogQuery::Or(a, b) => eval_query(a, line) || eval_query(b, line),
    }
}

/// Filter a laned log model by a query, returning matching lines.
#[must_use]
pub fn filter_model<'a>(query: &LogQuery, model: &'a LanedLogModel) -> Vec<&'a LanedLogLine> {
    // Pre-compile regex once if the query contains one, for efficiency.
    let compiled = compile_regex_if_needed(query);
    model
        .all_lines()
        .iter()
        .filter(|line| eval_with_compiled(query, line, &compiled))
        .collect()
}

/// Parse and filter in one step. Returns an error if the query is invalid.
pub fn query_model<'a>(
    input: &str,
    model: &'a LanedLogModel,
) -> Result<Vec<&'a LanedLogLine>, QueryError> {
    let query = parse_query(input)?;
    Ok(filter_model(&query, model))
}

// ---------------------------------------------------------------------------
// Compiled regex cache for efficient filtering
// ---------------------------------------------------------------------------

/// Pre-compiled regex patterns extracted from a query tree.
struct CompiledRegexes {
    /// Map from pattern string to compiled regex.
    regexes: std::collections::HashMap<String, regex::Regex>,
}

fn compile_regex_if_needed(query: &LogQuery) -> CompiledRegexes {
    let mut regexes = std::collections::HashMap::new();
    collect_regex_patterns(query, &mut regexes);
    CompiledRegexes { regexes }
}

fn collect_regex_patterns(
    query: &LogQuery,
    regexes: &mut std::collections::HashMap<String, regex::Regex>,
) {
    match query {
        LogQuery::TextRegex(pattern) => {
            if !regexes.contains_key(pattern) {
                if let Ok(re) = regex::Regex::new(pattern) {
                    regexes.insert(pattern.clone(), re);
                }
            }
        }
        LogQuery::Not(inner) => collect_regex_patterns(inner, regexes),
        LogQuery::And(a, b) | LogQuery::Or(a, b) => {
            collect_regex_patterns(a, regexes);
            collect_regex_patterns(b, regexes);
        }
        _ => {}
    }
}

fn eval_with_compiled(query: &LogQuery, line: &LanedLogLine, compiled: &CompiledRegexes) -> bool {
    match query {
        LogQuery::TextContains(s) => {
            let needle = s.to_ascii_lowercase();
            line.text.to_ascii_lowercase().contains(&needle)
        }
        LogQuery::LaneEq(lane) => line.lane == *lane,
        LogQuery::TextRegex(pattern) => compiled
            .regexes
            .get(pattern)
            .is_some_and(|re| re.is_match(&line.text)),
        LogQuery::IndexCmp(op, n) => match op {
            CmpOp::Eq => line.index == *n,
            CmpOp::Gt => line.index > *n,
            CmpOp::Lt => line.index < *n,
            CmpOp::Gte => line.index >= *n,
            CmpOp::Lte => line.index <= *n,
        },
        LogQuery::Not(inner) => !eval_with_compiled(inner, line, compiled),
        LogQuery::And(a, b) => {
            eval_with_compiled(a, line, compiled) && eval_with_compiled(b, line, compiled)
        }
        LogQuery::Or(a, b) => {
            eval_with_compiled(a, line, compiled) || eval_with_compiled(b, line, compiled)
        }
    }
}

// ---------------------------------------------------------------------------
// Display for QueryError (already implemented via Display trait above)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lane_model::{LanedLogLine, LanedLogModel, LogLane};

    fn line(text: &str, lane: LogLane, index: usize) -> LanedLogLine {
        LanedLogLine {
            text: text.to_owned(),
            lane,
            index,
        }
    }

    fn sample_model() -> LanedLogModel {
        LanedLogModel::from_lines(vec![
            line("[EVENT] start", LogLane::Event, 0),
            line("hello world", LogLane::Stdout, 1),
            line("Error: file not found", LogLane::Stderr, 2),
            line("Tool: read_file", LogLane::Tool, 3),
            line("Thinking: let me consider", LogLane::Thinking, 4),
            line("", LogLane::Unknown, 5),
            line("another stdout line", LogLane::Stdout, 6),
            line("warning: unused variable", LogLane::Stderr, 7),
            line("$ cargo test", LogLane::Tool, 8),
            line("all tests passed", LogLane::Stdout, 9),
        ])
    }

    // -- tokenizer tests --

    #[test]
    fn tokenize_bare_word() {
        let tokens = tokenize("error").ok();
        assert!(tokens.is_some());
        let tokens = tokens.unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], Token::Word("error".to_owned()));
    }

    #[test]
    fn tokenize_quoted_string() {
        let tokens = tokenize("\"file not found\"").ok();
        assert!(tokens.is_some());
        let tokens = tokens.unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], Token::Quoted("file not found".to_owned()));
    }

    #[test]
    fn tokenize_unterminated_quote() {
        let result = tokenize("\"unclosed");
        assert!(result.is_err());
        let err = result.err().unwrap_or_else(|| QueryError {
            message: String::new(),
            offset: None,
            hint: None,
        });
        assert!(err.message.contains("unterminated"));
    }

    #[test]
    fn tokenize_field_value() {
        let tokens = tokenize("lane:tool").ok();
        assert!(tokens.is_some());
        let tokens = tokens.unwrap_or_default();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], Token::Field("lane".to_owned()));
        assert_eq!(tokens[1], Token::Word("tool".to_owned()));
    }

    #[test]
    fn tokenize_field_regex() {
        let tokens = tokenize("text:/err.*/").ok();
        assert!(tokens.is_some());
        let tokens = tokens.unwrap_or_default();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], Token::Field("text".to_owned()));
        assert_eq!(tokens[1], Token::Regex("err.*".to_owned()));
    }

    #[test]
    fn tokenize_index_cmp() {
        let tokens = tokenize("index:>5").ok();
        assert!(tokens.is_some());
        let tokens = tokens.unwrap_or_default();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], Token::Field("index".to_owned()));
        assert_eq!(tokens[1], Token::Cmp(CmpOp::Gt, 5));
    }

    #[test]
    fn tokenize_boolean_keywords() {
        let tokens = tokenize("error AND lane:stderr").ok();
        assert!(tokens.is_some());
        let tokens = tokens.unwrap_or_default();
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0], Token::Word("error".to_owned()));
        assert_eq!(tokens[1], Token::And);
        assert_eq!(tokens[2], Token::Field("lane".to_owned()));
        assert_eq!(tokens[3], Token::Word("stderr".to_owned()));
    }

    #[test]
    fn tokenize_parens() {
        let tokens = tokenize("(a OR b)").ok();
        assert!(tokens.is_some());
        let tokens = tokens.unwrap_or_default();
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0], Token::LParen);
        assert_eq!(tokens[4], Token::RParen);
    }

    // -- parser tests --

    #[test]
    fn parse_bare_word() {
        let q = parse_query("error");
        assert!(q.is_ok());
        assert_eq!(q.ok(), Some(LogQuery::TextContains("error".to_owned())));
    }

    #[test]
    fn parse_quoted() {
        let q = parse_query("\"file not found\"");
        assert!(q.is_ok());
        assert_eq!(
            q.ok(),
            Some(LogQuery::TextContains("file not found".to_owned()))
        );
    }

    #[test]
    fn parse_lane_filter() {
        let q = parse_query("lane:tool");
        assert!(q.is_ok());
        assert_eq!(q.ok(), Some(LogQuery::LaneEq(LogLane::Tool)));
    }

    #[test]
    fn parse_lane_filter_case_insensitive() {
        let q = parse_query("LANE:STDERR");
        assert!(q.is_ok());
        assert_eq!(q.ok(), Some(LogQuery::LaneEq(LogLane::Stderr)));
    }

    #[test]
    fn parse_text_regex() {
        let q = parse_query("text:/err.*/");
        assert!(q.is_ok());
        assert_eq!(q.ok(), Some(LogQuery::TextRegex("err.*".to_owned())));
    }

    #[test]
    fn parse_index_gt() {
        let q = parse_query("index:>5");
        assert!(q.is_ok());
        assert_eq!(q.ok(), Some(LogQuery::IndexCmp(CmpOp::Gt, 5)));
    }

    #[test]
    fn parse_index_lte() {
        let q = parse_query("index:<=3");
        assert!(q.is_ok());
        assert_eq!(q.ok(), Some(LogQuery::IndexCmp(CmpOp::Lte, 3)));
    }

    #[test]
    fn parse_not() {
        let q = parse_query("NOT error");
        assert!(q.is_ok());
        assert_eq!(
            q.ok(),
            Some(LogQuery::Not(Box::new(LogQuery::TextContains(
                "error".to_owned()
            ))))
        );
    }

    #[test]
    fn parse_and_explicit() {
        let q = parse_query("error AND lane:stderr");
        assert!(q.is_ok());
        assert_eq!(
            q.ok(),
            Some(LogQuery::And(
                Box::new(LogQuery::TextContains("error".to_owned())),
                Box::new(LogQuery::LaneEq(LogLane::Stderr)),
            ))
        );
    }

    #[test]
    fn parse_and_implicit() {
        let q = parse_query("error lane:stderr");
        assert!(q.is_ok());
        assert_eq!(
            q.ok(),
            Some(LogQuery::And(
                Box::new(LogQuery::TextContains("error".to_owned())),
                Box::new(LogQuery::LaneEq(LogLane::Stderr)),
            ))
        );
    }

    #[test]
    fn parse_or() {
        let q = parse_query("error OR warning");
        assert!(q.is_ok());
        assert_eq!(
            q.ok(),
            Some(LogQuery::Or(
                Box::new(LogQuery::TextContains("error".to_owned())),
                Box::new(LogQuery::TextContains("warning".to_owned())),
            ))
        );
    }

    #[test]
    fn parse_precedence_and_over_or() {
        // "a OR b AND c" should parse as "a OR (b AND c)"
        let q = parse_query("a OR b AND c");
        assert!(q.is_ok());
        assert_eq!(
            q.ok(),
            Some(LogQuery::Or(
                Box::new(LogQuery::TextContains("a".to_owned())),
                Box::new(LogQuery::And(
                    Box::new(LogQuery::TextContains("b".to_owned())),
                    Box::new(LogQuery::TextContains("c".to_owned())),
                )),
            ))
        );
    }

    #[test]
    fn parse_grouped() {
        let q = parse_query("(error OR warning) AND lane:stderr");
        assert!(q.is_ok());
        assert_eq!(
            q.ok(),
            Some(LogQuery::And(
                Box::new(LogQuery::Or(
                    Box::new(LogQuery::TextContains("error".to_owned())),
                    Box::new(LogQuery::TextContains("warning".to_owned())),
                )),
                Box::new(LogQuery::LaneEq(LogLane::Stderr)),
            ))
        );
    }

    #[test]
    fn parse_empty_error() {
        let q = parse_query("");
        assert!(q.is_err());
        let err = q.err().unwrap_or_else(|| QueryError {
            message: String::new(),
            offset: None,
            hint: None,
        });
        assert!(err.message.contains("empty"));
        assert!(err.hint.is_some());
    }

    #[test]
    fn parse_unknown_lane_error() {
        let q = parse_query("lane:bogus");
        assert!(q.is_err());
        let err = q.err().unwrap_or_else(|| QueryError {
            message: String::new(),
            offset: None,
            hint: None,
        });
        assert!(err.message.contains("unknown lane"));
        assert!(err.hint.is_some());
    }

    #[test]
    fn parse_invalid_regex_error() {
        let q = parse_query("text:/[invalid/");
        assert!(q.is_err());
        let err = q.err().unwrap_or_else(|| QueryError {
            message: String::new(),
            offset: None,
            hint: None,
        });
        assert!(err.message.contains("invalid regex"));
    }

    #[test]
    fn parse_unclosed_paren_error() {
        let q = parse_query("(error OR warning");
        assert!(q.is_err());
        let err = q.err().unwrap_or_else(|| QueryError {
            message: String::new(),
            offset: None,
            hint: None,
        });
        assert!(err.message.contains("')'"));
    }

    #[test]
    fn parse_empty_field_value_error() {
        let q = parse_query("lane:");
        assert!(q.is_err());
        let err = q.err().unwrap_or_else(|| QueryError {
            message: String::new(),
            offset: None,
            hint: None,
        });
        assert!(err.message.contains("requires a value"));
    }

    // -- eval tests --

    #[test]
    fn eval_text_contains_case_insensitive() {
        let l = line("Error: File Not Found", LogLane::Stderr, 0);
        let q = LogQuery::TextContains("file not found".to_owned());
        assert!(eval_query(&q, &l));
    }

    #[test]
    fn eval_text_contains_no_match() {
        let l = line("hello world", LogLane::Stdout, 0);
        let q = LogQuery::TextContains("error".to_owned());
        assert!(!eval_query(&q, &l));
    }

    #[test]
    fn eval_lane_eq() {
        let l = line("Tool: read_file", LogLane::Tool, 0);
        assert!(eval_query(&LogQuery::LaneEq(LogLane::Tool), &l));
        assert!(!eval_query(&LogQuery::LaneEq(LogLane::Stderr), &l));
    }

    #[test]
    fn eval_text_regex() {
        let l = line("Error: file not found", LogLane::Stderr, 0);
        assert!(eval_query(
            &LogQuery::TextRegex("Error.*found".to_owned()),
            &l
        ));
        assert!(!eval_query(&LogQuery::TextRegex("^Tool:".to_owned()), &l));
    }

    #[test]
    fn eval_index_cmp() {
        let l = line("test", LogLane::Stdout, 5);
        assert!(eval_query(&LogQuery::IndexCmp(CmpOp::Eq, 5), &l));
        assert!(eval_query(&LogQuery::IndexCmp(CmpOp::Gt, 4), &l));
        assert!(eval_query(&LogQuery::IndexCmp(CmpOp::Gte, 5), &l));
        assert!(eval_query(&LogQuery::IndexCmp(CmpOp::Lt, 6), &l));
        assert!(eval_query(&LogQuery::IndexCmp(CmpOp::Lte, 5), &l));
        assert!(!eval_query(&LogQuery::IndexCmp(CmpOp::Gt, 5), &l));
    }

    #[test]
    fn eval_not() {
        let l = line("hello", LogLane::Stdout, 0);
        let q = LogQuery::Not(Box::new(LogQuery::TextContains("error".to_owned())));
        assert!(eval_query(&q, &l));
    }

    #[test]
    fn eval_and() {
        let l = line("Error: timeout", LogLane::Stderr, 0);
        let q = LogQuery::And(
            Box::new(LogQuery::TextContains("error".to_owned())),
            Box::new(LogQuery::LaneEq(LogLane::Stderr)),
        );
        assert!(eval_query(&q, &l));
    }

    #[test]
    fn eval_or() {
        let l = line("warning: unused", LogLane::Stderr, 0);
        let q = LogQuery::Or(
            Box::new(LogQuery::TextContains("error".to_owned())),
            Box::new(LogQuery::TextContains("warning".to_owned())),
        );
        assert!(eval_query(&q, &l));
    }

    // -- filter_model integration --

    #[test]
    fn filter_model_bare_text() {
        let model = sample_model();
        let q = parse_query("error");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        let results = filter_model(&q, &model);
        // Matches "Error: file not found" (index 2)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].index, 2);
    }

    #[test]
    fn filter_model_lane() {
        let model = sample_model();
        let q = parse_query("lane:tool");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        let results = filter_model(&q, &model);
        // Matches index 3 and 8.
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].index, 3);
        assert_eq!(results[1].index, 8);
    }

    #[test]
    fn filter_model_combined() {
        let model = sample_model();
        let q = parse_query("lane:stderr AND warning");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        let results = filter_model(&q, &model);
        // Matches "warning: unused variable" (index 7)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].index, 7);
    }

    #[test]
    fn filter_model_or() {
        let model = sample_model();
        let q = parse_query("lane:tool OR lane:event");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        let results = filter_model(&q, &model);
        // Matches events (0) and tools (3, 8).
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].index, 0);
        assert_eq!(results[1].index, 3);
        assert_eq!(results[2].index, 8);
    }

    #[test]
    fn filter_model_not() {
        let model = sample_model();
        let q = parse_query("NOT lane:stdout");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        let results = filter_model(&q, &model);
        // Everything except stdout lines (1, 6, 9) = 7 lines
        assert_eq!(results.len(), 7);
    }

    #[test]
    fn filter_model_regex() {
        let model = sample_model();
        let q = parse_query("text:/^\\[EVENT\\]/");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        let results = filter_model(&q, &model);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].index, 0);
    }

    #[test]
    fn filter_model_index_range() {
        let model = sample_model();
        let q = parse_query("index:>=3 AND index:<7");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        let results = filter_model(&q, &model);
        // Indices 3, 4, 5, 6
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn query_model_convenience() {
        let model = sample_model();
        let results = query_model("lane:stderr", &model);
        assert!(results.is_ok());
        let results = results.unwrap_or_default();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_model_invalid() {
        let model = sample_model();
        let results = query_model("", &model);
        assert!(results.is_err());
    }

    // -- golden tests: parse → AST snapshot --

    #[test]
    fn golden_parse_simple_text() {
        let q = parse_query("panic");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        assert_eq!(format!("{q:?}"), "TextContains(\"panic\")");
    }

    #[test]
    fn golden_parse_lane_filter() {
        let q = parse_query("lane:thinking");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        assert_eq!(format!("{q:?}"), "LaneEq(Thinking)");
    }

    #[test]
    fn golden_parse_complex_boolean() {
        let q = parse_query("(error OR warning) AND lane:stderr NOT index:>100");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        let debug = format!("{q:?}");
        assert!(debug.contains("Or("));
        assert!(debug.contains("And("));
        assert!(debug.contains("Not("));
        assert!(debug.contains("LaneEq(Stderr)"));
        assert!(debug.contains("IndexCmp(Gt, 100)"));
    }

    #[test]
    fn golden_parse_regex() {
        let q = parse_query("text:/^Error: .+/");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        assert_eq!(format!("{q:?}"), "TextRegex(\"^Error: .+\")");
    }

    #[test]
    fn golden_parse_implicit_and_chain() {
        let q = parse_query("lane:tool cargo test");
        assert!(q.is_ok());
        let q = q.unwrap_or_else(|_| LogQuery::TextContains(String::new()));
        let debug = format!("{q:?}");
        // Should be And(And(LaneEq(Tool), TextContains("cargo")), TextContains("test"))
        assert!(debug.contains("And("));
        assert!(debug.contains("LaneEq(Tool)"));
        assert!(debug.contains("TextContains(\"cargo\")"));
        assert!(debug.contains("TextContains(\"test\")"));
    }

    // -- error diagnostics golden tests --

    #[test]
    fn golden_error_empty() {
        let err = parse_query("").err();
        assert!(err.is_some());
        let err = err.unwrap_or_else(|| QueryError {
            message: String::new(),
            offset: None,
            hint: None,
        });
        assert_eq!(err.message, "empty query");
        assert_eq!(err.offset, Some(0));
        assert!(err.hint.is_some());
    }

    #[test]
    fn golden_error_unknown_lane() {
        let err = parse_query("lane:bogus").err();
        assert!(err.is_some());
        let err = err.unwrap_or_else(|| QueryError {
            message: String::new(),
            offset: None,
            hint: None,
        });
        assert!(err.message.contains("unknown lane `bogus`"));
        assert!(err.hint.as_deref().unwrap_or("").contains("thinking, tool"));
    }

    #[test]
    fn golden_error_invalid_regex() {
        let err = parse_query("text:/[invalid/").err();
        assert!(err.is_some());
        let err = err.unwrap_or_else(|| QueryError {
            message: String::new(),
            offset: None,
            hint: None,
        });
        assert!(err.message.contains("invalid regex"));
        assert!(err.hint.as_deref().unwrap_or("").contains("regex syntax"));
    }

    #[test]
    fn golden_error_unclosed_paren() {
        let err = parse_query("(error OR warning").err();
        assert!(err.is_some());
        let err = err.unwrap_or_else(|| QueryError {
            message: String::new(),
            offset: None,
            hint: None,
        });
        assert!(err.message.contains("')'"));
    }

    #[test]
    fn golden_error_display_format() {
        let err = QueryError {
            message: "test error".to_owned(),
            offset: Some(5),
            hint: Some("try this".to_owned()),
        };
        let formatted = format!("{err}");
        assert_eq!(formatted, "test error (at byte 5) — hint: try this");
    }

    #[test]
    fn golden_error_display_no_offset() {
        let err = QueryError {
            message: "test error".to_owned(),
            offset: None,
            hint: None,
        };
        let formatted = format!("{err}");
        assert_eq!(formatted, "test error");
    }
}
