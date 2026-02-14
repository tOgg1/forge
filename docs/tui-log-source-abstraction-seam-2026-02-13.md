# TUI log source abstraction seam (forge-apf)

Date: 2026-02-13  
Task: `forge-apf`

## Scope

Introduce a stable seam so future log providers (`parsed`/`diff`/`pty`) can be added without rewriting app state flow.

## Changes

- Added new seam module:
  - `crates/forge-tui/src/log_source_abstraction.rs`
- Exported seam module from crate root:
  - `crates/forge-tui/src/lib.rs`
- Added app-level route mapping API:
  - `App::current_log_route()` in `crates/forge-tui/src/app.rs`
  - Maps existing `LogSource` + `LogLayer` into seam route:
    - transport: `live` / `latest-run` / `selected-run`
    - content: `parsed` / `diff` (with `pty` reserved as future mode)

## Validation

Commands run:

```bash
cargo check -p forge-tui
cargo test -p forge-tui route_key_includes_transport_and_content -- --nocapture
cargo test -p forge-tui current_log_route_maps_ -- --nocapture
```

Observed:

- `cargo check -p forge-tui`: PASS at capture time.
- New seam tests: PASS.
- Later `cargo build -p forge-tui` failed due concurrent churn in `crates/forge-tui/src/app.rs` (missing methods unrelated to seam: `rendered_log_lines` / `collect_regex_match_indices`).
