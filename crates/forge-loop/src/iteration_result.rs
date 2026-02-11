#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopRunStatus {
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedRunUpdate {
    pub status: LoopRunStatus,
    pub exit_code: i32,
    pub output_tail: String,
    pub last_error: String,
}

pub fn status_from_error(err: Option<&str>) -> LoopRunStatus {
    if err.is_none() {
        LoopRunStatus::Success
    } else {
        LoopRunStatus::Error
    }
}

pub fn error_text(err: Option<&str>) -> String {
    err.unwrap_or("").to_string()
}

pub fn output_tail_or_fallback(primary: &str, fallback: &str) -> String {
    if !primary.is_empty() {
        primary.to_string()
    } else {
        fallback.to_string()
    }
}

pub fn build_persisted_run_update(
    exit_code: i32,
    primary_output_tail: &str,
    fallback_output_tail: &str,
    err: Option<&str>,
) -> PersistedRunUpdate {
    PersistedRunUpdate {
        status: status_from_error(err),
        exit_code,
        output_tail: output_tail_or_fallback(primary_output_tail, fallback_output_tail),
        last_error: error_text(err),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_persisted_run_update, error_text, output_tail_or_fallback, status_from_error,
        LoopRunStatus,
    };

    #[test]
    fn status_from_error_matches_runner_behavior() {
        assert_eq!(status_from_error(None), LoopRunStatus::Success);
        assert_eq!(status_from_error(Some("boom")), LoopRunStatus::Error);
    }

    #[test]
    fn error_text_is_empty_when_no_error() {
        assert_eq!(error_text(None), "");
        assert_eq!(error_text(Some("failure")), "failure");
    }

    #[test]
    fn output_tail_prefers_primary_then_fallback() {
        assert_eq!(output_tail_or_fallback("primary", "fallback"), "primary");
        assert_eq!(output_tail_or_fallback("", "fallback"), "fallback");
    }

    #[test]
    fn build_persisted_run_update_combines_all_fields() {
        let success = build_persisted_run_update(0, "run tail", "fallback", None);
        assert_eq!(success.status, LoopRunStatus::Success);
        assert_eq!(success.exit_code, 0);
        assert_eq!(success.output_tail, "run tail");
        assert_eq!(success.last_error, "");

        let failure = build_persisted_run_update(1, "", "fallback tail", Some("exec failed"));
        assert_eq!(failure.status, LoopRunStatus::Error);
        assert_eq!(failure.exit_code, 1);
        assert_eq!(failure.output_tail, "fallback tail");
        assert_eq!(failure.last_error, "exec failed");
    }
}
