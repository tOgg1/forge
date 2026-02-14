//! Lightweight link registry for actionable references in rendered text.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LinkTarget {
    Url(String),
    Run(String),
    Loop(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEntry {
    pub target: LinkTarget,
    pub display: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkRegistry {
    entries: Vec<LinkEntry>,
}

impl LinkRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn entries(&self) -> &[LinkEntry] {
        &self.entries
    }

    #[must_use]
    pub fn first(&self) -> Option<&LinkEntry> {
        self.entries.first()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn register_target(&mut self, target: LinkTarget) {
        if self.entries.iter().any(|entry| entry.target == target) {
            return;
        }
        let display = match &target {
            LinkTarget::Url(value) | LinkTarget::Run(value) | LinkTarget::Loop(value) => {
                value.clone()
            }
        };
        self.entries.push(LinkEntry { target, display });
    }

    pub fn register_text(&mut self, text: &str) {
        for token in text.split_whitespace() {
            let cleaned = trim_punctuation(token);
            if cleaned.is_empty() {
                continue;
            }
            if cleaned.starts_with("http://") || cleaned.starts_with("https://") {
                self.register_target(LinkTarget::Url(cleaned.to_owned()));
                continue;
            }
            if looks_like_loop_id(cleaned) {
                self.register_target(LinkTarget::Loop(cleaned.to_owned()));
                continue;
            }
            if looks_like_run_id(cleaned) {
                self.register_target(LinkTarget::Run(cleaned.to_owned()));
            }
        }
    }
}

fn trim_punctuation(token: &str) -> &str {
    token.trim_matches(|ch: char| {
        matches!(
            ch,
            ',' | ';' | ':' | '.' | ')' | '(' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\''
        )
    })
}

fn looks_like_loop_id(token: &str) -> bool {
    token.starts_with("loop-")
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn looks_like_run_id(token: &str) -> bool {
    token.starts_with("run-")
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::{LinkRegistry, LinkTarget};

    #[test]
    fn register_text_extracts_urls_and_ids() {
        let mut registry = LinkRegistry::new();
        registry.register_text("see https://example.com/docs run-123 loop-4");

        let targets: Vec<LinkTarget> = registry
            .entries()
            .iter()
            .map(|entry| entry.target.clone())
            .collect();
        assert_eq!(
            targets,
            vec![
                LinkTarget::Url("https://example.com/docs".to_owned()),
                LinkTarget::Run("run-123".to_owned()),
                LinkTarget::Loop("loop-4".to_owned()),
            ]
        );
    }

    #[test]
    fn register_text_deduplicates_targets() {
        let mut registry = LinkRegistry::new();
        registry.register_text("run-7 run-7 https://x.dev https://x.dev");

        assert_eq!(registry.entries().len(), 2);
        assert_eq!(
            registry.entries()[0].target,
            LinkTarget::Run("run-7".to_owned())
        );
        assert_eq!(
            registry.entries()[1].target,
            LinkTarget::Url("https://x.dev".to_owned())
        );
    }

    #[test]
    fn register_text_trims_common_trailing_punctuation() {
        let mut registry = LinkRegistry::new();
        registry.register_text("(https://example.com/docs), [run-4]");

        let targets: Vec<LinkTarget> = registry
            .entries()
            .iter()
            .map(|entry| entry.target.clone())
            .collect();
        assert_eq!(
            targets,
            vec![
                LinkTarget::Url("https://example.com/docs".to_owned()),
                LinkTarget::Run("run-4".to_owned()),
            ]
        );
    }
}
