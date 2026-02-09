#![allow(clippy::expect_used, clippy::unwrap_used)]

#[cfg(unix)]
mod unix_only {
    use std::os::unix::fs::PermissionsExt;

    use chrono::{TimeZone, Utc};
    use fmail_core::message::Message;
    use fmail_core::store::Store;

    #[test]
    fn dm_message_file_is_0600() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::new(dir.path()).expect("new store");
        let now = Utc.with_ymd_and_hms(2026, 2, 9, 12, 0, 0).unwrap();

        let mut msg = Message {
            id: String::new(),
            from: "alice".to_string(),
            to: "@bob".to_string(),
            time: chrono::DateTime::<chrono::Utc>::default(),
            body: serde_json::Value::String("hi".to_string()),
            reply_to: String::new(),
            priority: String::new(),
            host: String::new(),
            tags: vec![],
        };

        let id = store.save_message(&mut msg, now).expect("save");
        let path = store.dm_message_path("bob", &id);

        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "mode={mode:o}");
    }

    #[test]
    fn topic_message_file_is_0644() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::new(dir.path()).expect("new store");
        let now = Utc.with_ymd_and_hms(2026, 2, 9, 12, 0, 0).unwrap();

        let mut msg = Message {
            id: String::new(),
            from: "alice".to_string(),
            to: "task".to_string(),
            time: chrono::DateTime::<chrono::Utc>::default(),
            body: serde_json::Value::String("hello".to_string()),
            reply_to: String::new(),
            priority: String::new(),
            host: String::new(),
            tags: vec![],
        };

        let id = store.save_message(&mut msg, now).expect("save");
        let path = store.topic_message_path("task", &id);

        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o644, "mode={mode:o}");
    }
}
