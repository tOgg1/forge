//! Smart loop clustering by inferred work domain.

use std::collections::{BTreeMap, BTreeSet};

use crate::app::LoopView;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopDomainGroup {
    pub domain_key: String,
    pub label: String,
    pub loop_ids: Vec<String>,
    pub loop_names: Vec<String>,
    pub count: usize,
    pub confidence_pct: u8,
}

#[derive(Debug, Clone, Default)]
struct GroupBuilder {
    domain_key: String,
    label: String,
    loop_ids: BTreeSet<String>,
    loop_names: BTreeSet<String>,
    confidence_sum: u64,
    confidence_count: u64,
}

#[derive(Debug, Clone)]
struct DomainGuess {
    key: String,
    label: String,
    confidence_pct: u8,
}

#[must_use]
pub fn cluster_loops_by_domain(loops: &[LoopView]) -> Vec<LoopDomainGroup> {
    if loops.is_empty() {
        return Vec::new();
    }

    let mut groups: BTreeMap<String, GroupBuilder> = BTreeMap::new();
    for loop_view in loops {
        let guess = infer_domain(loop_view);
        let entry = groups
            .entry(guess.key.clone())
            .or_insert_with(|| GroupBuilder {
                domain_key: guess.key.clone(),
                label: guess.label.clone(),
                ..GroupBuilder::default()
            });

        entry.loop_ids.insert(loop_view.id.clone());
        entry.loop_names.insert(display_name(loop_view));
        entry.confidence_sum = entry
            .confidence_sum
            .saturating_add(guess.confidence_pct as u64);
        entry.confidence_count = entry.confidence_count.saturating_add(1);
    }

    let mut out = groups
        .into_values()
        .map(|builder| {
            let count = builder.loop_ids.len();
            let avg_confidence = if builder.confidence_count == 0 {
                50
            } else {
                (builder.confidence_sum / builder.confidence_count) as u8
            };
            LoopDomainGroup {
                domain_key: builder.domain_key,
                label: builder.label,
                loop_ids: builder.loop_ids.into_iter().collect(),
                loop_names: builder.loop_names.into_iter().collect(),
                count,
                confidence_pct: avg_confidence,
            }
        })
        .collect::<Vec<_>>();

    out.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| b.confidence_pct.cmp(&a.confidence_pct))
            .then_with(|| a.label.cmp(&b.label))
    });
    out
}

#[must_use]
pub fn compact_domain_summary(groups: &[LoopDomainGroup], max_groups: usize) -> String {
    if groups.is_empty() {
        return "none".to_owned();
    }
    groups
        .iter()
        .take(max_groups.max(1))
        .map(|group| format!("{} ({})", group.label, group.count))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn infer_domain(loop_view: &LoopView) -> DomainGuess {
    if let Some(crate_name) = repo_crate_name(&loop_view.repo_path) {
        return DomainGuess {
            key: format!("crate:{crate_name}"),
            label: format!("crate {crate_name}"),
            confidence_pct: 92,
        };
    }

    if let Some(path_leaf) = repo_leaf(&loop_view.repo_path) {
        if is_meaningful_token(path_leaf) {
            return DomainGuess {
                key: format!("repo:{path_leaf}"),
                label: format!("repo {path_leaf}"),
                confidence_pct: 84,
            };
        }
    }

    if let Some(token) = name_token(&loop_view.name) {
        return DomainGuess {
            key: format!("name:{token}"),
            label: token.replace('_', " "),
            confidence_pct: 72,
        };
    }

    if let Some(token) = non_empty_token(&loop_view.profile_name) {
        return DomainGuess {
            key: format!("profile:{token}"),
            label: format!("profile {token}"),
            confidence_pct: 64,
        };
    }

    if let Some(token) = non_empty_token(&loop_view.pool_name) {
        return DomainGuess {
            key: format!("pool:{token}"),
            label: format!("pool {token}"),
            confidence_pct: 60,
        };
    }

    DomainGuess {
        key: "misc".to_owned(),
        label: "misc".to_owned(),
        confidence_pct: 42,
    }
}

fn display_name(loop_view: &LoopView) -> String {
    if loop_view.name.trim().is_empty() {
        loop_view.id.clone()
    } else {
        loop_view.name.clone()
    }
}

fn repo_crate_name(path: &str) -> Option<&str> {
    let segments = path
        .split('/')
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>();
    for pair in segments.windows(2) {
        if pair[0] == "crates" && is_meaningful_token(pair[1]) {
            return Some(pair[1]);
        }
    }
    None
}

fn repo_leaf(path: &str) -> Option<&str> {
    path.split('/')
        .rev()
        .map(str::trim)
        .find(|segment| !segment.is_empty() && *segment != "src" && *segment != "repos")
}

fn name_token(name: &str) -> Option<String> {
    name.split(['-', '_', '/', ' '])
        .map(|token| token.trim().to_ascii_lowercase())
        .find(|token| is_meaningful_token(token))
}

fn non_empty_token(value: &str) -> Option<String> {
    let token = value.trim().to_ascii_lowercase();
    if is_meaningful_token(&token) {
        Some(token)
    } else {
        None
    }
}

fn is_meaningful_token(token: &str) -> bool {
    if token.len() < 3 {
        return false;
    }
    !matches!(
        token,
        "loop"
            | "loops"
            | "agent"
            | "worker"
            | "task"
            | "repo"
            | "repos"
            | "forge"
            | "main"
            | "default"
            | "pool"
            | "profile"
    )
}

#[cfg(test)]
mod tests {
    use super::{cluster_loops_by_domain, compact_domain_summary};
    use crate::app::LoopView;

    fn loop_view(id: &str, name: &str, repo_path: &str, profile_name: &str) -> LoopView {
        LoopView {
            id: id.to_owned(),
            short_id: id.to_owned(),
            name: name.to_owned(),
            repo_path: repo_path.to_owned(),
            profile_name: profile_name.to_owned(),
            ..LoopView::default()
        }
    }

    #[test]
    fn clusters_by_crate_when_repo_has_crates_segment() {
        let loops = vec![
            loop_view("a", "auth-1", "/repo/crates/auth-core/src", "dev"),
            loop_view("b", "auth-2", "/repo/crates/auth-core/tests", "dev"),
            loop_view("c", "db-1", "/repo/crates/db-layer/src", "dev"),
        ];

        let groups = cluster_loops_by_domain(&loops);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label, "crate auth-core");
        assert_eq!(groups[0].count, 2);
        assert!(groups[0].confidence_pct >= 85);
    }

    #[test]
    fn falls_back_to_name_or_profile() {
        let loops = vec![
            loop_view("a", "billing-worker-a", "", "default"),
            loop_view("b", "billing-worker-b", "", "default"),
            loop_view("c", "", "", "security"),
        ];
        let groups = cluster_loops_by_domain(&loops);
        assert_eq!(groups[0].count, 2);
        assert!(groups[0].label.contains("billing"));
        assert!(groups.iter().any(|group| group.label.contains("security")));
    }

    #[test]
    fn compact_summary_lists_top_groups() {
        let loops = vec![
            loop_view("a", "auth-1", "/repo/crates/auth-core/src", "dev"),
            loop_view("b", "auth-2", "/repo/crates/auth-core/tests", "dev"),
            loop_view("c", "db-1", "/repo/crates/db-layer/src", "dev"),
        ];
        let groups = cluster_loops_by_domain(&loops);
        let summary = compact_domain_summary(&groups, 2);
        assert!(summary.contains("auth-core (2)"));
        assert!(summary.contains("db-layer (1)"));
    }
}
