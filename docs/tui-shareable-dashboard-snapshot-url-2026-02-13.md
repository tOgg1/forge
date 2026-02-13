# Shareable Dashboard Snapshot URL (forge-rdh)

Date: 2026-02-13
Task: `forge-rdh`

## Scope implemented

Added `crates/forge-tui/src/dashboard_snapshot_share.rs` and exported it from `crates/forge-tui/src/lib.rs`.

Implemented read-only snapshot link primitives:

- `snapshot_id(meta, payload)`: deterministic `snap-<hex>` fingerprint across text/html/svg payloads and export metadata.
- `build_share_url(base_url, snapshot_id, meta)`: normalized URL builder with percent-encoded metadata query params.
- `build_snapshot_share_link(base_url, meta, payload)`: one-shot helper returning `{ snapshot_id, url }`.
- `SnapshotShareLink` value type for panel/action wiring.

Safety/consistency behavior:

- rejects non-http(s) base URLs
- trims trailing slashes from base URL
- enforces required snapshot IDs
- emits explicit `readonly=1` query flag

## Test coverage added

In `dashboard_snapshot_share::tests`:

- deterministic snapshot ID for identical payloads
- snapshot ID drift when payload changes
- base URL normalization + percent-encoding behavior
- invalid base URL rejection
- full link construction includes generated snapshot ID

## Validation

Executed:

- `cargo fmt --all -- crates/forge-tui/src/dashboard_snapshot_share.rs crates/forge-tui/src/lib.rs`
- `cargo test -p forge-tui dashboard_snapshot_share::tests:: -- --nocapture`
