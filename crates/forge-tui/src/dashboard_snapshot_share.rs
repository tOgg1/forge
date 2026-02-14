//! Shareable dashboard snapshot link generation for read-only web views.

use crate::view_export::{ViewExportMeta, ViewExportPayload};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotShareLink {
    pub snapshot_id: String,
    pub url: String,
}

#[must_use]
pub fn snapshot_id(meta: &ViewExportMeta, payload: &ViewExportPayload) -> String {
    let mut input = String::new();
    input.push_str(&meta.view_label);
    input.push('\n');
    input.push_str(&meta.mode_label);
    input.push('\n');
    input.push_str(&meta.generated_epoch_ms.to_string());
    input.push('\n');
    input.push_str(&payload.text);
    input.push('\n');
    input.push_str(&payload.html);
    input.push('\n');
    input.push_str(&payload.svg);

    let hash = fnv1a64(input.as_bytes());
    format!("snap-{hash:016x}")
}

pub fn build_share_url(
    base_url: &str,
    snapshot_id: &str,
    meta: &ViewExportMeta,
) -> Result<String, String> {
    let base_url = normalize_base_url(base_url)?;
    let snapshot_id = normalize_required(snapshot_id, "snapshot_id")?;
    let view = percent_encode(meta.view_label.trim());
    let mode = percent_encode(meta.mode_label.trim());

    let mut url = String::new();
    url.push_str(&base_url);
    url.push_str("/snapshots/");
    url.push_str(&snapshot_id);
    url.push_str("?view=");
    url.push_str(&view);
    url.push_str("&mode=");
    url.push_str(&mode);
    url.push_str("&generated=");
    url.push_str(&meta.generated_epoch_ms.to_string());
    url.push_str("&readonly=1");
    Ok(url)
}

pub fn build_snapshot_share_link(
    base_url: &str,
    meta: &ViewExportMeta,
    payload: &ViewExportPayload,
) -> Result<SnapshotShareLink, String> {
    let snapshot_id = snapshot_id(meta, payload);
    let url = build_share_url(base_url, &snapshot_id, meta)?;
    Ok(SnapshotShareLink { snapshot_id, url })
}

fn normalize_base_url(base_url: &str) -> Result<String, String> {
    let mut base_url = normalize_required(base_url, "base_url")?;
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err("base_url must start with http:// or https://".to_owned());
    }
    while base_url.ends_with('/') {
        base_url.pop();
    }
    Ok(base_url)
}

fn normalize_required(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value.to_owned())
    }
}

fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        let ch = *byte as char;
        let is_unreserved = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~');
        if is_unreserved {
            out.push(ch);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{build_share_url, build_snapshot_share_link, snapshot_id};
    use crate::view_export::{ViewExportMeta, ViewExportPayload};

    fn sample_meta() -> ViewExportMeta {
        ViewExportMeta {
            view_label: "Overview Main".to_owned(),
            mode_label: "Triage/Read-Only".to_owned(),
            generated_epoch_ms: 1_700_000_000_123,
        }
    }

    fn sample_payload() -> ViewExportPayload {
        ViewExportPayload {
            text: "frame text".to_owned(),
            html: "<pre>frame</pre>".to_owned(),
            svg: "<svg/>".to_owned(),
        }
    }

    #[test]
    fn snapshot_id_is_deterministic_for_same_payload() {
        let meta = sample_meta();
        let payload = sample_payload();
        let first = snapshot_id(&meta, &payload);
        let second = snapshot_id(&meta, &payload);
        assert_eq!(first, second);
        assert!(first.starts_with("snap-"));
        assert_eq!(first.len(), 21);
    }

    #[test]
    fn snapshot_id_changes_when_payload_changes() {
        let meta = sample_meta();
        let first = snapshot_id(&meta, &sample_payload());
        let mut modified = sample_payload();
        modified.text.push('!');
        let second = snapshot_id(&meta, &modified);
        assert_ne!(first, second);
    }

    #[test]
    fn share_url_normalizes_base_and_encodes_metadata() {
        let meta = sample_meta();
        let url = match build_share_url("https://forge.example.com///", "snap-0123", &meta) {
            Ok(url) => url,
            Err(err) => panic!("share url should build: {err}"),
        };
        assert_eq!(
            url,
            "https://forge.example.com/snapshots/snap-0123?view=Overview%20Main&mode=Triage%2FRead-Only&generated=1700000000123&readonly=1"
        );
    }

    #[test]
    fn share_url_rejects_invalid_base_url() {
        let err = match build_share_url("forge.example.com", "snap-0123", &sample_meta()) {
            Ok(url) => panic!("invalid base url should fail, got {url}"),
            Err(err) => err,
        };
        assert!(err.contains("http:// or https://"));
    }

    #[test]
    fn build_snapshot_share_link_returns_snapshot_and_url() {
        let link = match build_snapshot_share_link(
            "https://forge.example.com",
            &sample_meta(),
            &sample_payload(),
        ) {
            Ok(link) => link,
            Err(err) => panic!("snapshot share link should build: {err}"),
        };
        assert!(link.snapshot_id.starts_with("snap-"));
        assert!(link.url.contains("/snapshots/"));
        assert!(link.url.contains(&link.snapshot_id));
        assert!(link.url.contains("readonly=1"));
    }
}
