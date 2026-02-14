//! Operator-defined alert-rule DSL compiler and evaluator.
//!
//! DSL line format:
//! `rule <id> when <source>.<field> <op> <value> [and ...] then <severity> "<message>"`
//!
//! Sources:
//! - `status`
//! - `log`
//! - `inbox`
//!
//! Operators:
//! - `==`, `!=`
//! - `contains`, `!contains`
//! - `>`, `>=`, `<`, `<=` (numeric compare)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleEventSource {
    Status,
    Log,
    Inbox,
}

impl RuleEventSource {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "status" => Some(Self::Status),
            "log" => Some(Self::Log),
            "inbox" => Some(Self::Inbox),
            _ => None,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Log => "log",
            Self::Inbox => "inbox",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleAlertSeverity {
    Warning,
    Critical,
}

impl RuleAlertSeverity {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "warning" => Some(Self::Warning),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOperator {
    Eq,
    NotEq,
    Contains,
    NotContains,
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertRuleCondition {
    pub field: String,
    pub operator: RuleOperator,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertRule {
    pub id: String,
    pub source: RuleEventSource,
    pub conditions: Vec<AlertRuleCondition>,
    pub severity: RuleAlertSeverity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertRuleParseError {
    pub line: usize,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AlertRuleCompilation {
    pub rules: Vec<AlertRule>,
    pub errors: Vec<AlertRuleParseError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRuleEvent {
    pub loop_id: String,
    pub state: String,
    pub message: String,
    pub queue_depth: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRuleEvent {
    pub loop_id: String,
    pub level: String,
    pub source: String,
    pub line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxRuleEvent {
    pub thread_id: String,
    pub from: String,
    pub subject: String,
    pub body: String,
    pub ack_required: bool,
    pub unread_count: usize,
    pub pending_ack_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertRuleEvent {
    Status(StatusRuleEvent),
    Log(LogRuleEvent),
    Inbox(InboxRuleEvent),
}

impl AlertRuleEvent {
    fn source(&self) -> RuleEventSource {
        match self {
            Self::Status(_) => RuleEventSource::Status,
            Self::Log(_) => RuleEventSource::Log,
            Self::Inbox(_) => RuleEventSource::Inbox,
        }
    }

    fn field_value(&self, field: &str) -> Option<String> {
        let normalized = normalize_field(field);
        match self {
            Self::Status(event) => match normalized.as_str() {
                "loop_id" => Some(event.loop_id.clone()),
                "state" => Some(event.state.clone()),
                "message" => Some(event.message.clone()),
                "queue_depth" => Some(event.queue_depth.to_string()),
                _ => None,
            },
            Self::Log(event) => match normalized.as_str() {
                "loop_id" => Some(event.loop_id.clone()),
                "level" => Some(event.level.clone()),
                "source" => Some(event.source.clone()),
                "line" => Some(event.line.clone()),
                _ => None,
            },
            Self::Inbox(event) => match normalized.as_str() {
                "thread_id" => Some(event.thread_id.clone()),
                "from" => Some(event.from.clone()),
                "subject" => Some(event.subject.clone()),
                "body" => Some(event.body.clone()),
                "ack_required" => Some(event.ack_required.to_string()),
                "unread_count" => Some(event.unread_count.to_string()),
                "pending_ack_count" => Some(event.pending_ack_count.to_string()),
                _ => None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggeredRuleAlert {
    pub rule_id: String,
    pub source: RuleEventSource,
    pub severity: RuleAlertSeverity,
    pub message: String,
}

#[must_use]
pub fn compile_alert_rule_dsl(dsl: &str) -> AlertRuleCompilation {
    let mut compilation = AlertRuleCompilation::default();
    for (line_index, raw_line) in dsl.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match parse_rule_line(line) {
            Ok(rule) => compilation.rules.push(rule),
            Err(message) => compilation.errors.push(AlertRuleParseError {
                line: line_index + 1,
                source: line.to_owned(),
                message,
            }),
        }
    }
    compilation
}

#[must_use]
pub fn evaluate_alert_rules(
    rules: &[AlertRule],
    event: &AlertRuleEvent,
) -> Vec<TriggeredRuleAlert> {
    let mut out = Vec::new();
    for rule in rules {
        if rule.source != event.source() {
            continue;
        }
        if rule
            .conditions
            .iter()
            .all(|condition| condition_matches(condition, event))
        {
            out.push(TriggeredRuleAlert {
                rule_id: rule.id.clone(),
                source: rule.source,
                severity: rule.severity,
                message: rule.message.clone(),
            });
        }
    }
    out
}

#[must_use]
pub fn render_alert_rule_panel_lines(
    compilation: &AlertRuleCompilation,
    width: usize,
) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }

    let mut lines = vec![
        fit_width("ALERT RULE DSL", width),
        fit_width(
            &format!(
                "rules:{}  errors:{}",
                compilation.rules.len(),
                compilation.errors.len()
            ),
            width,
        ),
    ];

    if !compilation.errors.is_empty() {
        for error in compilation.errors.iter().take(3) {
            lines.push(fit_width(
                &format!("line {}: {}", error.line, error.message),
                width,
            ));
        }
        return lines;
    }

    if compilation.rules.is_empty() {
        lines.push(fit_width("no rules configured", width));
        return lines;
    }

    for (index, rule) in compilation.rules.iter().take(5).enumerate() {
        lines.push(fit_width(
            &format!(
                "{}. {} [{} {}]",
                index + 1,
                rule.id,
                rule.source.label(),
                rule.severity.label()
            ),
            width,
        ));
    }
    lines
}

fn parse_rule_line(line: &str) -> Result<AlertRule, String> {
    let Some(remainder) = line.strip_prefix("rule ") else {
        return Err("line must start with `rule `".to_owned());
    };

    let (id_part, tail) = remainder
        .split_once(" when ")
        .ok_or_else(|| "missing ` when ` segment".to_owned())?;
    let id = normalize_rule_id(id_part);
    if id.is_empty() {
        return Err("rule id is required".to_owned());
    }

    let (conditions_raw, action_raw) = tail
        .split_once(" then ")
        .ok_or_else(|| "missing ` then ` segment".to_owned())?;
    let condition_parts = conditions_raw
        .split(" and ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if condition_parts.is_empty() {
        return Err("at least one condition is required".to_owned());
    }

    let mut source: Option<RuleEventSource> = None;
    let mut conditions = Vec::new();
    for part in condition_parts {
        let (event_source, condition) = parse_condition(part)?;
        if let Some(existing) = source {
            if existing != event_source {
                return Err("all conditions must target the same source".to_owned());
            }
        } else {
            source = Some(event_source);
        }
        conditions.push(condition);
    }
    let source = source.ok_or_else(|| "failed to resolve source".to_owned())?;
    let (severity, message) = parse_action(action_raw)?;

    Ok(AlertRule {
        id,
        source,
        conditions,
        severity,
        message,
    })
}

fn parse_condition(part: &str) -> Result<(RuleEventSource, AlertRuleCondition), String> {
    let operators = [
        (" !contains ", RuleOperator::NotContains),
        (" contains ", RuleOperator::Contains),
        (" >= ", RuleOperator::Gte),
        (" <= ", RuleOperator::Lte),
        (" == ", RuleOperator::Eq),
        (" != ", RuleOperator::NotEq),
        (" > ", RuleOperator::Gt),
        (" < ", RuleOperator::Lt),
    ];

    let mut parsed = None;
    for (marker, operator) in operators {
        if let Some((left, right)) = part.split_once(marker) {
            parsed = Some((left.trim(), operator, right.trim()));
            break;
        }
    }
    let Some((left, operator, right)) = parsed else {
        return Err(format!("invalid condition syntax: {part}"));
    };
    if right.is_empty() {
        return Err("condition value is required".to_owned());
    }

    let (source_raw, field_raw) = left
        .split_once('.')
        .ok_or_else(|| "condition must use <source>.<field>".to_owned())?;
    let source = RuleEventSource::parse(source_raw)
        .ok_or_else(|| format!("unknown source {:?}", source_raw.trim()))?;
    let field = normalize_field(field_raw);
    if field.is_empty() {
        return Err("condition field is required".to_owned());
    }

    Ok((
        source,
        AlertRuleCondition {
            field,
            operator,
            value: parse_value(right)?,
        },
    ))
}

fn parse_action(part: &str) -> Result<(RuleAlertSeverity, String), String> {
    let (severity_raw, message_raw) = part
        .trim()
        .split_once(' ')
        .ok_or_else(|| "action must be `<severity> \"message\"`".to_owned())?;
    let Some(severity) = RuleAlertSeverity::parse(severity_raw) else {
        return Err(format!("unknown severity {:?}", severity_raw.trim()));
    };
    let message = parse_value(message_raw)?;
    if message.is_empty() {
        return Err("action message is required".to_owned());
    }
    Ok((severity, message))
}

fn parse_value(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("value is required".to_owned());
    }
    if trimmed.starts_with('"') {
        if !trimmed.ends_with('"') || trimmed.len() < 2 {
            return Err("quoted value is missing closing quote".to_owned());
        }
        return Ok(trimmed[1..trimmed.len() - 1].to_owned());
    }
    Ok(trimmed.to_owned())
}

fn condition_matches(condition: &AlertRuleCondition, event: &AlertRuleEvent) -> bool {
    let Some(actual_raw) = event.field_value(&condition.field) else {
        return false;
    };

    match condition.operator {
        RuleOperator::Eq => normalize_text(&actual_raw) == normalize_text(&condition.value),
        RuleOperator::NotEq => normalize_text(&actual_raw) != normalize_text(&condition.value),
        RuleOperator::Contains => {
            normalize_text(&actual_raw).contains(&normalize_text(&condition.value))
        }
        RuleOperator::NotContains => {
            !normalize_text(&actual_raw).contains(&normalize_text(&condition.value))
        }
        RuleOperator::Gt => compare_numeric(&actual_raw, &condition.value, |a, b| a > b),
        RuleOperator::Gte => compare_numeric(&actual_raw, &condition.value, |a, b| a >= b),
        RuleOperator::Lt => compare_numeric(&actual_raw, &condition.value, |a, b| a < b),
        RuleOperator::Lte => compare_numeric(&actual_raw, &condition.value, |a, b| a <= b),
    }
}

fn compare_numeric<F>(left: &str, right: &str, op: F) -> bool
where
    F: FnOnce(f64, f64) -> bool,
{
    let Ok(left_num) = left.trim().parse::<f64>() else {
        return false;
    };
    let Ok(right_num) = right.trim().parse::<f64>() else {
        return false;
    };
    op(left_num, right_num)
}

fn normalize_rule_id(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

fn normalize_field(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

fn normalize_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn fit_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if value.len() <= width {
        return value.to_owned();
    }
    value.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        compile_alert_rule_dsl, evaluate_alert_rules, render_alert_rule_panel_lines,
        AlertRuleEvent, LogRuleEvent, RuleAlertSeverity, RuleEventSource, StatusRuleEvent,
    };

    #[test]
    fn compile_parses_valid_rule_lines() {
        let dsl = r#"
            # comment
            rule panic-log when log.line contains "panic" then critical "panic detected"
            rule queue-deep when status.state == "error" and status.queue_depth >= 8 then warning "queue depth high"
        "#;
        let compilation = compile_alert_rule_dsl(dsl);
        assert!(compilation.errors.is_empty(), "{:?}", compilation.errors);
        assert_eq!(compilation.rules.len(), 2);
        assert_eq!(compilation.rules[0].id, "panic-log");
        assert_eq!(compilation.rules[0].source, RuleEventSource::Log);
        assert_eq!(compilation.rules[1].conditions.len(), 2);
    }

    #[test]
    fn compile_collects_parse_errors_with_line_numbers() {
        let dsl = r#"
            rule missing-action when status.state == "error"
            bad-prefix queue when status.state == "error" then warning "x"
        "#;
        let compilation = compile_alert_rule_dsl(dsl);
        assert!(compilation.rules.is_empty());
        assert_eq!(compilation.errors.len(), 2);
        assert_eq!(compilation.errors[0].line, 2);
        assert_eq!(compilation.errors[1].line, 3);
    }

    #[test]
    fn evaluate_matches_status_rules() {
        let dsl = r#"
            rule queue-deep when status.state == "error" and status.queue_depth >= 12 then warning "queue high"
        "#;
        let compilation = compile_alert_rule_dsl(dsl);
        assert!(compilation.errors.is_empty());
        let event = AlertRuleEvent::Status(StatusRuleEvent {
            loop_id: "loop-7".to_owned(),
            state: "error".to_owned(),
            message: "stuck queue".to_owned(),
            queue_depth: 14,
        });
        let alerts = evaluate_alert_rules(&compilation.rules, &event);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].rule_id, "queue-deep");
        assert_eq!(alerts[0].severity, RuleAlertSeverity::Warning);
    }

    #[test]
    fn evaluate_respects_source_and_contains_ops() {
        let dsl = r#"
            rule panic-log when log.line contains "panic" and log.level != "debug" then critical "panic log"
            rule inbox-ack when inbox.ack_required == true then warning "needs ack"
        "#;
        let compilation = compile_alert_rule_dsl(dsl);
        assert!(compilation.errors.is_empty());

        let log_event = AlertRuleEvent::Log(LogRuleEvent {
            loop_id: "loop-a".to_owned(),
            level: "error".to_owned(),
            source: "stdout".to_owned(),
            line: "panic: worker failed".to_owned(),
        });
        let alerts = evaluate_alert_rules(&compilation.rules, &log_event);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].rule_id, "panic-log");
        assert_eq!(alerts[0].severity, RuleAlertSeverity::Critical);
    }

    #[test]
    fn panel_lines_show_error_preview() {
        let compilation = compile_alert_rule_dsl(r#"rule x when status.state == "error""#);
        let lines = render_alert_rule_panel_lines(&compilation, 80);
        assert!(lines[0].contains("ALERT RULE DSL"));
        assert!(lines[1].contains("errors:1"));
        assert!(lines.iter().any(|line| line.contains("line 1")));
    }
}
