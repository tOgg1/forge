//! Incremental global search index across loops, runs, tasks, and logs.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SearchEntityKind {
    Loop,
    Run,
    Task,
    Log,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDocument {
    pub id: String,
    pub kind: SearchEntityKind,
    pub title: String,
    pub body: String,
    pub repo: Option<String>,
    pub profile: Option<String>,
    pub tags: Vec<String>,
    pub updated_at_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchFilter {
    pub repo: Option<String>,
    pub profile: Option<String>,
    pub required_tags: Vec<String>,
    pub kinds: Vec<SearchEntityKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub query: String,
    pub filter: SearchFilter,
    pub limit: usize,
    pub now_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub id: String,
    pub kind: SearchEntityKind,
    pub title: String,
    pub snippet: String,
    pub repo: Option<String>,
    pub profile: Option<String>,
    pub tags: Vec<String>,
    pub updated_at_epoch_s: i64,
    pub score: i64,
}

#[derive(Debug, Default, Clone)]
pub struct GlobalSearchIndex {
    documents: BTreeMap<String, SearchDocument>,
    doc_tokens: BTreeMap<String, BTreeSet<String>>,
    token_postings: BTreeMap<String, BTreeSet<String>>,
}

impl GlobalSearchIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, document: SearchDocument) {
        let doc_id = normalize_id(&document.id);
        if doc_id.is_empty() {
            return;
        }
        self.remove(&doc_id);

        let normalized = normalize_document(document, &doc_id);
        let tokens = document_tokens(&normalized);

        for token in &tokens {
            self.token_postings
                .entry(token.clone())
                .or_default()
                .insert(doc_id.clone());
        }
        self.doc_tokens.insert(doc_id.clone(), tokens);
        self.documents.insert(doc_id, normalized);
    }

    pub fn remove(&mut self, document_id: &str) {
        let doc_id = normalize_id(document_id);
        if doc_id.is_empty() {
            return;
        }
        if let Some(tokens) = self.doc_tokens.remove(&doc_id) {
            for token in tokens {
                if let Some(postings) = self.token_postings.get_mut(&token) {
                    postings.remove(&doc_id);
                    if postings.is_empty() {
                        self.token_postings.remove(&token);
                    }
                }
            }
        }
        self.documents.remove(&doc_id);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    #[must_use]
    pub fn search(&self, request: &SearchRequest) -> Vec<SearchHit> {
        let query_terms = query_terms(&request.query);
        let filter = NormalizedFilter::from_request(&request.filter);
        let now_epoch_s = request.now_epoch_s.max(0);
        let limit = if request.limit == 0 {
            20
        } else {
            request.limit
        };

        let candidate_ids = if query_terms.is_empty() {
            self.documents.keys().cloned().collect::<BTreeSet<_>>()
        } else {
            self.collect_candidate_ids(&query_terms)
        };

        let mut hits = Vec::new();
        for doc_id in candidate_ids {
            let Some(doc) = self.documents.get(&doc_id) else {
                continue;
            };
            if !filter.matches(doc) {
                continue;
            }
            let tokens = self
                .doc_tokens
                .get(&doc_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect::<Vec<_>>();
            let relevance = relevance_score(doc, &tokens, &query_terms);
            if !query_terms.is_empty() && relevance == 0 {
                continue;
            }
            let recency = recency_score(now_epoch_s, doc.updated_at_epoch_s);
            let score = relevance * 100 + recency;
            hits.push(SearchHit {
                id: doc.id.clone(),
                kind: doc.kind,
                title: doc.title.clone(),
                snippet: snippet_for(doc, &query_terms),
                repo: doc.repo.clone(),
                profile: doc.profile.clone(),
                tags: doc.tags.clone(),
                updated_at_epoch_s: doc.updated_at_epoch_s,
                score,
            });
        }

        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(b.updated_at_epoch_s.cmp(&a.updated_at_epoch_s))
                .then(a.id.cmp(&b.id))
        });
        hits.truncate(limit);
        hits
    }

    fn collect_candidate_ids(&self, query_terms: &[String]) -> BTreeSet<String> {
        let mut candidates = BTreeSet::new();
        for query in query_terms {
            let mut term_candidates = BTreeSet::new();
            for (token, postings) in &self.token_postings {
                if token.contains(query) {
                    term_candidates.extend(postings.iter().cloned());
                }
            }
            if candidates.is_empty() {
                candidates = term_candidates;
            } else {
                candidates.extend(term_candidates);
            }
        }
        candidates
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedFilter {
    repo: Option<String>,
    profile: Option<String>,
    required_tags: BTreeSet<String>,
    kinds: BTreeSet<SearchEntityKind>,
}

impl NormalizedFilter {
    fn from_request(request: &SearchFilter) -> Self {
        Self {
            repo: request.repo.as_deref().map(normalize_field),
            profile: request.profile.as_deref().map(normalize_field),
            required_tags: request
                .required_tags
                .iter()
                .map(|tag| normalize_field(tag))
                .filter(|tag| !tag.is_empty())
                .collect(),
            kinds: request.kinds.iter().copied().collect(),
        }
    }

    fn matches(&self, doc: &SearchDocument) -> bool {
        if let Some(repo) = self.repo.as_deref() {
            let doc_repo = doc.repo.as_deref().map(normalize_field).unwrap_or_default();
            if doc_repo != repo {
                return false;
            }
        }
        if let Some(profile) = self.profile.as_deref() {
            let doc_profile = doc
                .profile
                .as_deref()
                .map(normalize_field)
                .unwrap_or_default();
            if doc_profile != profile {
                return false;
            }
        }
        if !self.kinds.is_empty() && !self.kinds.contains(&doc.kind) {
            return false;
        }
        if !self.required_tags.is_empty() {
            let doc_tags = doc
                .tags
                .iter()
                .map(|tag| normalize_field(tag))
                .collect::<BTreeSet<_>>();
            for required in &self.required_tags {
                if !doc_tags.contains(required) {
                    return false;
                }
            }
        }
        true
    }
}

fn normalize_document(mut doc: SearchDocument, doc_id: &str) -> SearchDocument {
    doc.id = doc_id.to_owned();
    doc.title = if doc.title.trim().is_empty() {
        format!("Item {}", doc_id)
    } else {
        doc.title.trim().to_owned()
    };
    doc.body = doc.body.trim().to_owned();
    doc.repo = doc
        .repo
        .as_deref()
        .map(normalize_field)
        .filter(|v| !v.is_empty());
    doc.profile = doc
        .profile
        .as_deref()
        .map(normalize_field)
        .filter(|v| !v.is_empty());
    doc.tags = doc
        .tags
        .iter()
        .map(|tag| normalize_field(tag))
        .filter(|tag| !tag.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    doc
}

fn document_tokens(doc: &SearchDocument) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    tokens.extend(tokenize(&doc.id));
    tokens.extend(tokenize(&doc.title));
    tokens.extend(tokenize(&doc.body));
    if let Some(repo) = doc.repo.as_deref() {
        tokens.extend(tokenize(repo));
    }
    if let Some(profile) = doc.profile.as_deref() {
        tokens.extend(tokenize(profile));
    }
    for tag in &doc.tags {
        tokens.extend(tokenize(tag));
    }
    tokens
}

fn query_terms(query: &str) -> Vec<String> {
    tokenize(query).into_iter().collect()
}

fn relevance_score(doc: &SearchDocument, tokens: &[String], query_terms: &[String]) -> i64 {
    if query_terms.is_empty() {
        return 1;
    }
    let id_text = normalize_field(&doc.id);
    let title_text = normalize_field(&doc.title);
    let body_text = normalize_field(&doc.body);
    let mut score = 0_i64;

    for term in query_terms {
        let mut term_score = 0_i64;
        if id_text.starts_with(term) {
            term_score += 65;
        } else if id_text.contains(term) {
            term_score += 45;
        }
        if title_text.starts_with(term) {
            term_score += 55;
        } else if title_text.contains(term) {
            term_score += 35;
        }
        if body_text.contains(term) {
            term_score += 15;
        }
        for token in tokens {
            if token == term {
                term_score += 35;
            } else if token.starts_with(term) {
                term_score += 20;
            } else if token.contains(term) {
                term_score += 8;
            }
        }
        if term_score > 0 {
            score += term_score;
        }
    }
    score
}

fn recency_score(now_epoch_s: i64, updated_at_epoch_s: i64) -> i64 {
    if now_epoch_s <= 0 || updated_at_epoch_s <= 0 {
        return 0;
    }
    if updated_at_epoch_s >= now_epoch_s {
        return 300;
    }
    let age_secs = now_epoch_s - updated_at_epoch_s;
    if age_secs >= 7 * 24 * 3_600 {
        return 0;
    }
    (7 * 24 * 3_600 - age_secs) / 1_800
}

fn snippet_for(doc: &SearchDocument, query_terms: &[String]) -> String {
    if doc.body.is_empty() {
        return doc.title.clone();
    }
    let normalized_body = normalize_field(&doc.body);
    for term in query_terms {
        if let Some(index) = normalized_body.find(term) {
            let start = index.saturating_sub(24);
            let end = (index + term.len() + 56).min(doc.body.len());
            return ellipsize(&doc.body[start..end], 96);
        }
    }
    ellipsize(&doc.body, 96)
}

fn ellipsize(value: &str, max_len: usize) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= max_len {
        trimmed.to_owned()
    } else {
        let head = &trimmed[..max_len.saturating_sub(3)];
        format!("{head}...")
    }
}

fn tokenize(value: &str) -> BTreeSet<String> {
    normalize_field(value)
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_field(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        } else {
            normalized.push(' ');
        }
    }
    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{GlobalSearchIndex, SearchDocument, SearchEntityKind, SearchFilter, SearchRequest};

    fn request(query: &str) -> SearchRequest {
        SearchRequest {
            query: query.to_owned(),
            filter: SearchFilter::default(),
            limit: 10,
            now_epoch_s: 200_000,
        }
    }

    #[test]
    fn upsert_and_partial_match_find_document() {
        let mut index = GlobalSearchIndex::new();
        index.upsert(SearchDocument {
            id: "task-123".to_owned(),
            kind: SearchEntityKind::Task,
            title: "Fix payload parser".to_owned(),
            body: "Parser fails on partial payload tokens".to_owned(),
            repo: Some("forge".to_owned()),
            profile: Some("ops".to_owned()),
            tags: vec!["bug".to_owned()],
            updated_at_epoch_s: 199_900,
        });

        let hits = index.search(&request("pay"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "task-123");
        assert!(hits[0].score > 0);
    }

    #[test]
    fn filters_by_repo_profile_tags_and_kind() {
        let mut index = GlobalSearchIndex::new();
        index.upsert(SearchDocument {
            id: "loop-a".to_owned(),
            kind: SearchEntityKind::Loop,
            title: "Loop A".to_owned(),
            body: "Repo A".to_owned(),
            repo: Some("repo-a".to_owned()),
            profile: Some("night".to_owned()),
            tags: vec!["p1".to_owned(), "infra".to_owned()],
            updated_at_epoch_s: 150_000,
        });
        index.upsert(SearchDocument {
            id: "loop-b".to_owned(),
            kind: SearchEntityKind::Loop,
            title: "Loop B".to_owned(),
            body: "Repo B".to_owned(),
            repo: Some("repo-b".to_owned()),
            profile: Some("day".to_owned()),
            tags: vec!["p2".to_owned()],
            updated_at_epoch_s: 150_000,
        });

        let hits = index.search(&SearchRequest {
            query: "loop".to_owned(),
            filter: SearchFilter {
                repo: Some("repo-a".to_owned()),
                profile: Some("night".to_owned()),
                required_tags: vec!["infra".to_owned()],
                kinds: vec![SearchEntityKind::Loop],
            },
            limit: 10,
            now_epoch_s: 200_000,
        });

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "loop-a");
    }

    #[test]
    fn ranking_prefers_relevance_then_recency() {
        let mut index = GlobalSearchIndex::new();
        index.upsert(SearchDocument {
            id: "task-exact".to_owned(),
            kind: SearchEntityKind::Task,
            title: "global search index".to_owned(),
            body: "exact phrase in title".to_owned(),
            repo: None,
            profile: None,
            tags: vec![],
            updated_at_epoch_s: 150_000,
        });
        index.upsert(SearchDocument {
            id: "task-recent".to_owned(),
            kind: SearchEntityKind::Task,
            title: "misc".to_owned(),
            body: "global search appears in body only".to_owned(),
            repo: None,
            profile: None,
            tags: vec![],
            updated_at_epoch_s: 199_990,
        });

        let hits = index.search(&request("global search"));
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "task-exact");
        assert_eq!(hits[1].id, "task-recent");
    }

    #[test]
    fn upsert_replaces_old_tokens_incrementally() {
        let mut index = GlobalSearchIndex::new();
        index.upsert(SearchDocument {
            id: "run-1".to_owned(),
            kind: SearchEntityKind::Run,
            title: "old title".to_owned(),
            body: "contains alpha".to_owned(),
            repo: None,
            profile: None,
            tags: vec![],
            updated_at_epoch_s: 100,
        });
        index.upsert(SearchDocument {
            id: "run-1".to_owned(),
            kind: SearchEntityKind::Run,
            title: "new title".to_owned(),
            body: "contains beta".to_owned(),
            repo: None,
            profile: None,
            tags: vec![],
            updated_at_epoch_s: 200,
        });

        assert!(index.search(&request("alpha")).is_empty());
        let hits = index.search(&request("beta"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "run-1");
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn remove_deletes_document_from_index() {
        let mut index = GlobalSearchIndex::new();
        index.upsert(SearchDocument {
            id: "log-1".to_owned(),
            kind: SearchEntityKind::Log,
            title: "daemon log".to_owned(),
            body: "warning line".to_owned(),
            repo: None,
            profile: None,
            tags: vec![],
            updated_at_epoch_s: 100,
        });
        index.remove("log-1");
        assert_eq!(index.len(), 0);
        assert!(index.search(&request("warning")).is_empty());
    }
}
