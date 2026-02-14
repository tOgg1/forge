//! Log source abstraction seam for future parsed/diff/pty providers.

/// Logical transport selected by the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogTransportKind {
    LiveLoop,
    LatestRun,
    SelectedRun,
}

impl LogTransportKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::LiveLoop => "live",
            Self::LatestRun => "latest-run",
            Self::SelectedRun => "selected-run",
        }
    }
}

/// Logical content mode presented in the log pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogContentKind {
    Parsed,
    Diff,
    Pty,
}

impl LogContentKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Parsed => "parsed",
            Self::Diff => "diff",
            Self::Pty => "pty",
        }
    }
}

/// Route description consumed by future log providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LogSourceRoute {
    pub transport: LogTransportKind,
    pub content: LogContentKind,
}

impl LogSourceRoute {
    #[must_use]
    pub const fn new(transport: LogTransportKind, content: LogContentKind) -> Self {
        Self { transport, content }
    }

    #[must_use]
    pub fn route_key(self) -> String {
        format!("{}:{}", self.transport.label(), self.content.label())
    }
}

/// Future-proof fetch query for log providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSourceQuery {
    pub loop_id: String,
    pub run_id: Option<String>,
    pub line_limit: usize,
}

/// Generic payload shape for source providers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogSourcePayload {
    pub lines: Vec<String>,
    pub message: String,
}

/// Provider interface seam. Existing runtime can keep its current backend while
/// future parsed/diff/pty providers implement this trait.
pub trait LogSourceProvider {
    fn fetch(&self, route: LogSourceRoute, query: &LogSourceQuery) -> LogSourcePayload;
}

#[cfg(test)]
mod tests {
    use super::{LogContentKind, LogSourceRoute, LogTransportKind};

    #[test]
    fn route_key_includes_transport_and_content() {
        let key =
            LogSourceRoute::new(LogTransportKind::LatestRun, LogContentKind::Diff).route_key();
        assert_eq!(key, "latest-run:diff");
    }

    #[test]
    fn labels_include_future_pty_mode() {
        assert_eq!(LogContentKind::Pty.label(), "pty");
        assert_eq!(LogTransportKind::SelectedRun.label(), "selected-run");
    }
}
