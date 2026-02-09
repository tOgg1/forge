#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorMessage {
    pub timestamp_rfc3339: String,
    pub text: String,
}

pub fn inject_loop_memory(base_prompt: &str, loop_memory: &str) -> String {
    if loop_memory.trim().is_empty() {
        return base_prompt.to_string();
    }
    format!("{}{}", base_prompt.trim_end_matches('\n'), loop_memory)
}

pub fn append_operator_messages(base_prompt: &str, messages: &[OperatorMessage]) -> String {
    if messages.is_empty() {
        return base_prompt.to_string();
    }

    let mut out = String::from(base_prompt.trim_end_matches('\n'));
    for entry in messages {
        out.push_str("\n\n## Operator Message (");
        out.push_str(entry.timestamp_rfc3339.as_str());
        out.push_str(")\n\n");
        out.push_str(entry.text.trim());
    }
    out
}

pub fn compose_prompt(
    base_prompt: &str,
    loop_memory: &str,
    messages: &[OperatorMessage],
) -> String {
    let with_memory = inject_loop_memory(base_prompt, loop_memory);
    append_operator_messages(&with_memory, messages)
}

#[cfg(test)]
mod tests {
    use super::{append_operator_messages, compose_prompt, inject_loop_memory, OperatorMessage};

    #[test]
    fn inject_loop_memory_skips_blank_memory() {
        assert_eq!(inject_loop_memory("base\n", " \n\t "), "base\n");
    }

    #[test]
    fn inject_loop_memory_trims_base_newlines_before_append() {
        let got = inject_loop_memory("base\n\n", "\n\n## Loop Context (persistent)\n");
        assert_eq!(got, "base\n\n## Loop Context (persistent)\n");
    }

    #[test]
    fn append_operator_messages_keeps_base_when_empty() {
        assert_eq!(append_operator_messages("base", &[]), "base");
    }

    #[test]
    fn append_operator_messages_appends_in_order_and_trims_message_text() {
        let got = append_operator_messages(
            "base\n",
            &[
                OperatorMessage {
                    timestamp_rfc3339: "2026-02-09T17:00:00Z".to_string(),
                    text: "  first  ".to_string(),
                },
                OperatorMessage {
                    timestamp_rfc3339: "2026-02-09T17:01:00Z".to_string(),
                    text: "\nsecond\n".to_string(),
                },
            ],
        );
        assert_eq!(
            got,
            "base\n\n## Operator Message (2026-02-09T17:00:00Z)\n\nfirst\n\n## Operator Message (2026-02-09T17:01:00Z)\n\nsecond"
        );
    }

    #[test]
    fn append_operator_messages_with_empty_base_matches_go_shape() {
        let got = append_operator_messages(
            "",
            &[OperatorMessage {
                timestamp_rfc3339: "2026-02-09T17:00:00Z".to_string(),
                text: "msg".to_string(),
            }],
        );
        assert_eq!(got, "\n\n## Operator Message (2026-02-09T17:00:00Z)\n\nmsg");
    }

    #[test]
    fn compose_prompt_injects_memory_before_operator_messages() {
        let got = compose_prompt(
            "base\n",
            "\n\n## Loop Context (persistent)\n\nCurrent:\n- task-1 [in_progress]\n",
            &[OperatorMessage {
                timestamp_rfc3339: "2026-02-09T17:02:00Z".to_string(),
                text: "blocked on schema diff".to_string(),
            }],
        );
        assert_eq!(
            got,
            "base\n\n## Loop Context (persistent)\n\nCurrent:\n- task-1 [in_progress]\n\n## Operator Message (2026-02-09T17:02:00Z)\n\nblocked on schema diff"
        );
    }
}
