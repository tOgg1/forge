use std::io::{IsTerminal, Write};
use std::thread;
use std::time::Duration;

use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::Store;
use fmail_core::store::TopicSummary;
use fmail_tui::{build_threads, summarize_thread, ThreadMessage, ThreadSummary};
use serde_json::Value;

#[derive(Debug, Clone)]
struct LiveMailboxSnapshot {
    agents: Vec<AgentRecord>,
    topics: Vec<TopicSummary>,
    total_messages: usize,
    total_threads: usize,
    latest_thread: Option<ThreadSummary>,
}

fn main() {
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if interactive {
        loop {
            print!("\x1b[2J\x1b[H");
            let _ = std::io::stdout().flush();
            render_snapshot();
            println!();
            println!("refresh: 2s   exit: Ctrl+C");
            thread::sleep(Duration::from_secs(2));
        }
    } else {
        render_snapshot();
    }
}

fn render_snapshot() {
    let frame = fmail_tui::bootstrap_frame();
    println!("{}", frame.snapshot());
    println!();
    println!("fmail snapshot (rust)");
    println!();

    let cwd = match std::env::current_dir() {
        Ok(path) => path,
        Err(err) => {
            println!("error: read current directory: {err}");
            return;
        }
    };

    let store = match Store::new(&cwd) {
        Ok(store) => store,
        Err(err) => {
            println!("error: initialize fmail store: {err}");
            return;
        }
    };

    let snapshot = match load_live_mailbox_snapshot(&store) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            println!("error: load live mailbox snapshot: {err}");
            return;
        }
    };

    println!("agents: {}", snapshot.agents.len());
    println!("topics: {}", snapshot.topics.len());
    println!("messages: {}", snapshot.total_messages);
    println!("threads: {}", snapshot.total_threads);
    if let Some(thread) = snapshot.latest_thread.as_ref() {
        println!(
            "latest thread: {} ({} msgs, {} participants, last {})",
            trim(&thread.title, 32),
            thread.message_count,
            thread.participant_count,
            thread.last_activity
        );
    }
    println!();

    if !snapshot.agents.is_empty() {
        println!("AGENTS");
        for agent in snapshot.agents.into_iter().take(20) {
            let status = agent.status.unwrap_or_else(|| "unknown".to_string());
            println!(
                "  {:<24} {:<12} {}",
                trim(&agent.name, 24),
                trim(&status, 12),
                agent.host.unwrap_or_default()
            );
        }
        println!();
    }

    if snapshot.topics.is_empty() {
        println!("No fmail topics found");
        return;
    }

    println!("{:<24} {:>8} LAST_ACTIVITY", "TOPIC", "MESSAGES");
    for topic in snapshot.topics.into_iter().take(40) {
        let last_activity = topic
            .last_activity
            .map(|value| value.to_rfc3339())
            .unwrap_or_default();
        println!(
            "{:<24} {:>8} {}",
            trim(&topic.name, 24),
            topic.messages,
            last_activity
        );
    }
}

fn load_live_mailbox_snapshot(store: &Store) -> Result<LiveMailboxSnapshot, String> {
    let topics = store.list_topics()?;
    let agents = store.list_agent_records()?.unwrap_or_default();
    let messages = load_messages(store)?;

    let thread_messages: Vec<ThreadMessage> = messages.iter().map(to_thread_message).collect();
    let threads = build_threads(&thread_messages);
    let latest_thread = threads
        .iter()
        .max_by(|a, b| a.last_activity.cmp(&b.last_activity))
        .map(summarize_thread);

    Ok(LiveMailboxSnapshot {
        agents,
        topics,
        total_messages: thread_messages.len(),
        total_threads: threads.len(),
        latest_thread,
    })
}

fn load_messages(store: &Store) -> Result<Vec<Message>, String> {
    let mut messages = Vec::new();
    for path in store.list_all_message_files()? {
        messages.push(store.read_message(&path)?);
    }
    Ok(messages)
}

fn to_thread_message(message: &Message) -> ThreadMessage {
    let mut thread_message = ThreadMessage::new(
        &message.id,
        &message.from,
        &message.to,
        &message_timestamp(message),
        &message_body_text(&message.body),
    );
    thread_message.reply_to = message.reply_to.clone();
    thread_message.priority = message.priority.clone();
    thread_message.tags = message.tags.clone();
    thread_message
}

fn message_timestamp(message: &Message) -> String {
    if message.id.len() >= 15 {
        return message.id[..15].to_string();
    }
    message.time.format("%Y%m%d-%H%M%S").to_string()
}

fn message_body_text(body: &Value) -> String {
    match body {
        Value::String(text) => text.clone(),
        Value::Null => "null".to_string(),
        _ => serde_json::to_string(body).unwrap_or_else(|_| "<invalid-json>".to_string()),
    }
}

fn trim(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    if max <= 1 {
        return value.chars().take(max).collect();
    }
    let mut out: String = value.chars().take(max - 1).collect();
    out.push('~');
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::unwrap_used)]

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use fmail_core::message::Message;
    use serde_json::json;

    use super::{load_live_mailbox_snapshot, Store};

    #[test]
    fn live_snapshot_includes_thread_state_from_store_messages() {
        let root = temp_project_dir("live-snapshot-thread");
        let store = Store::new(&root).expect("init store");

        let now = message("seed", "task", json!("seed")).time;
        store
            .register_agent_record("architect", "devbox", now)
            .expect("register architect");
        store
            .register_agent_record("coder", "devbox", now)
            .expect("register coder");

        let mut root_msg = message("architect", "task", json!("root"));
        store
            .save_message(&mut root_msg, now)
            .expect("save root message");

        let mut reply_msg = message("coder", "task", json!("reply"));
        reply_msg.reply_to = root_msg.id.clone();
        store
            .save_message(&mut reply_msg, now)
            .expect("save reply message");

        let snapshot = load_live_mailbox_snapshot(&store).expect("load snapshot");
        assert_eq!(snapshot.agents.len(), 2);
        assert_eq!(snapshot.total_messages, 2);
        assert_eq!(snapshot.total_threads, 1);

        let latest = snapshot.latest_thread.expect("latest thread");
        assert_eq!(latest.message_count, 2);
        assert_eq!(latest.participant_count, 2);

        cleanup_temp_dir(&root);
    }

    #[test]
    fn live_snapshot_refreshes_after_new_message() {
        let root = temp_project_dir("live-snapshot-refresh");
        let store = Store::new(&root).expect("init store");
        let now = message("seed", "task", json!("seed")).time;

        let mut first = message("architect", "task", json!("first"));
        store
            .save_message(&mut first, now)
            .expect("save first message");

        let before = load_live_mailbox_snapshot(&store).expect("load before");
        assert_eq!(before.total_messages, 1);

        let mut second = message("coder", "task", json!("second"));
        store
            .save_message(&mut second, now)
            .expect("save second message");

        let after = load_live_mailbox_snapshot(&store).expect("load after");
        assert_eq!(after.total_messages, 2);

        cleanup_temp_dir(&root);
    }

    fn message(from: &str, to: &str, body: serde_json::Value) -> Message {
        Message {
            id: String::new(),
            from: from.to_string(),
            to: to.to_string(),
            time: Default::default(),
            body,
            reply_to: String::new(),
            priority: String::new(),
            host: String::new(),
            tags: Vec::new(),
        }
    }

    fn temp_project_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("fmail-tui-{tag}-{nanos}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn cleanup_temp_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }
}
