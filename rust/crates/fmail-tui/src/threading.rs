//! Threading engine: builds conversation trees from flat message lists.
//!
//! Ports Go `internal/fmailtui/threading/threading.go` with full parity:
//! `BuildThreads`, `BuildThread`, `FlattenThread`, `SummarizeThread`,
//! `IsCrossTargetReply`.

use std::collections::{HashMap, HashSet};

const MAX_DISPLAY_DEPTH: usize = 10;

/// A single message used for threading. Mirrors the Go `fmail.Message` fields
/// that the threading engine needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub timestamp: String,
    pub body: String,
    pub reply_to: String,
    pub priority: String,
    pub tags: Vec<String>,
    pub host: String,
}

impl ThreadMessage {
    #[must_use]
    pub fn new(id: &str, from: &str, to: &str, timestamp: &str, body: &str) -> Self {
        Self {
            id: id.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            timestamp: timestamp.to_owned(),
            body: body.to_owned(),
            reply_to: String::new(),
            priority: String::new(),
            tags: Vec::new(),
            host: String::new(),
        }
    }
}

/// A threaded conversation rooted at a single message.
#[derive(Debug, Clone)]
pub struct Thread {
    pub root_id: String,
    pub root_msg: ThreadMessage,
    pub nodes: Vec<ThreadNode>,
    pub depth: usize,
    pub agents: Vec<String>,
    pub last_activity: String,
}

/// A single node in the thread tree.
#[derive(Debug, Clone)]
pub struct ThreadNode {
    pub message: ThreadMessage,
    pub parent_id: Option<String>,
    pub children_ids: Vec<String>,
    pub depth: usize,
}

/// Summary of a thread for list views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSummary {
    pub title: String,
    pub participant_count: usize,
    pub message_count: usize,
    pub last_activity: String,
}

/// Build threads from a flat list of messages. Returns threads sorted by
/// earliest root message.
#[must_use]
pub fn build_threads(messages: &[ThreadMessage]) -> Vec<Thread> {
    let (node_map, ordered_ids) = index_and_link(messages);

    // Find roots (nodes with no parent).
    let mut root_ids: Vec<&str> = ordered_ids
        .iter()
        .filter(|id| {
            node_map
                .get(id.as_str())
                .map_or(true, |n| n.parent_id.is_none())
        })
        .map(String::as_str)
        .collect();

    // Sort roots chronologically (stable).
    root_ids.sort_by(|a, b| {
        let ma = node_map.get(*a);
        let mb = node_map.get(*b);
        message_less_opt(ma, mb)
    });

    root_ids
        .iter()
        .filter_map(|root_id| build_thread_from(&node_map, root_id))
        .collect()
}

/// Build a single thread from a root message ID. Walks up the parent chain to
/// find the true root.
#[must_use]
pub fn build_thread_by_id(messages: &[ThreadMessage], root_id: &str) -> Option<Thread> {
    let root_id = root_id.trim();
    if root_id.is_empty() {
        return None;
    }

    let (node_map, _) = index_and_link(messages);

    let start = node_map.get(root_id)?;

    // Walk up to find the true root, breaking cycles.
    let mut current_id = root_id.to_owned();
    let mut seen = HashSet::new();
    loop {
        let node = node_map.get(current_id.as_str())?;
        if let Some(ref pid) = node.parent_id {
            if seen.contains(pid.as_str()) {
                break;
            }
            seen.insert(current_id.clone());
            current_id = pid.clone();
        } else {
            break;
        }
    }
    let _ = start; // used above implicitly

    build_thread_from(&node_map, &current_id)
}

/// Flatten a thread into display order (depth-first, chronological siblings).
#[must_use]
pub fn flatten_thread(thread: &Thread) -> Vec<&ThreadNode> {
    if thread.nodes.is_empty() {
        return Vec::new();
    }

    let node_map: HashMap<&str, &ThreadNode> = thread
        .nodes
        .iter()
        .map(|n| (n.message.id.as_str(), n))
        .collect();

    // Find the root node.
    let root = node_map
        .get(thread.root_id.as_str())
        .copied()
        .or_else(|| thread.nodes.first());

    let Some(root) = root else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(thread.nodes.len());
    walk_dfs(root, &node_map, &mut out);
    out
}

fn walk_dfs<'a>(
    node: &'a ThreadNode,
    node_map: &HashMap<&str, &'a ThreadNode>,
    out: &mut Vec<&'a ThreadNode>,
) {
    out.push(node);
    if node.children_ids.is_empty() {
        return;
    }
    // Collect children and sort chronologically.
    let mut children: Vec<&ThreadNode> = node
        .children_ids
        .iter()
        .filter_map(|cid| node_map.get(cid.as_str()).copied())
        .collect();
    children.sort_by(|a, b| message_less_cmp(&a.message, &b.message));
    for child in children {
        walk_dfs(child, node_map, out);
    }
}

/// Summarize a thread for list displays.
#[must_use]
pub fn summarize_thread(thread: &Thread) -> ThreadSummary {
    ThreadSummary {
        title: first_line(&thread.root_msg.body),
        participant_count: thread.agents.len(),
        message_count: thread.nodes.len(),
        last_activity: thread.last_activity.clone(),
    }
}

/// Check if a node's reply crosses targets (topics/DMs).
#[must_use]
pub fn is_cross_target_reply(node: &ThreadNode, parent: Option<&ThreadNode>) -> bool {
    let Some(parent) = parent else {
        return false;
    };
    node.message.to.trim() != parent.message.to.trim()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

struct InternalNode {
    message: ThreadMessage,
    parent_id: Option<String>,
    children_ids: Vec<String>,
    depth: usize,
}

fn index_and_link(messages: &[ThreadMessage]) -> (HashMap<String, ThreadNode>, Vec<String>) {
    // Index all messages by ID.
    let mut nodes: HashMap<String, InternalNode> = HashMap::with_capacity(messages.len());
    let mut all_ids: Vec<String> = Vec::with_capacity(messages.len());

    for msg in messages {
        let id = msg.id.trim();
        if id.is_empty() {
            continue;
        }
        all_ids.push(id.to_owned());
        nodes.insert(
            id.to_owned(),
            InternalNode {
                message: msg.clone(),
                parent_id: None,
                children_ids: Vec::new(),
                depth: 0,
            },
        );
    }

    // Sort chronologically for deterministic linking.
    all_ids.sort_by(|a, b| {
        let ma = nodes.get(a);
        let mb = nodes.get(b);
        match (ma, mb) {
            (Some(a), Some(b)) => message_less_cmp(&a.message, &b.message),
            _ => std::cmp::Ordering::Equal,
        }
    });

    // Link parents.
    for id in &all_ids {
        let reply_to = {
            let Some(node) = nodes.get(id.as_str()) else {
                continue;
            };
            let rt = node.message.reply_to.trim().to_owned();
            if rt.is_empty() || rt == *id {
                continue;
            }
            rt
        };

        // Check parent exists.
        if !nodes.contains_key(&reply_to) {
            continue;
        }

        // Check for cycle.
        if would_create_cycle(&nodes, id, &reply_to) {
            continue;
        }

        // Link.
        if let Some(node) = nodes.get_mut(id.as_str()) {
            node.parent_id = Some(reply_to.clone());
        }
        if let Some(parent) = nodes.get_mut(&reply_to) {
            parent.children_ids.push(id.clone());
        }
    }

    // Assign depths.
    for id in &all_ids {
        let depth = compute_depth(&nodes, id);
        if let Some(node) = nodes.get_mut(id.as_str()) {
            node.depth = depth;
        }
    }

    // Convert to public ThreadNode.
    let result: HashMap<String, ThreadNode> = nodes
        .into_iter()
        .map(|(id, n)| {
            (
                id,
                ThreadNode {
                    message: n.message,
                    parent_id: n.parent_id,
                    children_ids: n.children_ids,
                    depth: n.depth,
                },
            )
        })
        .collect();

    (result, all_ids)
}

fn would_create_cycle(
    nodes: &HashMap<String, InternalNode>,
    node_id: &str,
    parent_id: &str,
) -> bool {
    let mut cur = parent_id.to_owned();
    let mut seen = HashSet::new();
    loop {
        if cur == node_id {
            return true;
        }
        if seen.contains(&cur) {
            break;
        }
        seen.insert(cur.clone());
        let Some(n) = nodes.get(&cur) else {
            break;
        };
        let Some(ref pid) = n.parent_id else {
            break;
        };
        cur = pid.clone();
    }
    false
}

fn compute_depth(nodes: &HashMap<String, InternalNode>, id: &str) -> usize {
    let mut depth = 0;
    let mut cur = id.to_owned();
    loop {
        let Some(n) = nodes.get(&cur) else {
            break;
        };
        let Some(ref pid) = n.parent_id else {
            break;
        };
        depth += 1;
        if depth >= MAX_DISPLAY_DEPTH {
            return MAX_DISPLAY_DEPTH;
        }
        cur = pid.clone();
    }
    depth
}

fn build_thread_from(node_map: &HashMap<String, ThreadNode>, root_id: &str) -> Option<Thread> {
    let root_node = node_map.get(root_id)?;

    // Collect subtree via DFS.
    let mut collected = Vec::with_capacity(32);
    let mut stack = vec![root_id.to_owned()];
    let mut seen = HashSet::new();

    while let Some(id) = stack.pop() {
        if seen.contains(&id) {
            continue;
        }
        seen.insert(id.clone());
        let Some(node) = node_map.get(id.as_str()) else {
            continue;
        };
        collected.push(node.clone());
        for child_id in &node.children_ids {
            stack.push(child_id.clone());
        }
    }

    // Sort chronologically.
    collected.sort_by(|a, b| message_less_cmp(&a.message, &b.message));

    // Compute agents, last_activity, max depth.
    let mut agents_set = HashSet::new();
    let mut agents = Vec::new();
    let mut last_activity = String::new();
    let mut max_depth = 0;

    for node in &collected {
        let from = node.message.from.trim();
        if !from.is_empty() && agents_set.insert(from.to_owned()) {
            agents.push(from.to_owned());
        }
        // For DMs, include the recipient.
        let to = node.message.to.trim();
        if to.starts_with('@') {
            let peer = to.trim_start_matches('@');
            if !peer.is_empty() && agents_set.insert(peer.to_owned()) {
                agents.push(peer.to_owned());
            }
        }
        if node.message.timestamp > last_activity {
            last_activity = node.message.timestamp.clone();
        }
        if node.depth > max_depth {
            max_depth = node.depth;
        }
    }
    agents.sort();

    Some(Thread {
        root_id: root_id.to_owned(),
        root_msg: root_node.message.clone(),
        nodes: collected,
        depth: max_depth,
        agents,
        last_activity,
    })
}

fn message_less_cmp(a: &ThreadMessage, b: &ThreadMessage) -> std::cmp::Ordering {
    // Compare by timestamp, then ID, then From, then To.
    match a.timestamp.cmp(&b.timestamp) {
        std::cmp::Ordering::Equal => {}
        ord => return ord,
    }
    match a.id.cmp(&b.id) {
        std::cmp::Ordering::Equal => {}
        ord => return ord,
    }
    match a.from.cmp(&b.from) {
        std::cmp::Ordering::Equal => {}
        ord => return ord,
    }
    a.to.cmp(&b.to)
}

fn message_less_opt(a: Option<&ThreadNode>, b: Option<&ThreadNode>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => message_less_cmp(&a.message, &b.message),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn first_line(body: &str) -> String {
    let s = body.trim();
    if s.is_empty() {
        return String::new();
    }
    if let Some(idx) = s.find('\n') {
        s[..idx].trim().to_owned()
    } else {
        s.to_owned()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, from: &str, to: &str, ts: &str, body: &str) -> ThreadMessage {
        ThreadMessage::new(id, from, to, ts, body)
    }

    fn msg_reply(
        id: &str,
        from: &str,
        to: &str,
        ts: &str,
        body: &str,
        reply_to: &str,
    ) -> ThreadMessage {
        let mut m = msg(id, from, to, ts, body);
        m.reply_to = reply_to.to_owned();
        m
    }

    #[test]
    fn basic_chain() {
        let msgs = vec![
            msg(
                "20260209-080000-0001",
                "alice",
                "task",
                "20260209-080000",
                "root",
            ),
            msg_reply(
                "20260209-080001-0001",
                "bob",
                "task",
                "20260209-080001",
                "r1",
                "20260209-080000-0001",
            ),
            msg_reply(
                "20260209-080002-0001",
                "alice",
                "task",
                "20260209-080002",
                "r2",
                "20260209-080001-0001",
            ),
        ];

        let threads = build_threads(&msgs);
        assert_eq!(threads.len(), 1);
        let th = &threads[0];
        assert_eq!(th.root_id, "20260209-080000-0001");
        assert_eq!(th.depth, 2);
        assert_eq!(th.nodes.len(), 3);

        let flat = flatten_thread(th);
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].message.id, "20260209-080000-0001");
        assert_eq!(flat[1].message.id, "20260209-080001-0001");
        assert_eq!(flat[2].message.id, "20260209-080002-0001");
    }

    #[test]
    fn missing_parent_becomes_root() {
        let msgs = vec![msg_reply(
            "20260209-080000-0001",
            "alice",
            "task",
            "20260209-080000",
            "orphan",
            "missing",
        )];
        let threads = build_threads(&msgs);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].root_id, "20260209-080000-0001");
    }

    #[test]
    fn cycle_breaks_deterministically() {
        // A replies to B, B replies to A -> cycle.
        let msgs = vec![
            msg_reply(
                "20260209-080000-0001",
                "alice",
                "task",
                "20260209-080000",
                "a",
                "20260209-080001-0001",
            ),
            msg_reply(
                "20260209-080001-0001",
                "bob",
                "task",
                "20260209-080001",
                "b",
                "20260209-080000-0001",
            ),
        ];
        let threads = build_threads(&msgs);
        assert_eq!(threads.len(), 1);
        // Linking is chronological: A links to B first, then B's link to A creates cycle => dropped.
        // So B is the root (A's parent is B).
        assert_eq!(threads[0].root_id, "20260209-080001-0001");
        assert_eq!(threads[0].depth, 1);
    }

    #[test]
    fn self_reply_ignored() {
        let msgs = vec![msg_reply(
            "20260209-080000-0001",
            "alice",
            "task",
            "20260209-080000",
            "x",
            "20260209-080000-0001",
        )];
        let threads = build_threads(&msgs);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].root_id, "20260209-080000-0001");
    }

    #[test]
    fn depth_clamped() {
        let mut msgs = Vec::with_capacity(16);
        let mut prev_id = String::new();
        for i in 0..16 {
            let id = format!("20260209-0800{i:02}-0001");
            let ts = format!("20260209-0800{i:02}");
            let mut m = msg(&id, "a", "task", &ts, "x");
            if !prev_id.is_empty() {
                m.reply_to = prev_id.clone();
            }
            msgs.push(m);
            prev_id = id;
        }

        let threads = build_threads(&msgs);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].depth, MAX_DISPLAY_DEPTH);
    }

    #[test]
    fn build_thread_finds_root() {
        let msgs = vec![
            msg(
                "20260209-080000-0001",
                "alice",
                "task",
                "20260209-080000",
                "root",
            ),
            msg_reply(
                "20260209-080001-0001",
                "bob",
                "task",
                "20260209-080001",
                "r1",
                "20260209-080000-0001",
            ),
        ];
        let Some(th) = build_thread_by_id(&msgs, "20260209-080001-0001") else {
            panic!("expected thread");
        };
        assert_eq!(th.root_id, "20260209-080000-0001");
    }

    #[test]
    fn summarize_first_line() {
        let msgs = vec![msg(
            "20260209-080000-0001",
            "alice",
            "task",
            "20260209-080000",
            "hello\nworld",
        )];
        let Some(th) = build_thread_by_id(&msgs, "20260209-080000-0001") else {
            panic!("expected thread");
        };
        let sum = summarize_thread(&th);
        assert_eq!(sum.title, "hello");
        assert_eq!(sum.message_count, 1);
        assert_eq!(sum.participant_count, 1);
        assert!(!sum.last_activity.is_empty());
    }

    #[test]
    fn cross_target_reply() {
        let msgs = vec![
            msg(
                "20260209-080000-0001",
                "alice",
                "task",
                "20260209-080000",
                "root",
            ),
            msg_reply(
                "20260209-080001-0001",
                "bob",
                "build",
                "20260209-080001",
                "reply",
                "20260209-080000-0001",
            ),
        ];
        let threads = build_threads(&msgs);
        assert_eq!(threads.len(), 1);
        let flat = flatten_thread(&threads[0]);
        assert_eq!(flat.len(), 2);

        let node_map: HashMap<&str, &ThreadNode> =
            flat.iter().map(|n| (n.message.id.as_str(), *n)).collect();
        let parent = flat[1]
            .parent_id
            .as_deref()
            .and_then(|pid| node_map.get(pid).copied());
        assert!(is_cross_target_reply(flat[1], parent));
    }

    #[test]
    fn multiple_threads() {
        let msgs = vec![
            msg("1", "alice", "task", "20260209-080000", "thread A"),
            msg("2", "bob", "task", "20260209-080001", "thread B"),
            msg_reply("3", "alice", "task", "20260209-080002", "reply A", "1"),
        ];
        let threads = build_threads(&msgs);
        assert_eq!(threads.len(), 2);
        // First thread: root 1 with child 3.
        assert_eq!(threads[0].root_id, "1");
        assert_eq!(threads[0].nodes.len(), 2);
        // Second thread: root 2 standalone.
        assert_eq!(threads[1].root_id, "2");
        assert_eq!(threads[1].nodes.len(), 1);
    }

    #[test]
    fn dm_participants() {
        let msgs = vec![msg("1", "alice", "@bob", "20260209-080000", "hey")];
        let threads = build_threads(&msgs);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].agents, vec!["alice", "bob"]);
    }

    #[test]
    fn empty_messages() {
        let threads = build_threads(&[]);
        assert!(threads.is_empty());
    }

    #[test]
    fn flatten_preserves_sibling_order() {
        let msgs = vec![
            msg("r", "alice", "task", "20260209-080000", "root"),
            msg_reply("c2", "charlie", "task", "20260209-080002", "second", "r"),
            msg_reply("c1", "bob", "task", "20260209-080001", "first", "r"),
        ];
        let threads = build_threads(&msgs);
        let flat = flatten_thread(&threads[0]);
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].message.id, "r");
        assert_eq!(flat[1].message.id, "c1");
        assert_eq!(flat[2].message.id, "c2");
    }
}
