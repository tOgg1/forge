# TUI-911 live loops table: unique ID prefix highlighting + resolver parity

Task: `forge-6fe`  
Status: delivered

## What shipped

- Added a live loops block at top of Overview pane with compact, high-signal rows:
  - selection marker
  - shortest unique ID prefix emphasis (`[prefix]suffix`)
  - state, queue depth, run count, name
- Added shortest-unique-prefix derivation for displayed loop IDs.
- Added resolver parity bridge by exposing CLI resolver API:
  - `crates/forge-cli/src/queue.rs`: `resolve_loop_ref` is now public.
- Added Overview-specific regression coverage for:
  - unique-prefix length derivation
  - unique-prefix highlighting format
  - snapshot with live loops section.

## Semantics

- Prefix derivation follows shortest unique token logic across currently visible loop IDs.
- Resolver semantics are aligned with Forge CLI resolver behavior (short ID/full ID/name/prefix ambiguity messages).

## UX impact

- Operator can scan fleet identity faster without full-ID noise.
- Immediate visual disambiguation when IDs share long common prefixes.
- Overview gains command-center density while preserving selected-loop deep detail below.

## Files

- `crates/forge-tui/src/overview_tab.rs`
- `crates/forge-cli/src/queue.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy -p forge-tui --all-targets -- -D warnings`
- `cargo test -p forge-tui`
- `cargo test -p forge-cli`
