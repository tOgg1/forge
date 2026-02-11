use chrono::{DateTime, SecondsFormat, Utc};

use crate::runtime_limits::{RuntimeMetaValue, RuntimeMetadata};

pub const WAIT_UNTIL_KEY: &str = "wait_until";

pub fn set_wait_until(metadata: &mut RuntimeMetadata, until: DateTime<Utc>) {
    metadata.insert(
        WAIT_UNTIL_KEY.to_string(),
        RuntimeMetaValue::Text(until.to_rfc3339_opts(SecondsFormat::Secs, true)),
    );
}

pub fn clear_wait_until(metadata: &mut RuntimeMetadata) {
    metadata.remove(WAIT_UNTIL_KEY);
}

pub fn wait_until(metadata: Option<&RuntimeMetadata>) -> Option<DateTime<Utc>> {
    let metadata = metadata?;
    let value = metadata.get(WAIT_UNTIL_KEY)?;
    match value {
        RuntimeMetaValue::Timestamp(value) => Some(*value),
        RuntimeMetaValue::Text(value) => DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|dt| dt.with_timezone(&Utc)),
        RuntimeMetaValue::Int(_) | RuntimeMetaValue::Float(_) | RuntimeMetaValue::Object(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{clear_wait_until, set_wait_until, wait_until, WAIT_UNTIL_KEY};
    use crate::runtime_limits::{RuntimeMetaValue, RuntimeMetadata};
    use chrono::{TimeZone, Utc};

    #[test]
    fn set_wait_until_writes_rfc3339_z_text() {
        let mut meta = RuntimeMetadata::new();
        let ts = Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap();
        set_wait_until(&mut meta, ts);
        assert_eq!(
            meta.get(WAIT_UNTIL_KEY),
            Some(&RuntimeMetaValue::Text("2026-02-10T12:00:00Z".to_string()))
        );
    }

    #[test]
    fn wait_until_parses_text_and_timestamp() {
        let ts = Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap();
        let mut meta = RuntimeMetadata::new();
        meta.insert(
            WAIT_UNTIL_KEY.to_string(),
            RuntimeMetaValue::Text("2026-02-10T12:00:00Z".to_string()),
        );
        assert_eq!(wait_until(Some(&meta)), Some(ts));

        meta.insert(WAIT_UNTIL_KEY.to_string(), RuntimeMetaValue::Timestamp(ts));
        assert_eq!(wait_until(Some(&meta)), Some(ts));
    }

    #[test]
    fn clear_wait_until_removes_key() {
        let mut meta = RuntimeMetadata::new();
        meta.insert(
            WAIT_UNTIL_KEY.to_string(),
            RuntimeMetaValue::Text("2026-02-10T12:00:00Z".to_string()),
        );
        clear_wait_until(&mut meta);
        assert!(!meta.contains_key(WAIT_UNTIL_KEY));
    }
}
