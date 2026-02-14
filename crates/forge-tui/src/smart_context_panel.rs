//! Smart context panel: auto-surface related loops, comms, git, failures, and graph neighborhood.

use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLoopSample {
    pub loop_id: String,
    pub state: String,
    pub summary: String,
    pub files: Vec<String>,
    pub crates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFmailThread {
    pub thread_id: String,
    pub subject: String,
    pub body: String,
    pub mentioned_loops: Vec<String>,
    pub last_message_at_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextCommit {
    pub commit_id: String,
    pub summary: String,
    pub files_changed: Vec<String>,
    pub touched_loops: Vec<String>,
    pub authored_at_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFailureSample {
    pub loop_id: String,
    pub signature: String,
    pub excerpt: String,
    pub seen_at_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDependencyLink {
    pub loop_id: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SmartContextPanelInput {
    pub focused_loop_id: String,
    pub loops: Vec<ContextLoopSample>,
    pub fmail_threads: Vec<ContextFmailThread>,
    pub commits: Vec<ContextCommit>,
    pub failures: Vec<ContextFailureSample>,
    pub dependencies: Vec<ContextDependencyLink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedLoopMatch {
    pub loop_id: String,
    pub score: i32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FmailMatch {
    pub thread_id: String,
    pub score: i32,
    pub subject: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMatch {
    pub commit_id: String,
    pub score: i32,
    pub summary: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureMatch {
    pub loop_id: String,
    pub score: i32,
    pub signature: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GraphNeighborhood {
    pub upstream: Vec<String>,
    pub downstream: Vec<String>,
    pub siblings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SmartContextPanelReport {
    pub focused_loop_id: String,
    pub related_loops: Vec<RelatedLoopMatch>,
    pub fmail_mentions: Vec<FmailMatch>,
    pub relevant_commits: Vec<CommitMatch>,
    pub sibling_failures: Vec<FailureMatch>,
    pub graph: GraphNeighborhood,
}

#[must_use]
pub fn build_smart_context_panel(input: &SmartContextPanelInput) -> SmartContextPanelReport {
    let focused_loop_id = normalize_id(&input.focused_loop_id);
    if focused_loop_id.is_empty() {
        return SmartContextPanelReport::default();
    }

    let loops = input.loops.iter().map(normalize_loop).collect::<Vec<_>>();
    let fmail_threads = input
        .fmail_threads
        .iter()
        .map(normalize_fmail)
        .collect::<Vec<_>>();
    let commits = input
        .commits
        .iter()
        .map(normalize_commit)
        .collect::<Vec<_>>();
    let failures = input
        .failures
        .iter()
        .map(normalize_failure)
        .collect::<Vec<_>>();
    let dependencies = input
        .dependencies
        .iter()
        .map(normalize_dependency)
        .collect::<Vec<_>>();

    let focused_loop = loops.iter().find(|item| item.loop_id == focused_loop_id);
    let focus_tokens = build_focus_tokens(&focused_loop_id, focused_loop);
    let graph = build_graph_neighborhood(&focused_loop_id, &dependencies);

    let related_loops = rank_related_loops(&focused_loop_id, &loops, &focus_tokens, &graph);
    let fmail_mentions = rank_fmail_mentions(&focused_loop_id, &focus_tokens, &fmail_threads);
    let relevant_commits = rank_relevant_commits(&focused_loop_id, &focus_tokens, &commits);
    let sibling_failures = rank_sibling_failures(&focused_loop_id, &failures);

    SmartContextPanelReport {
        focused_loop_id,
        related_loops,
        fmail_mentions,
        relevant_commits,
        sibling_failures,
        graph,
    }
}

#[must_use]
pub fn render_smart_context_panel_lines(
    report: &SmartContextPanelReport,
    width: usize,
    max_lines: usize,
) -> Vec<String> {
    if width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    lines.push(fit_width(
        &format!("Smart context focus:{}", report.focused_loop_id),
        width,
    ));

    push_section_header(&mut lines, width, "related loops");
    if report.related_loops.is_empty() {
        lines.push(fit_width("  (none)", width));
    } else {
        for item in report.related_loops.iter().take(3) {
            lines.push(fit_width(
                &format!(
                    "  {} score={} {}",
                    item.loop_id,
                    item.score,
                    item.reasons.join("; ")
                ),
                width,
            ));
        }
    }

    push_section_header(&mut lines, width, "recent fmail");
    if report.fmail_mentions.is_empty() {
        lines.push(fit_width("  (none)", width));
    } else {
        for item in report.fmail_mentions.iter().take(2) {
            lines.push(fit_width(
                &format!("  {} score={} {}", item.thread_id, item.score, item.subject),
                width,
            ));
        }
    }

    push_section_header(&mut lines, width, "git context");
    if report.relevant_commits.is_empty() {
        lines.push(fit_width("  (none)", width));
    } else {
        for item in report.relevant_commits.iter().take(2) {
            lines.push(fit_width(
                &format!(
                    "  {} score={} {}",
                    short_commit(&item.commit_id),
                    item.score,
                    item.summary
                ),
                width,
            ));
        }
    }

    push_section_header(&mut lines, width, "sibling failures");
    if report.sibling_failures.is_empty() {
        lines.push(fit_width("  (none)", width));
    } else {
        for item in report.sibling_failures.iter().take(2) {
            lines.push(fit_width(
                &format!("  {} score={} {}", item.loop_id, item.score, item.signature),
                width,
            ));
        }
    }

    push_section_header(&mut lines, width, "graph");
    lines.push(fit_width(
        &format!("  upstream:{}", join_or_none(&report.graph.upstream)),
        width,
    ));
    lines.push(fit_width(
        &format!("  downstream:{}", join_or_none(&report.graph.downstream)),
        width,
    ));
    lines.push(fit_width(
        &format!("  siblings:{}", join_or_none(&report.graph.siblings)),
        width,
    ));

    lines.into_iter().take(max_lines).collect()
}

fn rank_related_loops(
    focused_loop_id: &str,
    loops: &[ContextLoopSample],
    focus_tokens: &HashSet<String>,
    graph: &GraphNeighborhood,
) -> Vec<RelatedLoopMatch> {
    let graph_related = graph
        .upstream
        .iter()
        .chain(graph.downstream.iter())
        .cloned()
        .collect::<HashSet<_>>();

    let mut out = Vec::new();
    for loop_sample in loops {
        if loop_sample.loop_id == focused_loop_id || loop_sample.loop_id.is_empty() {
            continue;
        }

        let mut score = 0;
        let mut reasons = Vec::new();

        let mut tokens = tokenize_set(&loop_sample.summary);
        tokens.extend(loop_sample.files.iter().flat_map(|item| tokenize(item)));
        tokens.extend(loop_sample.crates.iter().flat_map(|item| tokenize(item)));

        let overlap = overlap_count(focus_tokens, &tokens);
        if overlap > 0 {
            score += (overlap as i32) * 3;
            reasons.push(format!("semantic-overlap:{}", overlap));
        }

        let shared_files = overlap_count(
            &loop_sample
                .files
                .iter()
                .map(|item| normalize_text(item))
                .collect::<HashSet<_>>(),
            &focus_tokens
                .iter()
                .filter(|token| token.contains('/'))
                .cloned()
                .collect::<HashSet<_>>(),
        );
        if shared_files > 0 {
            score += (shared_files as i32) * 5;
            reasons.push(format!("shared-files:{}", shared_files));
        }

        if graph_related.contains(&loop_sample.loop_id) {
            score += 15;
            reasons.push("dependency-neighbor".to_owned());
        }

        if score <= 0 {
            continue;
        }

        out.push(RelatedLoopMatch {
            loop_id: loop_sample.loop_id.clone(),
            score,
            reasons,
        });
    }

    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.loop_id.cmp(&b.loop_id)));
    out
}

fn rank_fmail_mentions(
    focused_loop_id: &str,
    focus_tokens: &HashSet<String>,
    threads: &[ContextFmailThread],
) -> Vec<FmailMatch> {
    let mut out = Vec::new();
    for thread in threads {
        if thread.thread_id.is_empty() {
            continue;
        }

        let mut score = 0;
        let mut reasons = Vec::new();
        if thread
            .mentioned_loops
            .iter()
            .any(|loop_id| loop_id == focused_loop_id)
        {
            score += 30;
            reasons.push("direct-loop-mention".to_owned());
        }

        let mut tokens = tokenize_set(&thread.subject);
        tokens.extend(tokenize_set(&thread.body));
        let overlap = overlap_count(focus_tokens, &tokens);
        if overlap > 0 {
            score += (overlap as i32) * 2;
            reasons.push(format!("semantic-overlap:{}", overlap));
        }

        if score <= 0 {
            continue;
        }

        let recency_bonus = recency_bucket(thread.last_message_at_epoch_s) as i32;
        score += recency_bonus;
        if recency_bonus > 0 {
            reasons.push(format!("recent:+{}", recency_bonus));
        }

        out.push(FmailMatch {
            thread_id: thread.thread_id.clone(),
            score,
            subject: thread.subject.clone(),
            reasons,
        });
    }

    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.thread_id.cmp(&b.thread_id)));
    out
}

fn rank_relevant_commits(
    focused_loop_id: &str,
    focus_tokens: &HashSet<String>,
    commits: &[ContextCommit],
) -> Vec<CommitMatch> {
    let mut out = Vec::new();
    for commit in commits {
        if commit.commit_id.is_empty() {
            continue;
        }

        let mut score = 0;
        let mut reasons = Vec::new();
        if commit
            .touched_loops
            .iter()
            .any(|loop_id| loop_id == focused_loop_id)
        {
            score += 25;
            reasons.push("touched-focused-loop".to_owned());
        }

        let mut tokens = tokenize_set(&commit.summary);
        tokens.extend(commit.files_changed.iter().flat_map(|item| tokenize(item)));
        let overlap = overlap_count(focus_tokens, &tokens);
        if overlap > 0 {
            score += (overlap as i32) * 2;
            reasons.push(format!("semantic-overlap:{}", overlap));
        }

        if score <= 0 {
            continue;
        }

        let recency_bonus = recency_bucket(commit.authored_at_epoch_s) as i32;
        score += recency_bonus;
        if recency_bonus > 0 {
            reasons.push(format!("recent:+{}", recency_bonus));
        }

        out.push(CommitMatch {
            commit_id: commit.commit_id.clone(),
            score,
            summary: commit.summary.clone(),
            reasons,
        });
    }

    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.commit_id.cmp(&b.commit_id)));
    out
}

fn rank_sibling_failures(
    focused_loop_id: &str,
    failures: &[ContextFailureSample],
) -> Vec<FailureMatch> {
    let focus_signature = failures
        .iter()
        .rev()
        .find(|sample| sample.loop_id == focused_loop_id)
        .map(|sample| sample.signature.clone())
        .unwrap_or_default();
    let focus_tokens = tokenize_set(&focus_signature);

    let mut out = Vec::new();
    for failure in failures {
        if failure.loop_id.is_empty() || failure.loop_id == focused_loop_id {
            continue;
        }

        let mut score = 0;
        let mut reasons = Vec::new();
        let signature_overlap = overlap_count(&focus_tokens, &tokenize_set(&failure.signature));
        if signature_overlap > 0 {
            score += (signature_overlap as i32) * 4;
            reasons.push(format!("signature-overlap:{}", signature_overlap));
        }

        if score <= 0 {
            continue;
        }

        score += recency_bucket(failure.seen_at_epoch_s) as i32;
        out.push(FailureMatch {
            loop_id: failure.loop_id.clone(),
            score,
            signature: failure.signature.clone(),
            reasons,
        });
    }

    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.loop_id.cmp(&b.loop_id)));
    out
}

fn build_graph_neighborhood(
    focused_loop_id: &str,
    dependencies: &[ContextDependencyLink],
) -> GraphNeighborhood {
    let mut upstream = BTreeSet::new();
    let mut downstream = BTreeSet::new();
    let mut map = HashMap::<String, Vec<String>>::new();

    for link in dependencies {
        map.insert(link.loop_id.clone(), link.depends_on.clone());
        if link.loop_id == focused_loop_id {
            for parent in &link.depends_on {
                upstream.insert(parent.clone());
            }
        }
        if link.depends_on.iter().any(|dep| dep == focused_loop_id) {
            downstream.insert(link.loop_id.clone());
        }
    }

    let mut siblings = BTreeSet::new();
    for loop_id in map.keys() {
        if loop_id == focused_loop_id {
            continue;
        }
        let deps = map.get(loop_id).cloned().unwrap_or_default();
        if deps.iter().any(|dep| upstream.contains(dep)) {
            siblings.insert(loop_id.clone());
        }
    }

    GraphNeighborhood {
        upstream: upstream.into_iter().collect(),
        downstream: downstream.into_iter().collect(),
        siblings: siblings.into_iter().collect(),
    }
}

fn build_focus_tokens(
    focused_loop_id: &str,
    focused: Option<&ContextLoopSample>,
) -> HashSet<String> {
    let mut tokens = tokenize_set(focused_loop_id);
    if let Some(loop_sample) = focused {
        tokens.extend(tokenize_set(&loop_sample.summary));
        tokens.extend(loop_sample.files.iter().flat_map(|item| tokenize(item)));
        tokens.extend(loop_sample.crates.iter().flat_map(|item| tokenize(item)));
    }
    tokens
}

fn normalize_loop(sample: &ContextLoopSample) -> ContextLoopSample {
    ContextLoopSample {
        loop_id: normalize_id(&sample.loop_id),
        state: normalize_text(&sample.state),
        summary: normalize_text(&sample.summary),
        files: sample
            .files
            .iter()
            .map(|item| normalize_text(item))
            .collect(),
        crates: sample
            .crates
            .iter()
            .map(|item| normalize_text(item))
            .collect(),
    }
}

fn normalize_fmail(sample: &ContextFmailThread) -> ContextFmailThread {
    ContextFmailThread {
        thread_id: normalize_id(&sample.thread_id),
        subject: normalize_text(&sample.subject),
        body: normalize_text(&sample.body),
        mentioned_loops: sample
            .mentioned_loops
            .iter()
            .map(|item| normalize_id(item))
            .collect(),
        last_message_at_epoch_s: sample.last_message_at_epoch_s.max(0),
    }
}

fn normalize_commit(sample: &ContextCommit) -> ContextCommit {
    ContextCommit {
        commit_id: normalize_id(&sample.commit_id),
        summary: normalize_text(&sample.summary),
        files_changed: sample
            .files_changed
            .iter()
            .map(|item| normalize_text(item))
            .collect(),
        touched_loops: sample
            .touched_loops
            .iter()
            .map(|item| normalize_id(item))
            .collect(),
        authored_at_epoch_s: sample.authored_at_epoch_s.max(0),
    }
}

fn normalize_failure(sample: &ContextFailureSample) -> ContextFailureSample {
    ContextFailureSample {
        loop_id: normalize_id(&sample.loop_id),
        signature: normalize_text(&sample.signature),
        excerpt: normalize_text(&sample.excerpt),
        seen_at_epoch_s: sample.seen_at_epoch_s.max(0),
    }
}

fn normalize_dependency(link: &ContextDependencyLink) -> ContextDependencyLink {
    ContextDependencyLink {
        loop_id: normalize_id(&link.loop_id),
        depends_on: link
            .depends_on
            .iter()
            .map(|item| normalize_id(item))
            .collect(),
    }
}

fn overlap_count(left: &HashSet<String>, right: &HashSet<String>) -> usize {
    left.iter().filter(|token| right.contains(*token)).count()
}

fn tokenize_set(value: &str) -> HashSet<String> {
    tokenize(value).into_iter().collect()
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-' && ch != '/')
        .map(normalize_text)
        .filter(|token| token.chars().count() >= 3)
        .collect()
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn recency_bucket(epoch_s: i64) -> u8 {
    if epoch_s <= 0 {
        return 0;
    }
    // Approximate recency with deterministic coarse buckets.
    if epoch_s >= 1_900_000_000 {
        8
    } else if epoch_s >= 1_800_000_000 {
        5
    } else if epoch_s >= 1_700_000_000 {
        3
    } else {
        1
    }
}

fn push_section_header(lines: &mut Vec<String>, width: usize, title: &str) {
    lines.push(fit_width(&format!("{}:", title), width));
}

fn short_commit(value: &str) -> String {
    value.chars().take(8).collect()
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.join(",")
    }
}

fn fit_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let char_count = value.chars().count();
    if char_count <= width {
        return value.to_owned();
    }
    if width <= 3 {
        return value.chars().take(width).collect();
    }
    let mut out: String = value.chars().take(width - 3).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::{
        build_smart_context_panel, render_smart_context_panel_lines, ContextCommit,
        ContextDependencyLink, ContextFailureSample, ContextFmailThread, ContextLoopSample,
        SmartContextPanelInput,
    };

    fn sample_input() -> SmartContextPanelInput {
        SmartContextPanelInput {
            focused_loop_id: "loop-a".to_owned(),
            loops: vec![
                ContextLoopSample {
                    loop_id: "loop-a".to_owned(),
                    state: "running".to_owned(),
                    summary: "triage parser timeout in crates/forge-tui/src/app.rs".to_owned(),
                    files: vec!["crates/forge-tui/src/app.rs".to_owned()],
                    crates: vec!["forge-tui".to_owned()],
                },
                ContextLoopSample {
                    loop_id: "loop-b".to_owned(),
                    state: "running".to_owned(),
                    summary: "touches app.rs and parser wiring".to_owned(),
                    files: vec!["crates/forge-tui/src/app.rs".to_owned()],
                    crates: vec!["forge-tui".to_owned()],
                },
                ContextLoopSample {
                    loop_id: "loop-c".to_owned(),
                    state: "waiting".to_owned(),
                    summary: "database migration".to_owned(),
                    files: vec!["crates/forge-db/src/lib.rs".to_owned()],
                    crates: vec!["forge-db".to_owned()],
                },
            ],
            fmail_threads: vec![
                ContextFmailThread {
                    thread_id: "t-1".to_owned(),
                    subject: "loop-a parser timeout".to_owned(),
                    body: "Need app.rs logs and parser stacktrace".to_owned(),
                    mentioned_loops: vec!["loop-a".to_owned()],
                    last_message_at_epoch_s: 1_900_000_123,
                },
                ContextFmailThread {
                    thread_id: "t-2".to_owned(),
                    subject: "general update".to_owned(),
                    body: "no relation".to_owned(),
                    mentioned_loops: vec![],
                    last_message_at_epoch_s: 1_600_000_000,
                },
            ],
            commits: vec![
                ContextCommit {
                    commit_id: "abc12345def".to_owned(),
                    summary: "fix parser timeout in app.rs".to_owned(),
                    files_changed: vec!["crates/forge-tui/src/app.rs".to_owned()],
                    touched_loops: vec!["loop-a".to_owned()],
                    authored_at_epoch_s: 1_900_000_120,
                },
                ContextCommit {
                    commit_id: "zzz999".to_owned(),
                    summary: "docs update".to_owned(),
                    files_changed: vec!["docs/guide.md".to_owned()],
                    touched_loops: vec![],
                    authored_at_epoch_s: 1_500_000_000,
                },
            ],
            failures: vec![
                ContextFailureSample {
                    loop_id: "loop-a".to_owned(),
                    signature: "timeout parser app".to_owned(),
                    excerpt: "timeout while parsing log stream".to_owned(),
                    seen_at_epoch_s: 1_900_000_100,
                },
                ContextFailureSample {
                    loop_id: "loop-b".to_owned(),
                    signature: "parser timeout".to_owned(),
                    excerpt: "parser timeout in worker".to_owned(),
                    seen_at_epoch_s: 1_900_000_110,
                },
            ],
            dependencies: vec![
                ContextDependencyLink {
                    loop_id: "loop-a".to_owned(),
                    depends_on: vec!["loop-c".to_owned()],
                },
                ContextDependencyLink {
                    loop_id: "loop-b".to_owned(),
                    depends_on: vec!["loop-c".to_owned(), "loop-a".to_owned()],
                },
            ],
        }
    }

    #[test]
    fn related_loops_prioritize_semantic_and_dependency_overlap() {
        let report = build_smart_context_panel(&sample_input());
        assert_eq!(report.focused_loop_id, "loop-a");
        assert_eq!(report.related_loops[0].loop_id, "loop-b");
        assert!(report.related_loops[0].score > 0);
        assert!(report.related_loops[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("dependency-neighbor")));
    }

    #[test]
    fn fmail_mentions_prioritize_direct_loop_reference() {
        let report = build_smart_context_panel(&sample_input());
        assert_eq!(report.fmail_mentions[0].thread_id, "t-1");
        assert!(report.fmail_mentions[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("direct-loop-mention")));
    }

    #[test]
    fn commit_and_failure_context_are_ranked() {
        let report = build_smart_context_panel(&sample_input());
        assert_eq!(report.relevant_commits[0].commit_id, "abc12345def");
        assert_eq!(report.sibling_failures[0].loop_id, "loop-b");
        assert!(report.sibling_failures[0].score > 0);
    }

    #[test]
    fn graph_neighborhood_tracks_upstream_downstream_and_siblings() {
        let report = build_smart_context_panel(&sample_input());
        assert_eq!(report.graph.upstream, vec!["loop-c".to_owned()]);
        assert_eq!(report.graph.downstream, vec!["loop-b".to_owned()]);
        assert_eq!(report.graph.siblings, vec!["loop-b".to_owned()]);
    }

    #[test]
    fn render_contains_all_context_sections() {
        let report = build_smart_context_panel(&sample_input());
        let lines = render_smart_context_panel_lines(&report, 120, 20);

        assert!(lines
            .iter()
            .any(|line| line.contains("Smart context focus:loop-a")));
        assert!(lines.iter().any(|line| line.contains("related loops:")));
        assert!(lines.iter().any(|line| line.contains("recent fmail:")));
        assert!(lines.iter().any(|line| line.contains("git context:")));
        assert!(lines.iter().any(|line| line.contains("sibling failures:")));
        assert!(lines.iter().any(|line| line.contains("graph:")));
    }
}
